//! Prepara el repositorio, los datos de la app y los binarios del ejemplo.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use uuid::Uuid;

use super::scenario;

/// Espacio con id fijo para usar el agente principal por omisión (ECO-8).
pub const ELEPHANT: &str = "e00000000";

/// Todo lo que el ejemplo crea y debe limpiar al terminar.
pub struct Layout {
    /// Carpeta temporal única, se borra al terminar.
    pub root: PathBuf,
    /// Repositorio base (rama a la que se integra).
    pub repo: PathBuf,
    /// Datos privados de la app (`%APPDATA%\EzAgent\hub` simulado).
    pub data: PathBuf,
    /// Carpeta con `ez.exe` y `fake-ws-agent.exe`.
    pub bins: PathBuf,
    /// Id del espacio.
    pub space: String,
    /// Token del agente principal.
    pub token: String,
}

/// Ejecuta un programa con argumentos en lista, sin shell y sin consola.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = command("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|error| format!("no pude ejecutar git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} falló: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("no pude crear {}: {error}", parent.display()))?;
    }
    std::fs::write(path, text)
        .map_err(|error| format!("no pude escribir {}: {error}", path.display()))
}

fn binary_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn target_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        if !dir.is_empty() {
            return Ok(PathBuf::from(dir));
        }
    }
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    exe.parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "no pude localizar la carpeta target".to_string())
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .to_path_buf()
}

fn ensure_binary(dir: &Path, package: &str, name: &str) -> Result<PathBuf, String> {
    let path = dir.join(binary_name(name));
    if path.is_file() {
        return Ok(path);
    }
    let status = command("cargo")
        .current_dir(workspace_root())
        .args(["build", "-p", package, "--bin", name])
        .status()
        .map_err(|error| format!("no pude compilar {name}: {error}"))?;
    if !status.success() || !path.is_file() {
        return Err(format!("no se pudo compilar el binario {name}"));
    }
    Ok(path)
}

fn profiles(fake: &Path, script: &Path) -> String {
    let fake = scenario::quoted(&fake.to_string_lossy());
    let script = scenario::quoted(&script.to_string_lossy());
    format!(
        "[[agente]]\nid = \"fake_main\"\nnombre = \"Principal falso\"\nargv = [{fake}, \"--main\", {script}]\nmodelos = [\"main-1\"]\nfamilia = \"main\"\n\
         [[agente]]\nid = \"fake_exec\"\nnombre = \"Ejecutor falso\"\nargv = [{fake}, \"--exec\"]\nmodelos = [\"exec-1\"]\nfamilia = \"exec\"\n\
         [[agente]]\nid = \"fake_verify\"\nnombre = \"Verificador falso\"\nargv = [{fake}, \"--verify\"]\nmodelos = [\"verify-1\"]\nfamilia = \"verify\"\n"
    )
}

/// Crea todo lo necesario; devuelve error si falta `hc` o no se pueden compilar los binarios.
pub fn prepare() -> Result<Layout, String> {
    let bins = target_dir()?;
    let ez = ensure_binary(&bins, "ez-ctl", "ez")?;
    if !ez.is_file() {
        return Err("falta el binario ez".to_string());
    }
    let fake = ensure_binary(&bins, "ez-fakes", "fake-ws-agent")?;

    let root = std::env::temp_dir().join(format!("ez-e2e-fake-{}", Uuid::new_v4().simple()));
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let repo = root.join("repo");
    let data = root.join("datos");
    let worktree = root.join("wt");
    let script = root.join("guion.toml");
    let space = format!("e2e{}", &Uuid::new_v4().simple().to_string()[..8]);
    let token = ez_ctl::new_token();

    std::fs::create_dir_all(&repo).map_err(|error| error.to_string())?;
    write(&repo.join("README.md"), "Repositorio de prueba ECO-8.\n")?;
    git(&repo, &["init", "-q"])?;
    git(&repo, &["config", "user.email", "e2e@ezagent.local"])?;
    git(&repo, &["config", "user.name", "EzAgent E2E"])?;
    git(&repo, &["add", "README.md"])?;
    git(&repo, &["commit", "-q", "-m", "inicial"])?;
    let base_branch = git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let branch = format!("ez/{ELEPHANT}");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &branch,
            &worktree.to_string_lossy(),
        ],
    )?;

    write(&script, &scenario::script())?;
    write(&data.join("espacios.json"), &spaces(&space, &repo))?;
    write(&data.join("agentes.toml"), &profiles(&fake, &script))?;
    write(
        &data.join("suscripciones.toml"),
        "[[suscripcion]]\nnombre = \"Falsos\"\nagentes = [\"fake_main\", \"fake_exec\", \"fake_verify\"]\n",
    )?;
    let agents = serde_json::json!({
        "agents": [{
            "key": { "space": space, "elephant": ELEPHANT },
            "name": "Principal falso",
            "worktree": worktree,
            "branch": branch,
            "base_branch": base_branch,
        }]
    });
    write(&data.join("agentes.json"), &agents.to_string())?;
    let tokens = serde_json::json!({ ELEPHANT: token });
    write(&data.join("elefantes.json"), &tokens.to_string())?;
    let choices = serde_json::json!({
        ELEPHANT: { "agente": "fake_main", "modelo": "main-1" }
    });
    write(&data.join("principales.json"), &choices.to_string())?;

    Ok(Layout {
        root,
        repo,
        data,
        bins,
        space,
        token,
    })
}

fn spaces(space: &str, repo: &Path) -> String {
    serde_json::json!({
        "espacios": [{
            "id": space,
            "nombre": "Prueba punta a punta",
            "carpeta": repo,
        }]
    })
    .to_string()
}

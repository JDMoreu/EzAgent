//! La preparación de la prueba: binarios, carpetas temporales y configuración.
//!
//! Todo lo que la prueba toca vive bajo `std::env::temp_dir()`: la
//! configuración de EzAgent (`APPDATA`), la de mcode (`MINIMAX_DATA_DIR`), la
//! de Codex (`CODEX_HOME`), la de OpenCode (`OPENCODE_CONFIG_DIR`), el estado
//! de OpenCode (`LOCALAPPDATA`) y el registro de subagentes
//! (`EZ_SUBAGENTS_DIR`). La configuración real del usuario no se lee ni se
//! escribe.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::proc::{self, EnvVars};

/// El modelo de Codex del catálogo de la prueba y su nivel de esfuerzo.
pub const CODEX_MODEL: &str = "gpt-5-codex";
/// El nivel de esfuerzo de Codex que se usa en la prueba.
pub const CODEX_EFFORT: &str = "medium";
/// El modelo de mcode del `config.yaml` de la prueba.
pub const MCODE_MODEL: &str = "minimax/MiniMax-M3.1-Flash";
/// El nombre del modelo de Codex que no está en el caché.
pub const CODEX_MISSING: &str = "gpt-9-imaginario";
/// El nombre del modelo de mcode que no está en el `config.yaml`.
pub const MCODE_MISSING: &str = "minimax/modelo-que-no-existe";

/// La versión de los perfiles de fábrica (`ez-core`).
const PROFILES_VERSION: i64 = 8;

/// Todo lo que la prueba necesita para correr.
pub struct Sandbox {
    /// El identificador de la corrida (`ez-e2e-<id>`).
    pub id: String,
    /// La carpeta de trabajo de la prueba.
    pub root: PathBuf,
    /// La carpeta del proyecto (un repositorio de git).
    pub work: PathBuf,
    /// La carpeta donde los agentes falsos guardan su registro.
    pub registros: PathBuf,
    /// La carpeta con los encargos.
    pub briefs: PathBuf,
    /// El binario `ez`.
    pub ez: PathBuf,
    /// El agente falso.
    pub fake: PathBuf,
    /// El binario `hc`.
    pub hc: PathBuf,
    /// El binario `herdr-app`.
    pub app: PathBuf,
    /// El propio ejemplo: hace de lanzador de `ez` para el agente falso.
    pub launcher: PathBuf,
    /// El entorno de los subagentes (todo temporal).
    pub env: EnvVars,
}

/// Por qué no se puede preparar la prueba.
pub enum SetupError {
    /// Falta un binario o la ventana de Herdr no responde (código 2).
    Missing(String),
}

impl SetupError {
    /// El mensaje para la persona.
    pub fn message(&self) -> &str {
        match self {
            Self::Missing(message) => message,
        }
    }
}

/// El binario de la prueba: el del `target` del ejemplo o el del PATH.
fn find_binary(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut folders: Vec<PathBuf> = Vec::new();
    if let Some(folder) = exe.parent() {
        folders.push(folder.to_path_buf());
        if let Some(parent) = folder.parent() {
            folders.push(parent.to_path_buf());
        }
    }
    for folder in folders {
        for candidate in [folder.join(name), folder.join(format!("{name}.exe"))] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    proc::which(name)
}

/// Prepara la prueba: binarios, carpetas y configuración temporal.
pub fn prepare() -> Result<Sandbox, SetupError> {
    let id = format!("ez-e2e-{}", std::process::id());
    let root = std::env::temp_dir().join(&id);
    let work = root.join("work");
    let registros = root.join("registros");
    let briefs = root.join("briefs");
    let launcher = std::env::current_exe()
        .map_err(|error| SetupError::Missing(format!("No sé dónde está el ejemplo: {error}")))?;
    let ez = find_binary("ez").ok_or_else(|| {
        SetupError::Missing(
            "No encuentro el binario `ez`. Compílalo con: cargo build -p ez-ctl --bin ez"
                .to_string(),
        )
    })?;
    let fake = find_binary("fake-ws-agent").ok_or_else(|| {
        SetupError::Missing(
            "No encuentro el binario `fake-ws-agent`. Compílalo con: \
             cargo build -p ez-fakes --bin fake-ws-agent"
                .to_string(),
        )
    })?;
    let hc = proc::which("hc").ok_or_else(|| {
        SetupError::Missing("No encuentro `hc` (herdr-core) en el PATH.".to_string())
    })?;
    let app = proc::which("herdr-app")
        .ok_or_else(|| SetupError::Missing("No encuentro `herdr-app` en el PATH.".to_string()))?;
    let folders = [&work, &registros, &briefs];
    for folder in folders {
        std::fs::create_dir_all(folder).map_err(|error| {
            SetupError::Missing(format!("No pude crear {}: {error}", folder.display()))
        })?;
    }
    let mut env = EnvVars::default();
    env.set(
        "APPDATA",
        root.join("appdata").to_string_lossy().into_owned(),
    );
    env.set(
        "LOCALAPPDATA",
        root.join("localappdata").to_string_lossy().into_owned(),
    );
    env.set(
        "MINIMAX_DATA_DIR",
        root.join("minimax").to_string_lossy().into_owned(),
    );
    env.set(
        "CODEX_HOME",
        root.join("codex").to_string_lossy().into_owned(),
    );
    env.set(
        "OPENCODE_CONFIG_DIR",
        root.join("opencode-config").to_string_lossy().into_owned(),
    );
    env.set(
        "EZ_SUBAGENTS_DIR",
        root.join("subagentes").to_string_lossy().into_owned(),
    );
    env.set("EZ_HC_PATH", hc.to_string_lossy().into_owned());
    // DECISION: el entorno del subproceso no lleva claves de API: la prueba no
    // habla con ningún modelo y no las necesita.
    for name in [
        "EZ_URL",
        "EZ_TOKEN",
        "MINIMAX_API_KEY",
        "OPENCODE_API_KEY",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
    ] {
        env.remove(name);
    }
    let sandbox = Sandbox {
        id,
        root,
        work,
        registros,
        briefs,
        ez,
        fake,
        hc,
        app,
        launcher,
        env,
    };
    write_config(&sandbox)?;
    write_mcode_config(&sandbox)?;
    write_codex_cache(&sandbox)?;
    init_repo(&sandbox)?;
    write_proxy(&sandbox)?;
    Ok(sandbox)
}

/// La ruta del agente falso con barras normales.
///
/// DECISION: `ez-herdr` envuelve en PowerShell el comando de los agentes que
/// llevan variables de entorno (OpenCode) y cita el programa entre comillas
/// simples cuando su ruta no es «simple». En PowerShell, un programa citado no
/// se puede ejecutar con argumentos sin el operador `&`, así que el
/// envoltorio no arranca (`text.rs::command_line`). Con barras normales la
/// ruta sí es «simple» y el envoltorio funciona; el producto queda igual que
/// con el catálogo de fábrica, donde los comandos son nombres sueltos.
fn fake_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// El `voice.json` de la prueba: los agentes falsos y las rutas de Herdr.
fn write_config(sandbox: &Sandbox) -> Result<(), SetupError> {
    let mut agents = Map::new();
    for kind in [
        "claude", "opencode", "mcode", "minimax", "codex", "pi", "qwen",
    ] {
        let mut command = vec![
            fake_path(&sandbox.fake),
            "--echo".to_string(),
            sandbox.registros.to_string_lossy().into_owned(),
        ];
        if kind == "claude" {
            // DECISION: el agente de tipo `claude` declara su estado (§13); el
            // lanzador es el propio ejemplo, que llama a `ez` con el entorno
            // temporal (el de `hc run` no lleva variables de entorno).
            command.push("--report".to_string());
            command.push("terminado".to_string());
            command.push("--ez".to_string());
            command.push(sandbox.launcher.to_string_lossy().into_owned());
        }
        command.push("--as".to_string());
        command.push(kind.to_string());
        agents.insert(
            kind.to_string(),
            Value::Array(command.into_iter().map(Value::String).collect()),
        );
    }
    let mut config = Map::new();
    config.insert("agents".to_string(), Value::Object(agents));
    config.insert("hc_path".to_string(), json!(sandbox.hc.to_string_lossy()));
    config.insert(
        "herdr_app_path".to_string(),
        json!(sandbox.app.to_string_lossy()),
    );
    config.insert(
        "agent_profiles_version".to_string(),
        json!(PROFILES_VERSION),
    );
    let folder = sandbox.root.join("appdata").join("EzAgent");
    std::fs::create_dir_all(&folder).map_err(|error| {
        SetupError::Missing(format!("No pude crear {}: {error}", folder.display()))
    })?;
    let path = folder.join("voice.json");
    let text = serde_json::to_string_pretty(&Value::Object(config))
        .map_err(|error| SetupError::Missing(format!("No pude preparar el voice.json: {error}")))?;
    std::fs::write(&path, text).map_err(|error| {
        SetupError::Missing(format!("No pude escribir {}: {error}", path.display()))
    })
}

/// El `config.yaml` de mcode de la prueba: un proveedor, un modelo y el bloque
/// de esfuerzo (el formato de las pruebas de `ez-herdr`).
fn write_mcode_config(sandbox: &Sandbox) -> Result<(), SetupError> {
    let folder = sandbox.root.join("minimax");
    std::fs::create_dir_all(&folder).map_err(|error| {
        SetupError::Missing(format!("No pude crear {}: {error}", folder.display()))
    })?;
    let path = folder.join("config.yaml");
    let text = "provider:\n  minimax:\n    api_key: clave-de-prueba\n    models:\n      \
                MiniMax-M3.1-Flash:\n        context: 200k\ndefaultModelThinking:\n  effort: \
                default\n";
    std::fs::write(&path, text).map_err(|error| {
        SetupError::Missing(format!("No pude escribir {}: {error}", path.display()))
    })
}

/// El `models_cache.json` de Codex de la prueba: un modelo visible con sus
/// niveles y otro escondido.
fn write_codex_cache(sandbox: &Sandbox) -> Result<(), SetupError> {
    let folder = sandbox.root.join("codex");
    std::fs::create_dir_all(&folder).map_err(|error| {
        SetupError::Missing(format!("No pude crear {}: {error}", folder.display()))
    })?;
    let path = folder.join("models_cache.json");
    let data = json!({
        "models": [
            {
                "slug": CODEX_MODEL,
                "visibility": "list",
                "supported_reasoning_levels": [
                    {"effort": "low"},
                    {"effort": CODEX_EFFORT},
                    {"effort": "high"}
                ]
            },
            {
                "slug": "modelo-oculto",
                "visibility": "hide",
                "supported_reasoning_levels": ["high"]
            }
        ]
    });
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&data).unwrap_or_default(),
    )
    .map_err(|error| SetupError::Missing(format!("No pude escribir {}: {error}", path.display())))
}

/// La carpeta de trabajo: un repositorio de git con un commit.
fn init_repo(sandbox: &Sandbox) -> Result<(), SetupError> {
    let file = sandbox.work.join("README.md");
    std::fs::write(&file, "# Prueba de punta a punta\n").map_err(|error| {
        SetupError::Missing(format!("No pude escribir {}: {error}", file.display()))
    })?;
    for args in [
        vec!["init".to_string()],
        vec!["add".to_string(), ".".to_string()],
        vec![
            "-c".to_string(),
            "user.email=prueba@ezagent.local".to_string(),
            "-c".to_string(),
            "user.name=Prueba EzAgent".to_string(),
            "commit".to_string(),
            "-m".to_string(),
            "prueba de punta a punta".to_string(),
        ],
    ] {
        let output = proc::run(
            &sandbox.ez_git(),
            &args,
            &EnvVars::default(),
            Some(&sandbox.work),
        );
        if !output.ok() {
            return Err(SetupError::Missing(format!(
                "git {} falló: {}",
                args.join(" "),
                output.message()
            )));
        }
    }
    Ok(())
}

/// El `git` del sistema.
impl Sandbox {
    /// La ruta de `git`.
    fn ez_git(&self) -> PathBuf {
        proc::which("git").unwrap_or_else(|| PathBuf::from("git"))
    }
}

/// La configuración del lanzador: qué binario es `ez` y con qué entorno corre.
fn write_proxy(sandbox: &Sandbox) -> Result<(), SetupError> {
    let mut env = Map::new();
    for (name, value) in &sandbox.env.pairs {
        env.insert(name.clone(), Value::String(value.clone()));
    }
    let data = json!({
        "ez": sandbox.ez.to_string_lossy(),
        "env": Value::Object(env),
    });
    let path = sandbox.root.join("proxy.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&data).unwrap_or_default(),
    )
    .map_err(|error| SetupError::Missing(format!("No pude escribir {}: {error}", path.display())))
}

/// El archivo de un encargo, con su marca única.
pub fn write_brief(sandbox: &Sandbox, index: usize) -> Result<(PathBuf, String), SetupError> {
    let mark = format!("E2E-{}-{}", sandbox.id.trim_start_matches("ez-e2e-"), index);
    let path = sandbox.briefs.join(format!("{index}.md"));
    let text =
        format!("Trabaja en {mark}: revisa la carpeta del proyecto y escribe un informe corto.\n");
    std::fs::write(&path, text).map_err(|error| {
        SetupError::Missing(format!("No pude escribir {}: {error}", path.display()))
    })?;
    Ok((path, mark))
}

/// La ruta del archivo JSON con el resultado de la prueba.
pub fn result_path(sandbox: &Sandbox) -> PathBuf {
    std::env::temp_dir().join(format!("{}.json", sandbox.id))
}

/// Borra la carpeta de la prueba (solo si es de la prueba).
pub fn remove_root(sandbox: &Sandbox) {
    let temp = std::env::temp_dir();
    if sandbox.root.parent() == Some(temp.as_path()) && sandbox.root.exists() {
        let _ = std::fs::remove_dir_all(&sandbox.root);
    }
}

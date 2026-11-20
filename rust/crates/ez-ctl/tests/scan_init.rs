//! `ez init`, `ez resources` e `ez inventory --local` con `APPDATA` temporal y
//! un `PATH` con agentes falsos (un `.cmd` y un guion sin extensión que apunta
//! a un `.exe` falso). Nunca abre un agente: el `.exe` falso es el propio `ez`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos());
        let path =
            std::env::temp_dir().join(format!("ez-scan-e2e-{}-{id}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).expect("carpeta temporal");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("carpeta");
    }
    std::fs::write(path, text).expect("archivo");
}

/// Corre `ez` con el entorno falso y devuelve (código, salida, error).
fn run(root: &Path, args: &[&str]) -> (i32, String, String) {
    let bin = root.join("bin");
    let mut command = Command::new(env!("CARGO_BIN_EXE_ez"));
    command
        .args(args)
        .current_dir(root)
        .env("APPDATA", root.join("appdata"))
        .env("USERPROFILE", root)
        .env("PATH", &bin)
        .env("MINIMAX_DATA_DIR", root.join("minimax"))
        .env("CODEX_HOME", root.join("codex"))
        .env("OPENCODE_CONFIG_DIR", root.join("opencode"))
        .env_remove("HC_TERMINAL_ID")
        .env_remove("EZ_URL")
        .env_remove("EZ_TOKEN");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().expect("ejecutar ez");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Prepara un `APPDATA`/`PATH` falsos con qwen (guion sin extensión → `.exe`)
/// y codex (`.cmd`).
fn setup() -> TempDir {
    let root = TempDir::new();
    let bin = root.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    std::fs::copy(env!("CARGO_BIN_EXE_ez"), bin.join("fakeqwen.exe")).expect("copiar ez");
    write(
        &bin.join("fakeqwen"),
        "#!/bin/sh\nexec \"$basedir/fakeqwen.exe\" \"$@\"\n",
    );
    write(&bin.join("fakecodex.cmd"), "@ECHO off\n");
    let voice = serde_json::json!({
        "agents": {"qwen": ["fakeqwen"], "codex": ["fakecodex"]}
    });
    write(
        &root
            .path()
            .join("appdata")
            .join("EzAgent")
            .join("voice.json"),
        &serde_json::to_string_pretty(&voice).expect("json"),
    );
    root
}

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text.trim()).unwrap_or(serde_json::Value::Null)
}

#[test]
fn init_escribe_entorno_con_los_agentes_falsos() {
    let root = setup();
    let (code, out, err) = run(root.path(), &["init", "--json"]);
    assert_eq!(code, 0, "salida: {out}\nerror: {err}");
    let data = json(&out);
    assert_eq!(data["ok"], true);
    let path = root
        .path()
        .join("appdata")
        .join("EzAgent")
        .join("entorno.json");
    assert!(path.is_file(), "falta {}", path.display());
    let saved = json(&std::fs::read_to_string(&path).expect("archivo"));
    assert_eq!(saved["version"], 1);
    assert!(saved["system"]["total_ram_mb"].as_u64().unwrap_or(0) > 0);
    let agents = saved["agents"].as_array().expect("lista de agentes");
    let qwen = agents
        .iter()
        .find(|agent| agent["key"] == "qwen")
        .expect("qwen");
    assert_eq!(qwen["installed"], true);
    let native = qwen["path"].as_str().unwrap_or_default().to_lowercase();
    assert!(native.ends_with("fakeqwen.exe"), "ruta nativa: {native}");
    assert_eq!(qwen["version"], env!("CARGO_PKG_VERSION"));
    let codex = agents
        .iter()
        .find(|agent| agent["key"] == "codex")
        .expect("codex");
    assert_eq!(codex["installed"], true);
    assert!(codex["path"]
        .as_str()
        .unwrap_or_default()
        .ends_with("fakecodex.cmd"));
    let usage = saved["usage"].as_array().expect("uso por agente");
    for key in ["qwen", "codex"] {
        let agent = usage
            .iter()
            .find(|agent| agent["key"] == key)
            .unwrap_or_else(|| panic!("falta el uso de {key}: {usage:?}"));
        assert!(agent["state"].is_string(), "{agent}");
        assert!(
            agent["window_hours"].as_f64().unwrap_or(0.0) > 0.0,
            "{agent}"
        );
        assert!(
            !agent["source"].as_str().unwrap_or_default().is_empty(),
            "{agent}"
        );
        assert!(agent.get("tokens").is_none(), "sin tokens: {agent}");
    }
}

#[test]
fn recommend_respeta_un_modelo_con_punto_sin_comillas() {
    let root = setup();
    write(
        &root
            .path()
            .join("appdata")
            .join("EzAgent")
            .join("modelos.toml"),
        "[qwen3.coder-plus]\nagente = \"qwen\"\nnivel = \"alto\"\n\
         costo = \"gratis\"\nfamilia = \"qwen\"\n",
    );
    let (code, out, err) = run(root.path(), &["recommend", "planner", "--json"]);
    assert_eq!(code, 0, "salida: {out}\nerror: {err}");
    let data = json(&out);
    assert_eq!(data["ok"], true);
    assert_eq!(data["recommendation"]["agent"], "qwen");
    assert_eq!(data["recommendation"]["model"], "qwen3.coder-plus");
    assert!(!err.contains("modelos.toml"), "aviso falso: {err}");
}

#[test]
fn resources_e_inventory_funcionan_sin_herdr_y_sin_arbitro() {
    let root = setup();
    let (code, out, err) = run(root.path(), &["resources", "--json"]);
    assert_eq!(code, 0, "salida: {out}\nerror: {err}");
    let data = json(&out);
    assert_eq!(data["ok"], true);
    assert!(data["resources"]["free_ram_mb"].as_u64().unwrap_or(0) > 0);
    assert!(data["resources"]["free_commit_mb"].as_u64().unwrap_or(0) > 0);
    assert!(data["resources"]["disk_free_mb"].as_u64().unwrap_or(0) > 0);
    assert_exact_empty(data["resources"]["subagents"].as_array());

    let (code, out, err) = run(root.path(), &["inventory", "--local", "--json"]);
    assert_eq!(code, 0, "salida: {out}\nerror: {err}");
    let data = json(&out);
    assert_eq!(data["ok"], true);
    let agents = data["agents"].as_array().expect("agentes");
    assert!(agents.iter().any(|agent| agent["key"] == "qwen"));
    assert!(agents.iter().any(|agent| agent["key"] == "codex"));
}

#[test]
fn recommend_usa_los_agentes_falsos_y_el_modelos_toml() {
    let root = setup();
    write(
        &root
            .path()
            .join("appdata")
            .join("EzAgent")
            .join("modelos.toml"),
        "[qwen3-coder-plus]\nnivel = \"alto\"\ncosto = \"gratis\"\nfamilia = \"qwen\"\n",
    );
    let (code, out, err) = run(root.path(), &["recommend", "planner", "--json"]);
    assert_eq!(code, 0, "salida: {out}\nerror: {err}");
    let data = json(&out);
    assert_eq!(data["ok"], true);
    assert_eq!(data["recommendation"]["agent"], "qwen");
    assert_eq!(data["recommendation"]["model"], "qwen3-coder-plus");
    assert_eq!(data["recommendation"]["level"], "alto");
    assert_eq!(data["recommendation"]["cost"], "gratis");
    assert_eq!(data["recommendation"]["effort"], serde_json::Value::Null);
}

fn assert_exact_empty(items: Option<&Vec<serde_json::Value>>) {
    assert!(items.is_some_and(Vec::is_empty));
}

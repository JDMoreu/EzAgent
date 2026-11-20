//! The sandbox of the test: temporary user folders, fake agents and config.
//!
//! Everything lives under `std::env::temp_dir()`; the real configuration and
//! the real usage data of the person are never read or written.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::proc::{self, EnvVars};

/// The Codex model declared in the test cache.
pub const CODEX_MODEL: &str = "gpt-5-codex";
/// The mcode model declared in the test `config.yaml` and SQLite.
pub const MCODE_MODEL: &str = "MiniMax-M3.1-Flash";
/// The OpenCode Go model used in the window.
pub const OPENCODE_GO_MODEL: &str = "deepseek-v4.1-flash";
/// The OpenCode free model named by the Free notice.
pub const OPENCODE_FREE_MODEL: &str = "hy4-flash-free";

/// The version of the factory profiles (`ez-core`).
const PROFILES_VERSION: i64 = 8;

/// Everything the test needs to run.
pub struct Sandbox {
    /// Run identifier (`ez-ir4-<pid>-<n>`).
    pub id: String,
    /// Temporary root of the run.
    pub root: PathBuf,
    /// Working folder of the tests.
    pub work: PathBuf,
    /// Where the fake agents write what they receive.
    pub registros: PathBuf,
    /// The brief files.
    pub briefs: PathBuf,
    /// `%APPDATA%` of the test (EzAgent lives inside).
    pub appdata: PathBuf,
    /// `%USERPROFILE%`/`HOME` of the test.
    pub home: PathBuf,
    /// `%LOCALAPPDATA%` of the test.
    pub local: PathBuf,
    /// `CODEX_HOME` of the test.
    pub codex: PathBuf,
    /// `MINIMAX_DATA_DIR` of the test.
    pub minimax: PathBuf,
    /// `OPENCODE_CONFIG_DIR` of the test.
    pub opencode_config: PathBuf,
    /// `EZ_SUBAGENTS_DIR` of the test.
    pub subagentes: PathBuf,
    /// Folder with one fake executable per agent type.
    pub bin: PathBuf,
    /// The compiled `ez` binary.
    pub ez: PathBuf,
    /// The compiled `fake-ws-agent` binary.
    pub fake: PathBuf,
    /// The real `hc` binary.
    pub hc: PathBuf,
    /// The real `herdr-app`, if it is installed.
    pub app: Option<PathBuf>,
    /// Environment of every `ez` subprocess.
    pub env: EnvVars,
}

/// Finds a compiled binary next to the example or in the PATH.
fn find_binary(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut folders = Vec::new();
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

/// Creates the temporary sandbox and its configuration.
pub fn prepare() -> Result<Sandbox, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_nanos() % 100_000)
        .unwrap_or(0);
    let id = format!("ez-ir4-{}-{nanos}", std::process::id());
    let root = std::env::temp_dir().join(&id);
    if root.exists() {
        return Err(format!("la carpeta {} ya existe", root.display()));
    }
    let work = root.join("work");
    let registros = root.join("registros");
    let briefs = root.join("briefs");
    let appdata = root.join("appdata");
    let home = root.join("home");
    let local = root.join("localappdata");
    let codex = root.join("codex");
    let minimax = root.join("minimax");
    let opencode_config = root.join("opencode-config");
    let subagentes = root.join("subagentes");
    let bin = root.join("bin");
    for folder in [
        &work,
        &registros,
        &briefs,
        &appdata,
        &home,
        &local,
        &codex,
        &minimax,
        &opencode_config,
        &subagentes,
        &bin,
    ] {
        std::fs::create_dir_all(folder)
            .map_err(|error| format!("No pude crear {}: {error}", folder.display()))?;
    }
    let ez = find_binary("ez").ok_or_else(|| {
        "No encuentro el binario `ez`. Compílalo con: cargo build -p ez-ctl --bin ez".to_string()
    })?;
    let fake = find_binary("fake-ws-agent").ok_or_else(|| {
        "No encuentro `fake-ws-agent`. Compílalo con: \
         cargo build -p ez-fakes --bin fake-ws-agent"
            .to_string()
    })?;
    let hc = proc::which("hc")
        .ok_or_else(|| "No encuentro `hc` (herdr-core) en el PATH.".to_string())?;
    let app = proc::which("herdr-app");
    let sandbox = Sandbox {
        id,
        root,
        work,
        registros,
        briefs,
        appdata,
        home,
        local,
        codex,
        minimax,
        opencode_config,
        subagentes,
        bin,
        ez,
        fake,
        hc,
        app,
        env: EnvVars::default(),
    };
    copy_fakes(&sandbox)?;
    let mut sandbox = sandbox;
    sandbox.env = environment(&sandbox);
    write_config(&sandbox)?;
    write_modelos(&sandbox)?;
    write_mcode_config(&sandbox)?;
    write_codex_cache(&sandbox)?;
    Ok(sandbox)
}

/// Copies `fake-ws-agent` once per agent type so `ez usage` does not dedupe
/// them by program.
fn copy_fakes(sandbox: &Sandbox) -> Result<(), String> {
    for key in ["claude", "opencode", "mcode", "codex", "pi", "qwen"] {
        let target = sandbox.bin.join(format!("{key}.exe"));
        std::fs::copy(&sandbox.fake, &target).map_err(|error| {
            format!(
                "No pude copiar {} a {}: {error}",
                sandbox.fake.display(),
                target.display()
            )
        })?;
    }
    Ok(())
}

/// The environment of every `ez` subprocess: temporary user folders and the
/// fake agents first in the PATH.
fn environment(sandbox: &Sandbox) -> EnvVars {
    let mut env = EnvVars::default();
    env.set("APPDATA", path_text(&sandbox.appdata));
    env.set("USERPROFILE", path_text(&sandbox.home));
    env.set("HOME", path_text(&sandbox.home));
    env.set("LOCALAPPDATA", path_text(&sandbox.local));
    env.set("CODEX_HOME", path_text(&sandbox.codex));
    env.set("MINIMAX_DATA_DIR", path_text(&sandbox.minimax));
    env.set("OPENCODE_CONFIG_DIR", path_text(&sandbox.opencode_config));
    env.set("EZ_SUBAGENTS_DIR", path_text(&sandbox.subagentes));
    env.set("EZ_HC_PATH", path_text(&sandbox.hc));
    // DECISION: the rest of the PATH is kept so `hc` can start PowerShell and
    // the real Herdr helpers; every catalog entry points to a fake, so no real
    // agent can be launched through the product.
    let mut folders = vec![sandbox.bin.clone()];
    if let Some(original) = std::env::var_os("PATH") {
        folders.extend(std::env::split_paths(&original));
    }
    if let Ok(joined) = std::env::join_paths(folders) {
        env.set("PATH", joined.to_string_lossy().into_owned());
    }
    for name in [
        "EZ_URL",
        "EZ_TOKEN",
        "HC_TERMINAL_ID",
        "MINIMAX_API_KEY",
        "OPENCODE_API_KEY",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
    ] {
        env.remove(name);
    }
    env
}

/// The `voice.json` of the test: fake agents, real binary paths and generous
/// resource thresholds.
fn write_config(sandbox: &Sandbox) -> Result<(), String> {
    let mut agents = Map::new();
    for key in ["claude", "opencode", "mcode"] {
        agents.insert(key.to_string(), fake_command(sandbox, key));
    }
    agents.insert("minimax".to_string(), fake_command(sandbox, "mcode"));
    for key in ["codex", "pi", "qwen"] {
        agents.insert(key.to_string(), fake_command(sandbox, key));
    }
    let mut config = Map::new();
    config.insert("agents".to_string(), Value::Object(agents));
    config.insert("hc_path".to_string(), json!(path_text(&sandbox.hc)));
    if let Some(app) = &sandbox.app {
        config.insert("herdr_app_path".to_string(), json!(path_text(app)));
    }
    config.insert(
        "agent_profiles_version".to_string(),
        json!(PROFILES_VERSION),
    );
    config.insert(
        "resources".to_string(),
        json!({
            "min_free_ram_mb": 0,
            "min_free_commit_mb": 0,
            "max_heavy_subagents": 99,
            "heavy_mb": 10_000_000
        }),
    );
    write_json(&config_path(sandbox), &Value::Object(config))
}

/// The command of a fake agent.
fn fake_command(sandbox: &Sandbox, key: &str) -> Value {
    json!([
        path_text(&sandbox.bin.join(format!("{key}.exe"))),
        "--echo",
        path_text(&sandbox.registros)
    ])
}

/// The `modelos.toml` of the test.
///
/// DECISION: model names with dots are quoted (`["deepseek-v4.1-flash"]`).
/// TOML reads an unquoted `[deepseek-v4.1-flash]` as a nested table; the
/// product's own example has that problem (reported in the IR-4 report).
pub fn write_modelos(sandbox: &Sandbox) -> Result<(), String> {
    let text = "\
[\"deepseek-v4-pro\"]\n\
agente = \"opencode\"\nnivel = \"alto\"\ncosto = \"plan\"\nfamilia = \"deepseek\"\n\
\n\
[opus]\n\
agente = \"claude\"\nnivel = \"alto\"\ncosto = \"plan\"\nfamilia = \"anthropic\"\n\
\n\
[kimi-k3]\n\
agente = \"opencode\"\nnivel = \"alto\"\ncosto = \"pago\"\nfamilia = \"moonshot\"\n\
\n\
[\"deepseek-v4.1-flash\"]\n\
agente = \"opencode\"\nnivel = \"medio\"\ncosto = \"plan\"\nfamilia = \"deepseek\"\n\
\n\
[\"hy4-flash-free\"]\n\
agente = \"opencode\"\nnivel = \"medio\"\ncosto = \"gratis\"\nfamilia = \"opencode\"\n";
    write_text(&config_folder(sandbox).join("modelos.toml"), text)
}

/// The `config.yaml` of mcode: one provider, one model and the effort block.
fn write_mcode_config(sandbox: &Sandbox) -> Result<(), String> {
    let text = "provider:\n  minimax:\n    api_key: clave-de-prueba\n    models:\n      \
                MiniMax-M3.1-Flash:\n        context: 200k\ndefaultModelThinking:\n  effort: \
                default\n";
    write_text(&sandbox.minimax.join("config.yaml"), text)
}

/// The Codex cache with one visible model and its levels.
fn write_codex_cache(sandbox: &Sandbox) -> Result<(), String> {
    let data = json!({
        "models": [{
            "slug": CODEX_MODEL,
            "visibility": "list",
            "supported_reasoning_levels": [
                {"effort": "low"},
                {"effort": "medium"},
                {"effort": "high"}
            ]
        }]
    });
    write_json(&sandbox.codex.join("models_cache.json"), &data)
}

/// The `EzAgent` folder inside the test APPDATA.
pub fn config_folder(sandbox: &Sandbox) -> PathBuf {
    sandbox.appdata.join("EzAgent")
}

/// The `voice.json` path of the test.
pub fn config_path(sandbox: &Sandbox) -> PathBuf {
    config_folder(sandbox).join("voice.json")
}

/// Rewrites the resource thresholds of the test `voice.json`.
pub fn set_resources(
    sandbox: &Sandbox,
    ram_mb: u64,
    commit_mb: u64,
    max_heavy: u64,
) -> Result<(), String> {
    let path = config_path(sandbox);
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("No pude leer {}: {error}", path.display()))?;
    let mut data: Value = serde_json::from_str(&text)
        .map_err(|error| format!("El voice.json de la prueba no es JSON: {error}"))?;
    let Some(map) = data.as_object_mut() else {
        return Err("El voice.json de la prueba no es un objeto.".to_string());
    };
    map.insert(
        "resources".to_string(),
        json!({
            "min_free_ram_mb": ram_mb,
            "min_free_commit_mb": commit_mb,
            "max_heavy_subagents": max_heavy,
            "heavy_mb": 10_000_000
        }),
    );
    write_json(&path, &data)
}

/// Writes a brief and returns its path and unique mark.
pub fn write_brief(sandbox: &Sandbox, index: usize) -> Result<(PathBuf, String), String> {
    let mark = format!("IR4-{}-{index}", sandbox.id);
    let path = sandbox.briefs.join(format!("{index}.md"));
    let text = format!("Trabaja en {mark}: revisa la carpeta y escribe un informe corto.\n");
    write_text(&path, &text)?;
    Ok((path, mark))
}

/// The path of the result JSON, outside the folder that cleanup removes.
pub fn result_path(sandbox: &Sandbox) -> PathBuf {
    std::env::temp_dir().join(format!("{}.json", sandbox.id))
}

/// Removes the sandbox only when it is really under the temporary folder.
pub fn remove_root(sandbox: &Sandbox) {
    let temp = std::env::temp_dir();
    let name = sandbox
        .root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if sandbox.root.parent() == Some(temp.as_path()) && name.starts_with("ez-ir4-") {
        let _ = std::fs::remove_dir_all(&sandbox.root);
    }
}

/// Writes pretty JSON and creates the parent folder.
pub fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("No pude serializar {}: {error}", path.display()))?;
    write_text(path, &format!("{text}\n"))
}

/// Writes a text file and creates the parent folder.
pub fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("No pude crear {}: {error}", parent.display()))?;
    }
    std::fs::write(path, text)
        .map_err(|error| format!("No pude escribir {}: {error}", path.display()))
}

/// A path as text.
pub fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

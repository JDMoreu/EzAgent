//! Pruebas del inventario con un sistema falso y carpetas temporales.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use ez_core::config::VoiceConfig;
use serde_json::json;

use super::*;
use crate::resources::SubagentUsage;
use crate::system::SystemInfo;

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos());
        let path =
            std::env::temp_dir().join(format!("ez-scan-{}-{id}-{nanos}", std::process::id()));
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

/// Doble del sistema: `which` devuelve rutas preparadas y `version` responde.
struct Fake {
    paths: Vec<(String, PathBuf)>,
    versions: Vec<(PathBuf, String)>,
    opencode: Mutex<Vec<String>>,
}

impl Fake {
    fn new(paths: Vec<(&str, PathBuf)>) -> Self {
        Self {
            paths: paths
                .into_iter()
                .map(|(name, path)| (name.into(), path))
                .collect(),
            versions: Vec::new(),
            opencode: Mutex::new(Vec::new()),
        }
    }

    fn versioning(mut self, path: PathBuf, version: &str) -> Self {
        self.versions.push((path, version.to_string()));
        self
    }
}

impl Probe for Fake {
    fn system(&self, _workdir: &Path) -> Result<SystemInfo, String> {
        Err("no se usa".into())
    }
    fn free_ram_mb(&self) -> Result<u64, String> {
        Err("no se usa".into())
    }
    fn free_commit_mb(&self) -> Result<u64, String> {
        Err("no se usa".into())
    }
    fn cpu_percent(&self) -> Result<f32, String> {
        Err("no se usa".into())
    }
    fn disk_free_mb(&self, _workdir: &Path) -> Option<u64> {
        None
    }
    fn subagents(&self, _parent: &str) -> Result<Vec<SubagentUsage>, String> {
        Ok(Vec::new())
    }
    fn which(&self, program: &str) -> Option<PathBuf> {
        self.paths
            .iter()
            .find(|(name, _)| name == program)
            .map(|(_, path)| path.clone())
    }
    fn version(&self, program: &Path) -> Option<String> {
        self.versions
            .iter()
            .find(|(path, _)| path == program)
            .map(|(_, version)| version.clone())
    }
    fn opencode_models(&self, _program: &Path) -> Vec<String> {
        self.opencode
            .lock()
            .map(|items| items.clone())
            .unwrap_or_default()
    }
}

fn config() -> VoiceConfig {
    let data = json!({
        "agents": {"opencode": ["opencode"], "mcode": ["mcode"]},
        "agent_profiles": {
            "opencode": {
                "label": "OpenCode",
                "models": ["glm-5.3"],
                "efforts": [],
                "model_efforts": {"glm-5.3": ["high", "max"], "kimi-k3": ["low"]}
            },
            "mcode": {"label": "MiniMax Code", "models": [], "efforts": ["low", "high"]}
        }
    });
    match data {
        serde_json::Value::Object(map) => VoiceConfig::from_json(&map),
        _ => VoiceConfig::new(),
    }
}

#[test]
fn el_guion_sin_extension_se_resuelve_al_exe_al_que_apunta() {
    let dir = TempDir::new();
    let exe = dir.path().join("node_modules").join("opencode.exe");
    write(&exe, "");
    let shim = dir.path().join("opencode");
    write(
        &shim,
        "#!/bin/sh\nexec \"$basedir/node_modules/opencode.exe\" \"$@\"\n",
    );
    assert_eq!(native_path(&shim), exe);
}

#[test]
fn el_cmd_es_la_ruta_nativa_si_no_hay_exe() {
    let dir = TempDir::new();
    let cmd = dir.path().join("mcode.cmd");
    write(&cmd, "@ECHO off\n");
    assert_eq!(native_path(&cmd), cmd);
}

#[test]
fn el_inventario_marca_instalados_y_no_abre_agentes() {
    let dir = TempDir::new();
    let exe = dir.path().join("bin").join("opencode.exe");
    write(&exe, "");
    let shim = dir.path().join("bin").join("opencode");
    write(&shim, "exec \"$basedir/opencode.exe\" \"$@\"\n");
    let cmd = dir.path().join("bin").join("mcode.cmd");
    write(&cmd, "@ECHO off\n");

    let mut probe = Fake::new(vec![("opencode", shim), ("mcode", cmd.clone())])
        .versioning(exe.clone(), "1.2.3");
    probe.opencode = Mutex::new(vec!["provider/glm-5.3".into(), "glm-5.3".into()]);

    let agents = inventory(&probe, &config());
    assert_eq!(agents.len(), 2);
    let opencode = &agents[0];
    assert!(opencode.installed);
    assert_eq!(
        opencode.path.as_deref(),
        Some(exe.to_string_lossy().as_ref())
    );
    assert_eq!(opencode.version.as_deref(), Some("1.2.3"));
    assert_eq!(opencode.models, vec!["glm-5.3", "provider/glm-5.3"]);
    assert_eq!(opencode.efforts, vec!["high", "max", "low"]);
    let mcode = &agents[1];
    assert!(mcode.installed);
    assert_eq!(mcode.path.as_deref(), Some(cmd.to_string_lossy().as_ref()));
    assert_eq!(mcode.version, None);
    assert_eq!(mcode.efforts, vec!["low", "high"]);
}

#[test]
fn el_catalogo_vacio_no_produce_agentes() {
    let data = json!({"agents": {}});
    let map = match data {
        serde_json::Value::Object(map) => map,
        _ => Default::default(),
    };
    let cfg = VoiceConfig::from_json(&map);
    let agents = inventory(&Fake::new(Vec::new()), &cfg);
    assert!(agents.is_empty());
}

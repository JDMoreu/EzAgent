//! `ez watch`: el vigilante de los subagentes de un padre (§13 del diseño).
//!
//! Corre en una terminal de `hc` oculta: mira el registro del padre y sus
//! terminales, marca `fallido` lo que terminó sin informe y avisa al padre.
//! Es un comando interno: lo arrancan `ez delegate` y `ez report`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ez_herdr::subagents::{
    ensure_watcher, HcRuntime, MemoryProbe, MemoryReading, WatchConfig, WatchRuntime, WatchStop,
    Watcher, WatcherSetup, WatcherState, DEFAULT_GRACE_MS, DEFAULT_IDLE_MS, DEFAULT_INTERVAL_MS,
};
use ez_scan::capacity::Thresholds;
use ez_scan::Probe;
use serde_json::Value;

use crate::cli::{CliError, Parsed};

use super::state::{Context, LocalState};

/// `ez watch --parent P --dir D [--hc H]`.
pub(crate) struct WatchCommand {
    /// La terminal del padre.
    pub parent: String,
    /// La carpeta raíz del registro.
    pub dir: String,
    /// La ruta de `hc` (por omisión, la de la configuración).
    pub hc: Option<String>,
}

/// Analiza `ez watch`.
pub(crate) fn build_watch(parsed: &mut Parsed, _cwd: &Path) -> Result<LocalState, CliError> {
    let parent = parsed.required("parent")?;
    let dir = parsed.required("dir")?;
    let hc = parsed.take("hc");
    parsed.check_empty()?;
    Ok(LocalState::Watch(WatchCommand { parent, dir, hc }))
}

/// Corre el vigilante hasta que termina.
pub(crate) fn run_watch(
    command: &WatchCommand,
    context: &Context,
    env: &dyn Fn(&str) -> Option<String>,
    _json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let registry = ez_herdr::subagents::Registry::at(&command.dir);
    // DECISION: la ruta de `hc` ya viene resuelta en el contexto (el `--hc`
    // del vigilante): no hace falta leer la configuración real.
    let runtime = HcRuntime::new(ez_herdr::herdr::system_runtime(), context.hc.clone());
    // DECISION: los tiempos del vigilante salen del entorno: así una prueba
    // puede esperarlo sin tardar diez minutos.
    let config = WatchConfig {
        parent: command.parent.clone(),
        interval_ms: number(env, "EZ_WATCH_INTERVAL_MS", DEFAULT_INTERVAL_MS),
        idle_ms: number(env, "EZ_WATCH_IDLE_MS", DEFAULT_IDLE_MS),
        grace_ms: number(env, "EZ_WATCH_GRACE_MS", DEFAULT_GRACE_MS),
    };
    let mut watcher =
        Watcher::new(config, registry, Arc::new(runtime)).with_memory_probe(Arc::new(ScanMemory {
            parent: command.parent.clone(),
            thresholds: thresholds(env),
        }));
    // DECISION: el reintento automático se decide en cada novedad leyendo
    // `planes.toml`; `ez watch` solo instala el gancho (el vigilante no puede
    // depender de `ez-scan`).
    let appdata = env("APPDATA")
        .map(|base| PathBuf::from(base).join("EzAgent"))
        .unwrap_or_else(ez_core::dirs::app_data_dir);
    watcher = watcher.with_auto_retry(Arc::new(super::retry::WatchAutoRetry {
        parent: command.parent.clone(),
        root: command.dir.clone().into(),
        appdata,
    }));
    let stop = watcher.run();
    let message = match stop {
        WatchStop::ParentGone => "Vigilante terminado: el padre ya no está vivo.",
        WatchStop::Idle => "Vigilante terminado: no queda nada pendiente.",
    };
    let _ = writeln!(out, "{message}");
    let _ = err;
    0
}

/// Un número del entorno (o el valor de fábrica).
fn number(env: &dyn Fn(&str) -> Option<String>, name: &str, default: u64) -> u64 {
    env(name)
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

/// Los umbrales de `voice.json` según el entorno inyectado.
fn thresholds(env: &dyn Fn(&str) -> Option<String>) -> Thresholds {
    let path = env("APPDATA").map(|base| PathBuf::from(base).join("EzAgent").join("voice.json"));
    Thresholds::load(path.as_deref())
}

/// El medidor de memoria del vigilante: `ez-scan` y sus umbrales.
///
/// DECISION: el vigilante (`ez-herdr`) no puede depender de `ez-scan`, así que
/// esta implementación vive aquí y se la pasa con `with_memory_probe`.
struct ScanMemory {
    parent: String,
    thresholds: Thresholds,
}

impl MemoryProbe for ScanMemory {
    fn low(&self) -> Option<MemoryReading> {
        let probe = ez_scan::RealProbe;
        let free_ram_mb = probe.free_ram_mb().ok()?;
        let free_commit_mb = probe.free_commit_mb().ok()?;
        let ram_low = free_ram_mb < self.thresholds.min_free_ram_mb;
        let commit_low = free_commit_mb < self.thresholds.min_free_commit_mb;
        if !ram_low && !commit_low {
            return None;
        }
        let subagents = probe
            .subagents(&self.parent, &self.thresholds)
            .unwrap_or_default();
        let top = subagents.iter().max_by_key(|child| child.memory_mb);
        Some(MemoryReading {
            free_ram_mb,
            free_commit_mb,
            top_name: top.map(|child| child.name.clone()),
            top_mb: top.map_or(0, |child| child.memory_mb),
        })
    }
}

/// Arranca el vigilante del padre si no hay uno corriendo.
pub(crate) fn start_watcher(context: &Context, parent: &str) {
    let Some(ez) = std::env::current_exe().ok() else {
        return;
    };
    let setup = WatcherSetup {
        parent: parent.to_string(),
        root: context.root.clone(),
        hc: context.hc.clone(),
        ez,
    };
    let runtime = HcRuntime::new(ez_herdr::herdr::system_runtime(), context.hc.clone());
    if let WatcherState::Failed(message) = ensure_watcher(&runtime, &setup) {
        tracing::warn!("EzAgent: no pude arrancar el vigilante: {message}");
    }
}

/// Si la terminal es un hermano del padre (comparten padre en `hc`).
pub(crate) fn is_sibling(context: &Context, terminal: &str, parent: &str) -> bool {
    terminal_parent(context, terminal).as_deref() == Some(parent)
}

/// El padre de una terminal, según `hc get`.
fn terminal_parent(context: &Context, terminal: &str) -> Option<String> {
    let runtime = HcRuntime::new(ez_herdr::herdr::system_runtime(), context.hc.clone());
    let args = vec!["get".to_string(), terminal.to_string()];
    let output = runtime.hc(&args).ok()?;
    if output.code != 0 {
        return None;
    }
    let data = ez_core::pyloads::loads(&output.stdout).ok()?;
    data.get("parent")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::Mutex;

    use ez_core::config::VoiceConfig;

    use super::super::state::hc_path_with;
    use super::*;

    /// El `APPDATA` de las pruebas: una sola prueba a la vez lo cambia.
    static APPDATA: Mutex<()> = Mutex::new(());

    /// Una carpeta temporal con nombre único.
    fn temp_folder(tag: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!(
            "ez-watch-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&folder).expect("carpeta temporal");
        folder
    }

    #[test]
    fn con_la_ruta_dada_no_se_lee_la_configuracion() {
        let env = |_: &str| None;
        let llamadas = Cell::new(0);
        let ruta = hc_path_with(&env, Some("C:\\herdr\\hc.exe"), &|| {
            llamadas.set(llamadas.get() + 1);
            None
        });
        assert_eq!(ruta, "C:\\herdr\\hc.exe");
        assert_eq!(llamadas.get(), 0, "no debe leer la configuración");
    }

    #[test]
    fn sin_la_ruta_dada_se_lee_la_configuracion() {
        let env = |_: &str| None;
        let llamadas = Cell::new(0);
        let ruta = hc_path_with(&env, None, &|| {
            llamadas.set(llamadas.get() + 1);
            None
        });
        assert_eq!(ruta, "hc");
        assert_eq!(llamadas.get(), 1, "debe leer la configuración");
    }

    #[test]
    fn el_vigilante_no_necesita_el_voice_json_de_la_persona() {
        let _guard = APPDATA.lock().expect("APPDATA");
        let folder = temp_folder("appdata");
        let antes = std::env::var_os("APPDATA");
        std::env::set_var("APPDATA", &folder);
        // La carpeta no tiene `EzAgent/voice.json`: la configuración real de
        // la persona no está y no hace falta.
        assert!(!folder.join("EzAgent").join("voice.json").exists());
        let config = VoiceConfig::load(None).expect("configuración de fábrica");
        assert_eq!(
            config.get("hc_path").and_then(Value::as_str),
            Some("hc"),
            "sin voice.json solo queda el valor de fábrica"
        );
        let env = |name: &str| match name {
            "EZ_SUBAGENTS_DIR" => Some(folder.join("subagentes").to_string_lossy().into_owned()),
            _ => None,
        };
        let context = Context::with_hc(&env, Some("C:\\herdr\\hc.exe"));
        assert_eq!(context.hc, "C:\\herdr\\hc.exe");
        match antes {
            Some(value) => std::env::set_var("APPDATA", value),
            None => std::env::remove_var("APPDATA"),
        }
        let _ = std::fs::remove_dir_all(&folder);
    }
}

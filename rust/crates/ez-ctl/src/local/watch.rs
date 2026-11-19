//! `ez watch`: el vigilante de los subagentes de un padre (§13 del diseño).
//!
//! Corre en una terminal de `hc` oculta: mira el registro del padre y sus
//! terminales, marca `fallido` lo que terminó sin informe y avisa al padre.
//! Es un comando interno: lo arrancan `ez delegate` y `ez report`.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use ez_herdr::subagents::{
    ensure_watcher, HcRuntime, WatchConfig, WatchRuntime, WatchStop, Watcher, WatcherSetup,
    WatcherState, DEFAULT_GRACE_MS, DEFAULT_IDLE_MS, DEFAULT_INTERVAL_MS,
};
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
    let hc = command
        .hc
        .clone()
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| context.hc.clone());
    let runtime = HcRuntime::new(ez_herdr::herdr::system_runtime(), hc);
    // DECISION: los tiempos del vigilante salen del entorno: así una prueba
    // puede esperarlo sin tardar diez minutos.
    let config = WatchConfig {
        parent: command.parent.clone(),
        interval_ms: number(env, "EZ_WATCH_INTERVAL_MS", DEFAULT_INTERVAL_MS),
        idle_ms: number(env, "EZ_WATCH_IDLE_MS", DEFAULT_IDLE_MS),
        grace_ms: number(env, "EZ_WATCH_GRACE_MS", DEFAULT_GRACE_MS),
    };
    let mut watcher = Watcher::new(config, registry, Arc::new(runtime));
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

//! El vigilante de los subagentes (§13 del diseño).
//!
//! Corre uno por padre, en una terminal de `hc` oculta y sin padre. Cada 3 s
//! lee el registro del padre y la lista de sus terminales, marca `fallido` lo
//! que terminó sin informe y avisa al padre de cada cambio de estado con
//! `hc send`.
//!
//! `hc` y el reloj se inyectan ([`WatchRuntime`]) para que las pruebas no
//! dependan de Herdr ni esperen de verdad.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::herdr::{hc_slug, HerdrRuntime};

use super::{notice_text, pending_notices, Record, Registry, TaskState};

#[cfg(test)]
mod tests;

/// Cada cuánto mira el vigilante (ms).
pub const DEFAULT_INTERVAL_MS: u64 = 3_000;

/// Cuánto puede pasar sin nada pendiente antes de terminar (ms).
pub const DEFAULT_IDLE_MS: u64 = 600_000;

/// Margen para dar por muerta una terminal que todavía no aparece (ms).
pub const DEFAULT_GRACE_MS: u64 = 6_000;

/// La salida de `hc`.
#[derive(Debug, Clone, Default)]
pub struct HcOutput {
    /// El código de salida del proceso.
    pub code: i64,
    /// La salida estándar.
    pub stdout: String,
    /// La salida de error.
    pub stderr: String,
}

/// Lo que el vigilante necesita del exterior: `hc` y el reloj.
pub trait WatchRuntime: Send + Sync {
    /// Ejecuta `hc` con estos argumentos (sin shell).
    fn hc(&self, args: &[String]) -> Result<HcOutput, String>;

    /// Ahora, en milisegundos desde epoch.
    fn now_ms(&self) -> u64;

    /// Duerme estos milisegundos.
    fn sleep_ms(&self, ms: u64);
}

/// El runtime real: `hc` a través del runtime de Herdr.
pub struct HcRuntime {
    runtime: Arc<dyn HerdrRuntime>,
    hc: String,
    timeout: f64,
}

impl HcRuntime {
    /// Un runtime que habla con el `hc` de `path`.
    pub fn new(runtime: Arc<dyn HerdrRuntime>, path: impl Into<String>) -> Self {
        Self {
            runtime,
            hc: path.into(),
            timeout: 30.0,
        }
    }

    /// La ruta de `hc` que usa este runtime.
    pub fn hc_path(&self) -> &str {
        &self.hc
    }

    fn argv(&self, args: &[String]) -> Vec<String> {
        let mut argv = Vec::with_capacity(args.len() + 1);
        argv.push(self.hc.clone());
        argv.extend_from_slice(args);
        argv
    }
}

impl WatchRuntime for HcRuntime {
    fn hc(&self, args: &[String]) -> Result<HcOutput, String> {
        let argv = self.argv(args);
        match self.runtime.run(&argv, self.timeout, false) {
            Ok(done) => Ok(HcOutput {
                code: done.returncode,
                stdout: done.stdout,
                stderr: done.stderr,
            }),
            Err(error) => Err(error.to_string()),
        }
    }

    fn now_ms(&self) -> u64 {
        epoch_ms()
    }

    fn sleep_ms(&self, ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }
}

/// Ahora, en milisegundos desde epoch.
pub fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// Lo que el vigilante mira.
#[derive(Debug, Clone)]
pub struct WatchConfig {
    /// La terminal del padre: la que recibe los avisos.
    pub parent: String,
    /// Cada cuánto mira (ms).
    pub interval_ms: u64,
    /// Cuánto puede estar sin nada pendiente antes de terminar (ms).
    pub idle_ms: u64,
    /// Margen antes de marcar `fallido` una terminal que no aparece (ms).
    pub grace_ms: u64,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            parent: String::new(),
            interval_ms: DEFAULT_INTERVAL_MS,
            idle_ms: DEFAULT_IDLE_MS,
            grace_ms: DEFAULT_GRACE_MS,
        }
    }
}

/// Por qué terminó el vigilante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchStop {
    /// El padre ya no está vivo.
    ParentGone,
    /// No quedó nada pendiente durante el tiempo de inactividad.
    Idle,
}

/// Lo que pasó en una vuelta del vigilante.
#[derive(Debug, Clone, Default)]
pub struct Tick {
    /// Los registros que se marcaron como `fallido`.
    pub failed: Vec<String>,
    /// Los avisos que se mandaron al padre.
    pub notified: Vec<String>,
    /// Si el padre sigue vivo (`None` si `hc` no respondió).
    pub parent_alive: Option<bool>,
}

/// Una terminal que cuelga del padre, según `hc ls`.
#[derive(Debug, Clone)]
struct Child {
    address: String,
    live: bool,
    exit_code: Option<i64>,
}

/// El vigilante de un padre.
pub struct Watcher {
    config: WatchConfig,
    registry: Registry,
    runtime: Arc<dyn WatchRuntime>,
    /// Cuándo se vio cada registro por primera vez (para el margen).
    seen_at: HashMap<String, u64>,
    /// Cuándo fue la última vez que había algo pendiente.
    active_at: u64,
}

impl Watcher {
    /// Un vigilante del padre de `config`.
    pub fn new(
        config: WatchConfig,
        registry: Registry,
        runtime: Arc<dyn WatchRuntime>,
    ) -> Self {
        let now = runtime.now_ms();
        Self {
            config,
            registry,
            runtime,
            seen_at: HashMap::new(),
            active_at: now,
        }
    }

    /// Una vuelta: mira las terminales, marca lo que terminó y avisa.
    pub fn tick(&mut self) -> Tick {
        let mut tick = Tick::default();
        let records = self.registry.list(&self.config.parent).unwrap_or_default();
        let Some(children) = self.children() else {
            // DECISION: si `hc` no responde no se cambia nada: se reintenta en
            // la vuelta siguiente en vez de dar por muerto al subagente.
            return tick;
        };
        let alive = self.parent_alive();
        tick.parent_alive = Some(alive);
        if !alive {
            return tick;
        }
        self.mark_failed(&records, &children, &mut tick);
        // DECISION: se releen los registros: el aviso sale del estado nuevo
        // (por ejemplo, «falló» y no «está en proceso»).
        let fresh = self.registry.list(&self.config.parent).unwrap_or_default();
        self.notify(&fresh, &mut tick);
        self.refresh_activity();
        tick
    }

    /// El bucle: vuelve cada `interval_ms` hasta que termine.
    pub fn run(&mut self) -> WatchStop {
        loop {
            let tick = self.tick();
            if tick.parent_alive == Some(false) {
                return WatchStop::ParentGone;
            }
            if self.runtime.now_ms().saturating_sub(self.active_at) >= self.config.idle_ms {
                return WatchStop::Idle;
            }
            self.runtime.sleep_ms(self.config.interval_ms);
        }
    }

    /// Las terminales del padre, según `hc ls`.
    fn children(&self) -> Option<Vec<Child>> {
        let args = vec![
            "ls".to_string(),
            "--all".to_string(),
            "--json".to_string(),
            "--label".to_string(),
            format!("ez.parent={}", self.config.parent),
        ];
        let output = self.runtime.hc(&args).ok()?;
        if output.code != 0 {
            return None;
        }
        let data = ez_core::pyloads::loads(&output.stdout).ok()?;
        Some(parse_children(&data))
    }

    /// Si el padre sigue vivo (`hc get`).
    fn parent_alive(&self) -> bool {
        let args = vec!["get".to_string(), self.config.parent.clone()];
        let Ok(output) = self.runtime.hc(&args) else {
            return false;
        };
        if output.code != 0 {
            return false;
        }
        let Ok(data) = ez_core::pyloads::loads(&output.stdout) else {
            return false;
        };
        state_of(&data) == "live"
    }

    /// Marca `fallido` lo que terminó sin informe.
    fn mark_failed(&mut self, records: &[Record], children: &[Child], tick: &mut Tick) {
        let now = self.runtime.now_ms();
        for record in records {
            if record.state != TaskState::InProgress {
                continue;
            }
            let first = *self.seen_at.entry(record.name.clone()).or_insert(now);
            if now.saturating_sub(first) < self.config.grace_ms {
                continue;
            }
            let child = children.iter().find(|item| item.address == record.terminal);
            let detail = match child {
                Some(child) if child.live => continue,
                Some(child) => match child.exit_code {
                    Some(code) => format!("la terminal terminó sin informe (código {code})"),
                    None => "la terminal terminó sin informe".to_string(),
                },
                None => "la terminal terminó sin informe (no se encontró en Herdr)".to_string(),
            };
            let changed = self.registry.transition(
                &record.parent,
                &record.name,
                TaskState::Failed,
                "vigilante",
                &detail,
                |_| {},
            );
            if changed.is_ok() {
                tick.failed.push(record.name.clone());
            }
        }
    }

    /// Avisa al padre de cada cambio de estado pendiente.
    fn notify(&mut self, records: &[Record], tick: &mut Tick) {
        for record in pending_notices(records) {
            let text = notice_text(record);
            if !self.send(&text) {
                // DECISION: si `hc send` falla, el aviso se reintenta en la
                // vuelta siguiente (el registro sigue sin avisar).
                continue;
            }
            let saved = self
                .registry
                .mark_notified(&record.parent, &record.name, record.state);
            if saved.is_ok() {
                tick.notified.push(record.name.clone());
            }
        }
    }

    /// `hc send <padre> <aviso>`.
    fn send(&self, text: &str) -> bool {
        let args = vec![
            "send".to_string(),
            self.config.parent.clone(),
            text.to_string(),
            "--json".to_string(),
        ];
        match self.runtime.hc(&args) {
            Ok(output) => output.code == 0,
            Err(_) => false,
        }
    }

    /// Anota que hay algo pendiente (para saber cuándo terminar).
    fn refresh_activity(&mut self) {
        let records = self.registry.list(&self.config.parent).unwrap_or_default();
        let pending = records
            .iter()
            .any(|record| record.state == TaskState::InProgress)
            || !pending_notices(&records).is_empty();
        if pending {
            self.active_at = self.runtime.now_ms();
        }
    }
}

/// El estado de una terminal en el JSON de `hc`.
fn state_of(data: &Value) -> String {
    if let Some(state) = data.get("state").and_then(Value::as_str) {
        return state.to_string();
    }
    match data.get("status") {
        Some(Value::Object(status)) => status
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Some(Value::String(state)) => state.clone(),
        _ => String::new(),
    }
}

/// Las terminales de `hc ls --json`.
fn parse_children(data: &Value) -> Vec<Child> {
    let Some(terminals) = data.get("terminals").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut children = Vec::new();
    for terminal in terminals {
        let Some(address) = terminal.get("address").and_then(Value::as_str) else {
            continue;
        };
        let state = state_of(terminal);
        children.push(Child {
            address: address.to_string(),
            live: state == "live",
            exit_code: terminal.get("exit_code").and_then(Value::as_i64),
        });
    }
    children
}

/// Lo que el vigilante necesita para arrancar.
#[derive(Debug, Clone)]
pub struct WatcherSetup {
    /// La terminal del padre.
    pub parent: String,
    /// La carpeta raíz del registro.
    pub root: PathBuf,
    /// La ruta de `hc`.
    pub hc: String,
    /// La ruta de `ez` (el binario que arranca el vigilante).
    pub ez: PathBuf,
}

/// El registro y el vigilante de una delegación (§13).
///
/// Sin este dato la delegación no guarda nada: el comando decide si quiere el
/// mecanismo de avisos o no.
pub struct DelegateState {
    /// Dónde se guarda el registro del subagente.
    pub registry: Registry,
    /// El vigilante que avisa al padre (si lo hay, se arranca).
    pub watcher: Option<WatcherSetup>,
}

impl DelegateState {
    /// El registro del entorno (`EZ_SUBAGENTS_DIR` o `%LOCALAPPDATA%`).
    pub fn from_env() -> Self {
        Self {
            registry: Registry::from_env(),
            watcher: None,
        }
    }
}

/// Lo que pasó al arrancar el vigilante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherState {
    /// Se arrancó uno nuevo.
    Started,
    /// Ya había uno corriendo con ese nombre.
    AlreadyRunning,
    /// No se pudo arrancar.
    Failed(String),
}

/// El nombre de `hc` del vigilante de un padre.
pub fn watcher_name(parent: &str) -> String {
    format!("ez-watch-{}", hc_slug(parent))
}

/// El comando que arranca el vigilante.
pub fn watcher_args(setup: &WatcherSetup) -> Vec<String> {
    vec![
        "run".to_string(),
        "--json".to_string(),
        "--bg".to_string(),
        "--no-parent".to_string(),
        "--no-notify".to_string(),
        "--name".to_string(),
        watcher_name(&setup.parent),
        "--label".to_string(),
        format!("ez.watch={}", setup.parent),
        "--".to_string(),
        setup.ez.to_string_lossy().into_owned(),
        "watch".to_string(),
        "--parent".to_string(),
        setup.parent.clone(),
        "--dir".to_string(),
        setup.root.to_string_lossy().into_owned(),
        "--hc".to_string(),
        setup.hc.clone(),
    ]
}

/// Arranca el vigilante del padre si no hay uno corriendo.
///
/// Si `hc` dice que el nombre ya está ocupado, el vigilante ya está vivo: no
/// es un error.
pub fn ensure_watcher(runtime: &dyn WatchRuntime, setup: &WatcherSetup) -> WatcherState {
    let args = watcher_args(setup);
    let output = match runtime.hc(&args) {
        Ok(output) => output,
        Err(error) => return WatcherState::Failed(error),
    };
    if let Ok(data) = ez_core::pyloads::loads(&output.stdout) {
        if let Some(error) = data.get("error") {
            let kind = error.get("kind").and_then(Value::as_str);
            if kind == Some("name_taken") {
                return WatcherState::AlreadyRunning;
            }
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("no pude arrancar el vigilante")
                .to_string();
            return WatcherState::Failed(message);
        }
    }
    if output.code != 0 {
        let message = if output.stderr.trim().is_empty() {
            output.stdout.trim().to_string()
        } else {
            output.stderr.trim().to_string()
        };
        return WatcherState::Failed(if message.is_empty() {
            "hc run no devolvió nada".to_string()
        } else {
            message
        });
    }
    WatcherState::Started
}

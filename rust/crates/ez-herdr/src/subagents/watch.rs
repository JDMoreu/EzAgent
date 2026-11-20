//! El vigilante de los subagentes (§13 del diseño).
//!
//! Corre uno por padre, en una terminal de `hc` oculta y sin padre. Cada 3 s
//! lee el registro del padre y la lista de sus terminales, marca `fallido` lo
//! que terminó sin informe y avisa al padre de cada cambio de estado con
//! `hc send`.
//!
//! `hc` y el reloj se inyectan ([`WatchRuntime`]) para que las pruebas no
//! dependan de Herdr ni esperen de verdad.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ez_core::pyvalue::truthy;
use serde_json::Value;

use crate::herdr::hc_slug;

use super::{
    notice_text, pending_notices, AutoRetry, InvalidRecord, ListOutcome, Record, Registry,
    TaskState,
};

mod children;
mod memory;
mod novelties;
mod runtime;
#[cfg(test)]
mod tests;

pub use memory::{MemoryProbe, MemoryReading};
pub use runtime::{HcOutput, HcRuntime, WatchRuntime};

/// Cada cuánto mira el vigilante (ms).
pub const DEFAULT_INTERVAL_MS: u64 = 3_000;

/// Cuánto puede pasar sin nada pendiente antes de terminar (ms).
pub const DEFAULT_IDLE_MS: u64 = 600_000;

/// Margen para dar por muerta una terminal que todavía no aparece (ms).
pub const DEFAULT_GRACE_MS: u64 = 6_000;

/// Quién recibe los avisos del vigilante.
///
/// `ez watch` usa el camino de siempre (`hc send` a la terminal del padre);
/// la app de voz instala uno propio para llevar el aviso a su chat y su voz.
pub trait NoticeSink: Send + Sync {
    /// Entrega un aviso ya escrito; `record` falta en los que no son de un
    /// registro (un archivo ilegible o la memoria baja).
    ///
    /// `true` si llegó a su destino; si no, el vigilante lo reintenta en la
    /// vuelta siguiente.
    fn deliver(&self, record: Option<&Record>, text: &str) -> bool;
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
    /// Los registros que quedaron `bloqueado`.
    pub blocked: Vec<String>,
    /// Los registros que volvieron a `en_proceso`.
    pub resumed: Vec<String>,
    /// Los avisos de «sin avance» (sin cambio de estado).
    pub stalled: Vec<String>,
    /// Los reintentos automáticos que lanzaron un hijo nuevo.
    pub retried: Vec<String>,
    /// Los avisos que se mandaron al padre.
    pub notified: Vec<String>,
    /// Se avisó de memoria baja en esta vuelta.
    pub memory_warned: bool,
    /// Si el padre sigue vivo (`None` si `hc` no respondió).
    pub parent_alive: Option<bool>,
}

// Una terminal que cuelga del padre, según `hc ls`.
use children::{parse_children, state_of, Child};

/// Lo último que se vio de la pantalla de un hijo.
#[derive(Debug, Clone)]
struct ScreenMemory {
    screen: String,
    changed_at: u64,
    stall_notified: bool,
}

/// El vigilante de un padre.
pub struct Watcher {
    config: WatchConfig,
    registry: Registry,
    runtime: Arc<dyn WatchRuntime>,
    /// Quién recibe los avisos; `None` los manda a la terminal del padre con
    /// `hc send` (el camino de `ez watch`).
    notice_sink: Option<Arc<dyn NoticeSink>>,
    /// `true` cuando el padre no es una terminal (la app de voz): no hay
    /// `hc get` de vida y los hijos se cruzan por su dirección en el registro.
    parentless: bool,
    /// Cuándo se vio cada registro por primera vez (para el margen).
    seen_at: HashMap<String, u64>,
    /// Cuándo fue la última vez que había algo pendiente.
    active_at: u64,
    /// El `novedades.toml` de la persona (`%APPDATA%\EzAgent`).
    novelty_file: Option<PathBuf>,
    /// La última pantalla vista de cada hijo, para «sin avance».
    screens: HashMap<String, ScreenMemory>,
    /// Los archivos ilegibles de los que ya se avisó.
    warned_invalid: HashSet<PathBuf>,
    /// El medidor de memoria (lo instala `ez watch`).
    memory: Option<Arc<dyn MemoryProbe>>,
    /// Ya se avisó de memoria baja en este episodio.
    memory_low: bool,
    /// El reintento automático de §4 (lo instala `ez watch`).
    auto_retry: Option<Arc<dyn AutoRetry>>,
}

impl Watcher {
    /// Un vigilante del padre de `config`.
    pub fn new(config: WatchConfig, registry: Registry, runtime: Arc<dyn WatchRuntime>) -> Self {
        let now = runtime.now_ms();
        Self {
            config,
            registry,
            runtime,
            notice_sink: None,
            parentless: false,
            seen_at: HashMap::new(),
            active_at: now,
            novelty_file: Some(ez_core::dirs::app_data_dir().join("novedades.toml")),
            screens: HashMap::new(),
            warned_invalid: HashSet::new(),
            memory: None,
            memory_low: false,
            auto_retry: None,
        }
    }

    /// Otro `novedades.toml` (las pruebas nunca leen el de la persona).
    pub fn with_novelty_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.novelty_file = Some(path.into());
        self
    }

    /// El reintento automático de §4 (`ez watch` lo implementa con `ez-scan`).
    pub fn with_auto_retry(mut self, hook: Arc<dyn AutoRetry>) -> Self {
        self.auto_retry = Some(hook);
        self
    }

    /// Otro destinatario de los avisos (la app de voz, en vez de `hc send`).
    pub fn with_notice_sink(mut self, sink: Arc<dyn NoticeSink>) -> Self {
        self.notice_sink = Some(sink);
        self
    }

    /// El padre no es una terminal: su vida es la del proceso que lo corre.
    ///
    /// Sin `hc get` de vida y sin el filtro `ez.parent`: el registro del padre
    /// es la única lista de hijos y se cruzan por la dirección de su terminal.
    pub fn without_parent_terminal(mut self) -> Self {
        self.parentless = true;
        self
    }

    /// Pregunta al gancho si corresponde reintentar un bloqueo por límite.
    pub(super) fn trigger_auto_retry(&self, record: &Record, cause: &str, tick: &mut Tick) {
        let Some(hook) = &self.auto_retry else {
            return;
        };
        match hook.auto_retry(record, cause) {
            Ok(Some(name)) => tick.retried.push(name),
            Ok(None) => {}
            Err(error) => tracing::warn!("EzAgent: no pude reintentar {}: {error}", record.name),
        }
    }

    /// Una vuelta: mira las terminales, marca lo que terminó y avisa.
    pub fn tick(&mut self) -> Tick {
        let mut tick = Tick::default();
        let outcome = self.records();
        let Some(children) = self.children() else {
            // DECISION: si `hc` no responde no se cambia nada: se reintenta en
            // la vuelta siguiente en vez de dar por muerto al subagente.
            return tick;
        };
        let alive = if self.parentless {
            true
        } else {
            self.parent_alive()
        };
        if !self.parentless {
            tick.parent_alive = Some(alive);
        }
        if !alive {
            return tick;
        }
        self.mark_failed(&outcome.records, &children, &mut tick);
        // DECISION: se releen los registros: el aviso sale del estado nuevo
        // (por ejemplo, «falló» y no «está en proceso»).
        let fresh = self.records();
        self.notify_invalid(&fresh.invalid);
        self.check_memory(&mut tick);
        self.detect_novelties(&fresh.records, &children, &mut tick);
        let fresh = self.records();
        self.notify(&fresh.records, &mut tick);
        self.refresh_activity();
        tick
    }

    /// Los registros del padre sin que un archivo dañado oculte a los demás.
    fn records(&self) -> ListOutcome {
        // DECISION: un error de la carpeta se reintenta en la vuelta siguiente
        // en vez de dar por terminado al vigilante.
        self.registry
            .list_lenient(&self.config.parent)
            .unwrap_or_default()
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
    ///
    /// Con un padre que no es terminal no hay etiqueta `ez.parent` que filtrar:
    /// se listan todas y el registro decide cuáles importan.
    fn children(&self) -> Option<Vec<Child>> {
        let mut args = vec!["ls".to_string(), "--all".to_string(), "--json".to_string()];
        if !self.parentless {
            args.push("--label".to_string());
            args.push(format!("ez.parent={}", self.config.parent));
        }
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
            if !matches!(record.state, TaskState::InProgress | TaskState::Blocked) {
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
            if !self.send(Some(record), &text) {
                // DECISION: si el aviso no llega, se reintenta en la vuelta
                // siguiente (el registro sigue sin avisar).
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

    /// Avisa una sola vez por archivo ilegible y sigue con los demás; nunca
    /// borra ni reescribe el archivo.
    fn notify_invalid(&mut self, invalid: &[InvalidRecord]) {
        for item in invalid {
            if self.warned_invalid.contains(&item.path) {
                continue;
            }
            let text = format!(
                "[EzAgent · aviso] Registro ilegible: {}. Lo salto; revisa el archivo.",
                item.path.display()
            );
            if self.send(None, &text) {
                self.warned_invalid.insert(item.path.clone());
            }
        }
    }

    /// El `NoticeSink` si lo hay; si no, `hc send <padre> <aviso>`.
    fn send(&self, record: Option<&Record>, text: &str) -> bool {
        if let Some(sink) = &self.notice_sink {
            return sink.deliver(record, text);
        }
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
        let outcome = self.records();
        let pending = outcome
            .records
            .iter()
            .any(|record| matches!(record.state, TaskState::InProgress | TaskState::Blocked))
            || !pending_notices(&outcome.records).is_empty();
        if pending {
            self.active_at = self.runtime.now_ms();
        }
    }
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
///
/// DECISION: `hc` escribe el JSON de error en `stderr` (verificado en vivo el
/// 19-nov), así que se lee el que venga en cualquiera de los dos.
pub fn ensure_watcher(runtime: &dyn WatchRuntime, setup: &WatcherSetup) -> WatcherState {
    let args = watcher_args(setup);
    let output = match runtime.hc(&args) {
        Ok(output) => output,
        Err(error) => return WatcherState::Failed(error),
    };
    let data = parse_json(&output.stdout).or_else(|| parse_json(&output.stderr));
    if let Some(data) = data {
        if let Some(error) = data.get("error").filter(|value| truthy(value)) {
            let kind = error.get("kind").and_then(Value::as_str);
            if kind == Some("name_taken") {
                return WatcherState::AlreadyRunning;
            }
            let message = error_message(error);
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

/// El JSON de `hc`, venga por `stdout` o por `stderr`.
fn parse_json(text: &str) -> Option<Value> {
    let text = ez_core::pyvalue::py_strip(text);
    if text.is_empty() {
        return None;
    }
    ez_core::pyloads::loads(text).ok()
}

/// El mensaje de un error de `hc`.
fn error_message(error: &Value) -> String {
    match error {
        Value::Object(spec) => spec
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "no pude arrancar el vigilante".to_string()),
        other => ez_core::pyvalue::python_str(other),
    }
}

//! La app de voz como padre de sus subagentes (NV-3).
//!
//! La app lanza agentes por `delegate_to_agent`; aquí deja su registro
//! (`ez_herdr::subagents::Registry` bajo el padre fijo `voz`), lo vigila con el
//! `Watcher` de `ez-herdr` (bloqueos, fallos sin informe y «sin avance»),
//! anuncia cada novedad por voz y en el chat, responde «¿cómo va X?» con los
//! datos del registro y ofrece/ejecuta `ez retry --auto`.
//!
//! La app no es una terminal de Herdr: el padre no tiene `hc get` de vida y los
//! avisos no van por `hc send`, sino al mismo camino de anuncios de la app.

mod phrases;
mod port;
#[cfg(test)]
mod tests;

pub use phrases::{news_text, overview_phrase, parse_status_query, status_phrase, StatusQuery};
pub(crate) use port::AppHerdrPort;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use ez_agent::registry::{ToolResult, ToolSpec};
use ez_core::pytime::timestamp;
use ez_core::pyvalue::py_strip;
use ez_herdr::herdr::system_runtime;
use ez_herdr::subagents::{
    one_line, HcRuntime, NoticeSink, Record, Registry, TaskState, WatchConfig, Watcher,
    DEFAULT_GRACE_MS, DEFAULT_IDLE_MS, DEFAULT_INTERVAL_MS,
};
use serde_json::{json, Value};

use super::core::state::Inner;
use super::core::{SubagentWatch, VoiceApp};

/// El padre fijo de los subagentes de la app (§13): un nombre de `hc` válido
/// que no existe como terminal, porque la app no vive en Herdr.
pub const VOICE_PARENT: &str = "voz";

/// Hilo que ejecuta `ez retry`.
const RETRY_THREAD: &str = "ezagent-retry";
/// `CREATE_NO_WINDOW`: `ez` no abre una consola.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

impl VoiceApp {
    /// Arranca el vigilante de `voz` en su hilo (el `Watcher` de `ez-herdr`).
    ///
    /// En producción sale con el `hc` de la configuración; en las pruebas y el
    /// humo entra por [`SubagentWatch`]. Sin ninguno de los dos (dobles de
    /// prueba) no se arranca nada.
    // DECISION: el vigilante se activa con `watch_subagents` (solo
    // `VoiceApp::new`); el arnés de replay y los dobles no abren `hc`.
    pub(crate) fn start_subagent_watch(&self, setup: Option<SubagentWatch>, watch: bool) {
        let setup = match setup {
            Some(setup) => setup,
            None if watch => SubagentWatch {
                runtime: Arc::new(HcRuntime::new(
                    system_runtime(),
                    self.inner.agents.herdr.hc_path(),
                )),
                novelty_file: None,
                interval_ms: DEFAULT_INTERVAL_MS,
                grace_ms: DEFAULT_GRACE_MS,
            },
            None => return,
        };
        let SubagentWatch {
            runtime,
            novelty_file,
            interval_ms,
            grace_ms,
        } = setup;
        let app = self.clone();
        let stop = Arc::clone(&self.inner.agents.subagents_watch_stop);
        self.start_worker("ezagent-subagents-watch", move || {
            let config = WatchConfig {
                parent: VOICE_PARENT.to_string(),
                interval_ms,
                idle_ms: DEFAULT_IDLE_MS,
                grace_ms,
            };
            // El registro del vigilante es otro mango de la misma carpeta.
            let registry = Registry::at(app.inner.agents.registry.root().to_path_buf());
            let mut watcher = Watcher::new(config, registry, runtime)
                .without_parent_terminal()
                .with_notice_sink(Arc::new(AppNoticeSink {
                    inner: Arc::downgrade(&app.inner),
                }));
            if let Some(file) = novelty_file {
                watcher = watcher.with_novelty_file(file);
            }
            while !stop.is_set() {
                let _ = watcher.tick();
                if stop.wait_timeout(interval_ms as f64 / 1000.0) {
                    break;
                }
            }
        });
    }

    /// Anuncia las novedades pendientes del registro (una vez por episodio).
    ///
    /// Es el respaldo del vigilante: si el `hc` no responde, un `ez report`
    /// todavía se anuncia en la siguiente vuelta.
    pub(crate) fn poll_subagents(&self) {
        if self.inner.session.shutdown_done.load(Ordering::SeqCst) {
            return;
        }
        let registry = Arc::clone(&self.inner.agents.registry);
        let Ok(outcome) = registry.list_lenient(VOICE_PARENT) else {
            return;
        };
        for record in outcome.records {
            // §13: el padre no se avisa de lo que hizo él mismo.
            if record
                .history
                .last()
                .is_some_and(|change| change.by == record.parent)
            {
                continue;
            }
            let Some(text) = news_text(&record) else {
                continue;
            };
            if !self.take_subagent_news(&record) {
                continue;
            }
            self.emit_subagent_news(Some(&record), &text, true);
        }
    }

    /// Lo que ve el vigilante cuando entrega un aviso (`NoticeSink`).
    ///
    /// El texto corto lo escribe `news_text` con el registro; el del vigilante
    /// solo se usa cuando no hay estado nuevo (un «sin avance» o un archivo
    /// ilegible). Nada del texto se ejecuta.
    // DECISION: la app prefiere su frase corta de NV-3; el texto largo del
    // vigilante queda para lo que no es un cambio de estado.
    pub(crate) fn subagent_notice(&self, record: Option<&Record>, fallback: &str) {
        match record {
            Some(record) => match news_text(record) {
                Some(text) => {
                    if self.take_subagent_news(record) {
                        self.emit_subagent_news(Some(record), &text, false);
                    }
                }
                // El «sin avance» no cambia el estado: el vigilante ya lo
                // limita a una vez por episodio.
                None if record.state == TaskState::InProgress => {
                    self.emit_subagent_news(Some(record), fallback, false);
                }
                // `fallido` con «reemplazado por …»: el reintento ya lo contó.
                None => {}
            },
            None => self.emit_subagent_news(None, fallback, false),
        }
    }

    /// Marca la novedad como contada en esta sesión, si aún no lo estaba.
    ///
    /// La misma cerradura la usan el latido y el vigilante, así que una
    /// novedad se anuncia una sola vez aunque los dos la vean a la vez.
    fn take_subagent_news(&self, record: &Record) -> bool {
        let mut announced = self.inner.agents.announced.lock();
        let seen = announced.get(&record.name).copied();
        if seen == Some(record.state) {
            return false;
        }
        if seen.is_none() && record.notified == Some(record.state) {
            return false;
        }
        announced.insert(record.name.clone(), record.state);
        true
    }

    /// Chat y voz del aviso; con `mark_disk` lo deja anotado en el registro.
    fn emit_subagent_news(&self, record: Option<&Record>, text: &str, mark_disk: bool) {
        self.inner.chat.chat.add_note(text);
        let muted = timestamp() < *self.inner.session.mute_until.lock();
        if !muted {
            if let Some(record) = record.filter(|record| record.state == TaskState::Blocked) {
                *self.inner.agents.retry_offer.lock() = Some(record.name.clone());
            }
            self.queue_announcement(text, true);
        }
        if mark_disk {
            self.mark_subagent_notified(record);
        }
    }

    /// Guarda el `notified` fuera de la UI: el candado del registro puede
    /// esperar a otro escritor. Si el subagente ya cambió otra vez, no se pisa.
    fn mark_subagent_notified(&self, record: Option<&Record>) {
        let Some(record) = record else {
            return;
        };
        let app = self.clone();
        let registry = Arc::clone(&self.inner.agents.registry);
        let name = record.name.clone();
        let state = record.state;
        self.start_worker("ezagent-subagents-mark", move || {
            let current = app.inner.agents.announced.lock().get(&name).copied();
            if current != Some(state) {
                return;
            }
            if let Err(error) = registry.mark_notified(VOICE_PARENT, &name, state) {
                tracing::warn!(
                    "EzAgent: no pude marcar el aviso de {name}: {}",
                    error.message()
                );
            }
        });
    }

    /// La oferta de reintento pendiente, si la hay (pruebas y humo).
    pub fn pending_retry(&self) -> Option<String> {
        self.inner.agents.retry_offer.lock().clone()
    }

    /// Responde «¿cómo va X?» con el registro; el texto lo escribe el código.
    pub fn subagent_status_phrase(&self, name: &str) -> String {
        match self.inner.agents.registry.get(VOICE_PARENT, name) {
            Ok(Some(record)) => status_phrase(&record),
            Ok(None) => format!("No tengo el registro de {name}."),
            Err(error) => format!(
                "No pude leer el registro de {name}: {}.",
                one_line(error.message(), 160)
            ),
        }
    }

    /// Responde «¿cómo van los agentes?» con la lista del registro.
    pub fn subagents_overview_phrase(&self) -> String {
        match self.inner.agents.registry.list_lenient(VOICE_PARENT) {
            Ok(outcome) => overview_phrase(&outcome.records),
            Err(error) => format!(
                "No pude leer mis subagentes: {}.",
                one_line(error.message(), 160)
            ),
        }
    }

    /// Un «sí»/«dale» tras una oferta de reintento, o `false` si no aplica.
    ///
    /// La oferta caduca con cualquier otra frase: la persona puede seguir
    /// hablando sin que un «ok» viejo relance nada.
    pub(crate) fn answer_retry_offer(&self, text: &str) -> bool {
        let Some(name) = self.inner.agents.retry_offer.lock().clone() else {
            return false;
        };
        let key = plain_key(text);
        if ACCEPT_WORDS.contains(&key.as_str()) {
            *self.inner.agents.retry_offer.lock() = None;
            self.start_retry(&name);
            return true;
        }
        if REFUSE_WORDS.contains(&key.as_str()) {
            *self.inner.agents.retry_offer.lock() = None;
            let text = "Listo, lo dejo así.";
            self.inner.chat.chat.add_note(text);
            self.queue_announcement(text, false);
            return true;
        }
        *self.inner.agents.retry_offer.lock() = None;
        false
    }

    /// Marca `fallido` el registro de un lanzamiento asíncrono que no llegó.
    pub(crate) fn subagent_launch_failed(&self, address: &str, error: &str) {
        let registry = &self.inner.agents.registry;
        let Ok(outcome) = registry.list_lenient(VOICE_PARENT) else {
            return;
        };
        let detail = one_line(error, 300);
        for record in outcome.records.iter().filter(|record| {
            record.terminal == address
                && matches!(record.state, TaskState::InProgress | TaskState::Blocked)
        }) {
            // El cambio es del padre: no se anuncia, porque la herramienta ya
            // le devolvió el error al turno que lo pidió.
            if let Err(failed) = registry.transition(
                VOICE_PARENT,
                &record.name,
                TaskState::Failed,
                VOICE_PARENT,
                &detail,
                |_| {},
            ) {
                tracing::warn!(
                    "EzAgent: no pude marcar {} como fallido: {}",
                    record.name,
                    failed.message()
                );
            }
        }
    }

    /// Ejecuta el reintento en un hilo y cuenta el resultado.
    fn start_retry(&self, name: &str) {
        if !ez_herdr::herdr::valid_name(name) {
            let text = "Ese nombre no sirve para reintentar.";
            self.inner.chat.chat.add_note(text);
            self.queue_announcement(text, false);
            return;
        }
        let opening = format!("Dale, reintento {name} con la recomendación.");
        self.inner.chat.chat.add_note(&opening);
        self.queue_announcement(&opening, false);
        let app = self.clone();
        let name = name.to_string();
        self.start_worker(RETRY_THREAD, move || {
            let result = match app.inner.runtime.retry_runner.clone() {
                Some(runner) => runner(&name),
                None => run_ez_retry(&name),
            };
            let text = match result {
                Ok(new_name) if new_name != name => {
                    format!("Listo: {name} sigue como {new_name}.")
                }
                Ok(_) => format!("Listo: relancé {name}."),
                Err(error) => {
                    format!("No pude reintentar {name}: {}.", one_line(&error, 160))
                }
            };
            app.inner.chat.chat.add_note(&text);
            app.queue_announcement(&text, false);
        });
    }
}

/// El `NoticeSink` del vigilante de `voz`: lleva el aviso a la app.
struct AppNoticeSink {
    inner: Weak<Inner>,
}

impl NoticeSink for AppNoticeSink {
    fn deliver(&self, record: Option<&Record>, text: &str) -> bool {
        let Some(inner) = self.inner.upgrade() else {
            return false;
        };
        let app = VoiceApp { inner };
        if app.inner.session.shutdown_done.load(Ordering::SeqCst) {
            // Al cerrar no hay a quién anunciar; se da por entregado para no
            // dejar al vigilante reintentando.
            return true;
        }
        app.subagent_notice(record, text);
        true
    }
}

/// Palabras que aceptan la oferta de reintento (se comparan enteras).
const ACCEPT_WORDS: [&str; 9] = [
    "si", "dale", "hazlo", "adelante", "claro", "vale", "ok", "bueno", "listo",
];
/// Palabras que la rechazan.
const REFUSE_WORDS: [&str; 6] = ["no", "no gracias", "dejalo", "deja", "espera", "nada"];

/// Minúsculas, sin tildes ni signos: «Sí, dale.» → `si dale`.
pub(crate) fn plain_key(text: &str) -> String {
    let folded = ez_core::pyvalue::strip_accents(&text.to_lowercase());
    let mut out = String::with_capacity(folded.len());
    let mut pending_space = false;
    for character in folded.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(character);
        } else {
            pending_space = true;
        }
    }
    out
}

/// `ez retry <nombre> --auto` con argumentos en lista y sin shell.
///
/// `HC_TERMINAL_ID` hace que `ez` busque el registro bajo el padre de la app.
/// El nombre ya pasó `valid_name`; nunca se le pasa texto del subagente.
// DECISION: se usa el `ez` compilado en vez de duplicar la elección `--auto`
// de `ez-ctl`/IR-3 (lo autoriza la adenda de NV-3).
fn run_ez_retry(name: &str) -> Result<String, String> {
    use std::process::Command;

    let program = ez_program();
    let mut command = Command::new(&program);
    command.args(["retry", name, "--auto", "--json"]);
    command.env("HC_TERMINAL_ID", VOICE_PARENT);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|error| format!("no pude ejecutar {}: {error}", program.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stderr
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("ez retry falló");
        return Err(one_line(line, 200));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let row: serde_json::Map<String, Value> = serde_json::from_str(stdout.trim())
        .map_err(|error| format!("respuesta ilegible de ez retry: {error}"))?;
    let saved = row.get("name").and_then(Value::as_str).unwrap_or_default();
    let saved = py_strip(saved);
    Ok(if saved.is_empty() {
        name.to_string()
    } else {
        saved.to_string()
    })
}

/// El `ez` que instala EzAgent, junto a la app; si no, el del PATH.
fn ez_program() -> PathBuf {
    // DECISION: se busca `ez.exe` en la carpeta del ejecutable (donde lo deja
    // el instalador de EzAgent) y, si no está, se deja resolver por el PATH.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(folder) = exe.parent() {
            for candidate in [folder.join("ez.exe"), folder.join("../ez.exe")] {
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    PathBuf::from("ez")
}

/// Herramienta del LLM para las paráfrasis de «¿cómo va X?».
pub(crate) fn status_tool(registry: Arc<Registry>) -> ToolSpec {
    let mut tool = ToolSpec::new(
        "subagent_status",
        "Consulta el estado de los subagentes que lanzó la app (el registro de \
         EzAgent). Con name devuelve ese subagente; sin name, la lista. Úsala \
         cuando pregunten cómo va un agente o una tarea delegada.",
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Nombre del subagente; omítelo para ver todos."
                }
            }
        }),
        // Solo lee el registro: no ejecuta nada.
        0,
        Arc::new(move |args: &serde_json::Map<String, Value>| {
            let name = args
                .get("name")
                .and_then(Value::as_str)
                .map(py_strip)
                .unwrap_or_default()
                .to_string();
            let message = if name.is_empty() {
                match registry.list_lenient(VOICE_PARENT) {
                    Ok(outcome) => overview_phrase(&outcome.records),
                    Err(error) => format!(
                        "No pude leer mis subagentes: {}.",
                        one_line(error.message(), 160)
                    ),
                }
            } else {
                match registry.get(VOICE_PARENT, &name) {
                    Ok(Some(record)) => status_phrase(&record),
                    Ok(None) => format!("No tengo el registro de {name}."),
                    Err(error) => format!(
                        "No pude leer el registro de {name}: {}.",
                        one_line(error.message(), 160)
                    ),
                }
            };
            Ok(ToolResult::ok(json!({ "mensaje": message })).into())
        }),
    );
    // El resumen viene del subagente: se muestra como dato, no como orden.
    tool.untrusted_output = true;
    tool.timeout_s = 10.0;
    tool
}

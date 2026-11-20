//! La app de voz como padre de sus subagentes (NV-3).
//!
//! La app lanza agentes por `delegate_to_agent`; aquí deja su registro
//! (`ez_herdr::subagents::Registry` bajo el padre fijo `voz`), anuncia cada
//! novedad por voz y en el chat, responde «¿cómo va X?» con los datos del
//! registro y ofrece/ejecuta `ez retry --auto`.
//!
//! La app no es una terminal de Herdr: el registro es la única fuente y la
//! pantalla del subagente nunca se lee ni se ejecuta.

mod phrases;
mod port;
#[cfg(test)]
mod tests;

pub(crate) use port::AppHerdrPort;
pub use phrases::{news_text, overview_phrase, parse_status_query, status_phrase, StatusQuery};

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use ez_agent::registry::{ToolResult, ToolSpec};
use ez_core::pytime::timestamp;
use ez_core::pyvalue::py_strip;
use ez_herdr::subagents::{one_line, Registry, TaskState};
use serde_json::{json, Value};

use super::core::VoiceApp;

/// El padre fijo de los subagentes de la app (§13): un nombre de `hc` válido
/// que no existe como terminal, porque la app no vive en Herdr.
pub const VOICE_PARENT: &str = "voz";

/// Hilo que ejecuta `ez retry`.
const RETRY_THREAD: &str = "ezagent-retry";
/// `CREATE_NO_WINDOW`: `ez` no abre una consola.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

impl VoiceApp {
    /// Anuncia las novedades pendientes del registro (una vez por episodio).
    pub(crate) fn poll_subagents(&self) {
        if self.inner.session.shutdown_done.load(Ordering::SeqCst) {
            return;
        }
        let registry = Arc::clone(&self.inner.agents.registry);
        let Ok(outcome) = registry.list_lenient(VOICE_PARENT) else {
            return;
        };
        for record in outcome.records {
            if record.notified == Some(record.state) {
                continue;
            }
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
            self.inner.chat.chat.add_note(&text);
            let muted = timestamp() < *self.inner.session.mute_until.lock();
            if !muted {
                if record.state == TaskState::Blocked {
                    *self.inner.agents.retry_offer.lock() = Some(record.name.clone());
                }
                self.queue_announcement(&text, true);
            }
            if let Err(error) = registry.mark_notified(VOICE_PARENT, &record.name, record.state) {
                tracing::warn!(
                    "EzAgent: no pude marcar el aviso de {}: {}",
                    record.name,
                    error.message()
                );
            }
        }
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

/// Palabras que aceptan la oferta de reintento (se comparan enteras).
const ACCEPT_WORDS: [&str; 9] = [
    "si", "dale", "hazlo", "adelante", "claro", "vale", "ok", "bueno", "listo",
];
/// Palabras que la rechazan.
const REFUSE_WORDS: [&str; 6] = ["no", "no gracias", "dejalo", "deja", "espera", "nada"];

/// Minúsculas, sin tildes ni signos: «Sí, dale.» → `si dale`.
fn plain_key(text: &str) -> String {
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
            let candidate = folder.join("ez.exe");
            if candidate.is_file() {
                return candidate;
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

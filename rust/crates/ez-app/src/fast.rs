//! Port of `src/ezagent/voice/app.py` (`_fast_command`) (F3-16).
//!
//! Órdenes de control que no necesitan pasar por el LLM: agradecimientos,
//! saludos, listar agentes o tareas, silenciar el micrófono, cerrar la escucha
//! y repetir lo último dicho. El Python hace los efectos dentro del método; en
//! Rust devuelve un [`FastOutcome`] con lo observable para que el hilo de UI lo
//! aplique sin bloquearse.

use std::path::Path;

use ez_audio::wake::DEFAULT_WAKE_PHRASE;
use ez_core::pyvalue::{
    char_len, collapse_spaces, head_chars, py_rstrip, py_split, py_strip, strip_accents,
    text_or_empty,
};
use serde_json::{Map, Value};
use tracing::info;

/// Cortesías que se quitan al principio antes de buscar un atajo local.
/// «Ok, gracias. Mutéate» debe silenciar igual que «mutéate».
const COURTESY_WORDS: [&str; 15] = [
    "ok", "okay", "oki", "bueno", "listo", "perfecto", "dale", "genial", "gracias", "muchas",
    "mil", "eh", "este", "pues", "va",
];
const GREET_WORDS: [&str; 4] = ["hola", "buenas", "hola ezagent", "buenas ezagent"];
const AGENT_WORDS: [&str; 5] = [
    "que agentes hay",
    "que agentes tengo",
    "lista los agentes",
    "agentes vivos",
    "cuantos agentes hay",
];
const TASK_WORDS: [&str; 12] = [
    "que tareas tienes",
    "que tareas pendientes",
    "que tareas pendientes tengo",
    "que tareas tengo pendientes",
    "que estas vigilando",
    "que tareas estas vigilando",
    "tareas pendientes",
    "que tareas sigues",
    "que tareas hay pendientes",
    "que esta pendiente",
    "que hay pendiente",
    "como van las tareas",
];
const MUTE_WORDS: [&str; 12] = [
    "silencia el micro",
    "silencia el microfono",
    "silencio el micro",
    "silencio el microfono",
    "mutea el micro",
    "mutea el microfono",
    "silencia mi micro",
    "silencia mi microfono",
    "apaga el micro",
    "apaga el microfono",
    "mutea",
    "muteate",
];
const UNMUTE_WORDS: [&str; 6] = [
    "desmute",
    "desmutea",
    "unmute",
    "activa el micro",
    "reactiva el micro",
    "activa el microfono",
];
const CLOSE_WORDS: [&str; 10] = [
    "cierra",
    "cierrate",
    "cierra la escucha",
    "cierra el micro",
    "deja de escuchar",
    "callate",
    "pausa",
    "pausa la escucha",
    "pon pausa",
    "pausate",
];
const HANGUP_WORDS: [&str; 4] = ["adios", "cuelga", "termina la llamada", "colgar"];
const REPEAT_WORDS: [&str; 3] = ["repite", "repitelo", "otra vez"];

const STATE_NAMES: [(&str, &str); 4] = [
    ("working", "trabajando"),
    ("done", "terminado"),
    ("blocked", "bloqueado"),
    ("idle", "inactivo"),
];

/// Texto del aviso cuando Herdr no responde.
pub const AGENTS_UNAVAILABLE: &str = "No pude consultar Herdr.";
/// Nombre del hilo que lista agentes en Python (`ezagent-list-agents`).
pub const LIST_AGENTS_THREAD: &str = "ezagent-list-agents";

/// Modo de cierre pedido por un atajo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseMode {
    Pause,
    Hangup,
}

impl CloseMode {
    pub fn as_str(self) -> &'static str {
        match self {
            CloseMode::Pause => "pause",
            CloseMode::Hangup => "hangup",
        }
    }
}

/// Lo que `_fast_command` pide hacer. Los textos van con el tipo que usa
/// `_speak` en Python: siempre `"answer"` para los atajos.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FastOutcome {
    pub handled: bool,
    pub speech: Option<(&'static str, String)>,
    /// Nuevo estado del micrófono; `None` si el atajo no lo toca.
    pub mic: Option<bool>,
    /// Cierre tras responder; `None` si el atajo no cierra.
    pub close: Option<CloseMode>,
    /// Hay que listar agentes en un hilo de trabajo ([`LIST_AGENTS_THREAD`]).
    pub list_agents: bool,
    /// Hay que refrescar y mostrar la tarjeta de actividad.
    pub reveal_activity: bool,
}

/// Lo que `_fast_command` necesita de la app. Cada método corresponde a un
/// atributo del objeto Python; los valores por defecto reproducen la
/// configuración de fábrica.
// DECISION: `_fast_command` es un método de `VoiceApp` con efectos de hilos y
// UI; el trait sustituye el duck typing como `ActivitySource` en F3-15 y deja
// los efectos en la app. `pause_message` es de la app (no es exclusivo de este
// atajo), así que aquí solo se pide ya calculado.
pub trait FastHost {
    /// `self._close_action()`.
    fn close_action(&self) -> String {
        "pause".to_string()
    }

    /// `self.last_summary`.
    fn last_summary(&self) -> String {
        String::new()
    }

    /// `self._pause_message()` con `voice_wake` y la frase de fábrica.
    fn pause_message(&self) -> String {
        format!("En pausa. Di «{DEFAULT_WAKE_PHRASE}» cuando me necesites.")
    }

    /// `self.tasks.all()`.
    fn pending_tasks(&self) -> Vec<Map<String, Value>> {
        Vec::new()
    }

    /// `self.herdr.label_for(address)`.
    fn label_for(&self, _address: &str) -> String {
        String::new()
    }

    /// `self._agent_label(agent)`.
    fn agent_label(&self, agent: &str) -> String {
        if agent.is_empty() {
            "El agente".to_string()
        } else {
            agent.to_string()
        }
    }
}

/// `VoiceApp._normalize`: minúsculas, sin tildes, sin signos y con un solo
/// espacio entre palabras. Incluye el `re.sub(r"\s+", " ", …)` de Python.
pub fn normalize(text: &str) -> String {
    let folded = strip_accents(&text.to_lowercase());
    let mut out = String::with_capacity(folded.len());
    let mut pending_space = false;
    for character in folded.chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
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

/// Quita cortesías iniciales; si no queda nada, devuelve `key` tal cual.
pub fn strip_courtesy(key: &str) -> String {
    let mut words: Vec<&str> = py_split(key).collect();
    while words
        .first()
        .is_some_and(|word| COURTESY_WORDS.contains(word))
    {
        words.remove(0);
    }
    let joined = words.join(" ");
    if joined.is_empty() {
        key.to_string()
    } else {
        joined
    }
}

/// Modo de cierre normalizado como `_close_action()` de Python (cualquier
/// cosa que no sea «hangup» es «pause»).
fn close_action(host: &dyn FastHost) -> CloseMode {
    if host.close_action().to_lowercase() == "hangup" {
        CloseMode::Hangup
    } else {
        CloseMode::Pause
    }
}

fn answer(text: impl Into<String>) -> FastOutcome {
    FastOutcome {
        handled: true,
        speech: Some(("answer", text.into())),
        ..FastOutcome::default()
    }
}

fn set_contains(set: &[&str], key: &str) -> bool {
    set.contains(&key)
}

/// Atajo local: `True` si el texto quedó atendido sin pasar por el LLM.
// DECISION: el texto es `&str`; los casos `None`/`5` del Python lanzan
// `AttributeError` y no son representables en Rust.
pub fn fast_command(text: &str, host: &dyn FastHost) -> FastOutcome {
    let raw_key = normalize(text);
    let key = strip_courtesy(&raw_key);
    let words: Vec<&str> = py_split(&raw_key).collect();
    if words.contains(&"gracias") && words.iter().all(|word| COURTESY_WORDS.contains(word)) {
        info!("Atajo local: agradecimiento.");
        return answer("De nada.");
    }
    if set_contains(&GREET_WORDS, &key) {
        info!("Atajo local: saludo.");
        return answer("Hola, ¿qué necesitas?");
    }
    if set_contains(&AGENT_WORDS, &key) {
        info!("Atajo local: listar agentes.");
        return FastOutcome {
            handled: true,
            list_agents: true,
            ..FastOutcome::default()
        };
    }
    if set_contains(&TASK_WORDS, &key) {
        info!("Atajo local: listar tareas pendientes.");
        return tasks_outcome(host);
    }
    control_command(&key, host).unwrap_or_default()
}

/// Atajos de micrófono, cierre y repetición, en el orden del Python.
fn control_command(key: &str, host: &dyn FastHost) -> Option<FastOutcome> {
    if set_contains(&MUTE_WORDS, key) {
        info!("Atajo local: mute.");
        let mut outcome = answer(host.pause_message());
        outcome.mic = Some(true);
        return Some(outcome);
    }
    if set_contains(&UNMUTE_WORDS, key) {
        info!("Atajo local: unmute.");
        let mut outcome = answer("Micrófono activo.");
        outcome.mic = Some(false);
        return Some(outcome);
    }
    if set_contains(&CLOSE_WORDS, key) || set_contains(&HANGUP_WORDS, key) {
        let hangup = set_contains(&HANGUP_WORDS, key) || close_action(host) == CloseMode::Hangup;
        info!(
            "Atajo local: {}.",
            if hangup {
                "colgar"
            } else {
                "pausar la escucha"
            }
        );
        let mut outcome = answer(if hangup {
            "Listo, cierro.".to_string()
        } else {
            host.pause_message()
        });
        outcome.close = Some(if hangup {
            CloseMode::Hangup
        } else {
            CloseMode::Pause
        });
        return Some(outcome);
    }
    if set_contains(&REPEAT_WORDS, key) {
        info!("Atajo local: repetir.");
        let summary = host.last_summary();
        return Some(answer(if summary.is_empty() {
            "No hay nada que repetir todavía.".to_string()
        } else {
            summary
        }));
    }
    None
}

/// Texto de `list_agents` en el hilo de trabajo de Python.
// DECISION: Python lista agentes en `ezagent-list-agents` para no bloquear el
// hilo de Tk; `fast_command` solo pide el trabajo y la app llama a esta función
// con el resultado, o usa `AGENTS_UNAVAILABLE` si Herdr falla.
pub fn agents_report(agents: &[Map<String, Value>]) -> String {
    if agents.is_empty() {
        return "No hay agentes vivos.".to_string();
    }
    let parts: Vec<String> = agents.iter().take(4).map(agent_part).collect();
    let extra = agents.len() - parts.len();
    let suffix = if extra > 0 {
        format!(", y {extra} más")
    } else {
        String::new()
    };
    format!(
        "Hay {} agentes: {}{}.",
        agents.len(),
        parts.join("; "),
        suffix
    )
}

fn agent_part(agent: &Map<String, Value>) -> String {
    let label = text_or_empty(agent.get("label"));
    let shown = if label.is_empty() {
        visible_name(agent)
    } else {
        label
    };
    let raw_state = text_or_empty(agent.get("state"));
    let state = if raw_state.is_empty() {
        "—".to_string()
    } else {
        STATE_NAMES
            .iter()
            .find(|(key, _)| *key == raw_state)
            .map(|(_, label)| (*label).to_string())
            .unwrap_or(raw_state)
    };
    format!("{shown}, {state}")
}

fn tasks_outcome(host: &dyn FastHost) -> FastOutcome {
    let tasks = host.pending_tasks();
    let mut outcome = FastOutcome {
        handled: true,
        reveal_activity: true,
        ..FastOutcome::default()
    };
    if tasks.is_empty() {
        outcome.speech = Some(("answer", "No tengo tareas pendientes.".to_string()));
        return outcome;
    }
    let parts: Vec<String> = tasks
        .iter()
        .take(4)
        .map(|item| task_part(item, host))
        .collect();
    let extra = tasks.len() - parts.len();
    let suffix = if extra > 0 {
        format!("; y {extra} más")
    } else {
        String::new()
    };
    outcome.speech = Some((
        "answer",
        format!(
            "Tengo {} tareas pendientes: {}{}.",
            tasks.len(),
            parts.join("; "),
            suffix
        ),
    ));
    outcome
}

fn task_part(item: &Map<String, Value>, host: &dyn FastHost) -> String {
    let address = text_or_empty(item.get("address"));
    let label = host.label_for(&address);
    let shown = if label.is_empty() {
        let agent = text_or_empty(item.get("agent"));
        host.agent_label(if agent.is_empty() { "agente" } else { &agent })
    } else {
        label
    };
    let mut task = collapse_spaces(&text_or_empty(item.get("task")));
    if char_len(&task) > 100 {
        task = format!("{}…", py_rstrip(head_chars(&task, 99)));
    }
    if task.is_empty() {
        task = "sin descripción".to_string();
    }
    format!("{shown}: {task}")
}

/// `herdr.visible_name` para un agente sin etiqueta visible.
// DECISION: `herdr.py` aún no está portado (`herdr.rs` es esqueleto); se
// reproduce aquí la función completa para no tocar archivos fuera del brief.
fn visible_name(item: &Map<String, Value>) -> String {
    let where_text = collapse_spaces(&text_or_empty(item.get("where")));
    let (mut space, mut tab) = (String::new(), String::new());
    if let Some((left, right)) = where_text.split_once(" / ") {
        space = py_strip(left).to_string();
        tab = py_strip(right).to_string();
    } else if !where_text.is_empty() {
        tab = where_text;
    }
    if space.is_empty() {
        space = collapse_spaces(&text_or_empty(item.get("space")));
    }
    if tab.is_empty() {
        tab = collapse_spaces(&text_or_empty(item.get("name")));
    }
    if !tab.is_empty() && tab.chars().all(char::is_numeric) {
        tab = format!("pestaña {tab}");
    }
    if !tab.is_empty() && !space.is_empty() {
        return format!("{tab} de {space}");
    }
    if !tab.is_empty() {
        return tab;
    }
    let raw_agent = text_or_empty(item.get("agent"));
    let raw_agent = py_strip(&raw_agent);
    let agent = match raw_agent.to_lowercase().as_str() {
        "claude-code" | "claude" => "Claude Code",
        "opencode" => "OpenCode",
        "codex" => "Codex",
        "gemini" => "Gemini",
        _ => raw_agent,
    };
    let cwd = text_or_empty(item.get("cwd"));
    let folder = Path::new(&cwd)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !agent.is_empty() && !folder.is_empty() {
        return format!("{agent} en {folder}");
    }
    if agent.is_empty() {
        "el agente".to_string()
    } else {
        agent.to_string()
    }
}

#[cfg(test)]
mod tests;

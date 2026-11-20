//! Lo pendiente que quedó al cerrar la app: se pregunta y nada se reanuda solo
//! (DX-4).
//!
//! Al abrir, las tareas, las cadenas y el enjambre guardado quedan retenidos
//! hasta que la persona diga «sí». «No» los deja guardados sin volver a
//! preguntar en esta sesión; «descártalo» los borra. El texto de la pregunta lo
//! escribe este código, nunca el LLM.

#[cfg(test)]
mod tests;

use ez_core::pytime::timestamp;
use ez_core::pyvalue::{collapse_spaces, head_chars, py_rstrip, py_strip, text_or_empty};
use serde_json::{Map, Value};

use super::super::core::{log, VoiceApp};
use super::super::subagents::plain_key;

/// Un enjambre guardado que espera el «sí».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldSwarm {
    /// Id en el tablero.
    pub id: String,
    /// Lo que pidió la persona.
    pub goal: String,
}

/// Lo pendiente al abrir, retenido hasta la respuesta.
#[derive(Debug, Clone)]
pub struct PendingResume {
    /// Filas de `tasks.json` (dirección, agente y tarea).
    pub tasks: Vec<Map<String, Value>>,
    /// Cadenas en espera de que termine su fuente.
    pub chains: Vec<Map<String, Value>>,
    /// Enjambres guardados, ya en pausa.
    pub swarms: Vec<HeldSwarm>,
    /// La frase que se dice y se escribe en el chat.
    pub text: String,
    /// La persona dijo «no»: solo «reanuda lo pendiente» los retoma.
    pub declined: bool,
}

impl PendingResume {
    /// ¿No hay nada que preguntar?
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty() && self.chains.is_empty() && self.swarms.is_empty()
    }
}

/// Palabras que aceptan la reanudación (se buscan sueltas).
const ACCEPT_WORDS: [&str; 9] = [
    "si",
    "dale",
    "reanuda",
    "reanudalo",
    "retoma",
    "retomalo",
    "hazlo",
    "adelante",
    "claro",
];
/// Frases que también aceptan.
const ACCEPT_PHRASES: [&str; 2] = ["de acuerdo", "esta bien"];
/// Palabras que la rechazan.
const REFUSE_WORDS: [&str; 8] = [
    "no", "dejalo", "deja", "despues", "luego", "espera", "nada", "tampoco",
];
/// Frases que la rechazan.
const REFUSE_PHRASES: [&str; 3] = ["no gracias", "ahora no", "mas tarde"];
/// Palabras que borran lo pendiente (además de «descártalo»).
const DISCARD_WORDS: [&str; 8] = [
    "descartalo",
    "descartalos",
    "borralo",
    "borralos",
    "eliminalo",
    "eliminalos",
    "olvidalo",
    "olvidalos",
];
/// Comandos que retoman lo que se dejó con un «no».
const RESUME_COMMANDS: [&str; 4] = ["reanuda", "reanudalo", "retoma", "retomalo"];

/// `row[key]` como texto recortado.
fn value_text(row: &Map<String, Value>, key: &str) -> String {
    py_strip(&text_or_empty(row.get(key))).to_string()
}

/// El nombre visible del agente, sin consultar Herdr.
fn agent_name(raw: &str) -> String {
    match raw.to_lowercase().as_str() {
        "claude" | "claude-code" => "Claude Code".to_string(),
        "opencode" => "OpenCode".to_string(),
        "codex" => "Codex".to_string(),
        "gemini" => "Gemini".to_string(),
        "mcode" | "minimax" => "MiniMax".to_string(),
        "qwen" => "Qwen".to_string(),
        "" => "un agente".to_string(),
        _ => raw.to_string(),
    }
}

/// `«tarea» (Agente)` para nombrar una tarea pendiente.
fn task_part(row: &Map<String, Value>) -> String {
    let mut task = collapse_spaces(&value_text(row, "task"));
    if task.is_empty() {
        task = "una tarea sin describir".to_string();
    }
    if task.chars().count() > 70 {
        task = format!("{}…", py_rstrip(head_chars(&task, 69)));
    }
    format!("«{task}» ({})", agent_name(&value_text(row, "agent")))
}

/// `una cadena hacia Agente`.
fn chain_part(row: &Map<String, Value>) -> String {
    let agent = agent_name(&value_text(row, "agent"));
    format!("una cadena hacia {agent}")
}

/// Junta las partes con comas y «y» antes de la última.
fn join_spanish(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [only] => only.clone(),
        _ => {
            let Some((last, head)) = parts.split_last() else {
                return String::new();
            };
            format!("{} y {last}", head.join(", "))
        }
    }
}

/// La frase única que nombra todo lo pendiente (código, nunca el LLM).
pub fn resume_question(
    tasks: &[Map<String, Value>],
    chains: &[Map<String, Value>],
    swarms: &[HeldSwarm],
) -> String {
    let mut parts: Vec<String> = Vec::new();
    for row in tasks.iter().take(2) {
        parts.push(task_part(row));
    }
    if tasks.len() > 2 {
        let extra = tasks.len() - 2;
        parts.push(if extra == 1 {
            "una tarea más".to_string()
        } else {
            format!("{extra} tareas más")
        });
    }
    for row in chains.iter().take(2) {
        parts.push(chain_part(row));
    }
    if chains.len() > 2 {
        let extra = chains.len() - 2;
        parts.push(if extra == 1 {
            "una cadena más".to_string()
        } else {
            format!("{extra} cadenas más")
        });
    }
    match swarms {
        [] => {}
        [only] => {
            let goal = collapse_spaces(&only.goal);
            parts.push(if goal.is_empty() {
                "el enjambre guardado".to_string()
            } else if goal.chars().count() > 60 {
                format!("el enjambre «{}…»", py_rstrip(head_chars(&goal, 59)))
            } else {
                format!("el enjambre «{goal}»")
            });
        }
        many => parts.push(format!("{} enjambres guardados", many.len())),
    }
    let total = tasks.len() + chains.len() + swarms.len();
    let list = join_spanish(&parts);
    if list.is_empty() {
        return String::new();
    }
    if total == 1 {
        format!("Quedó pendiente {list}. ¿Quieres que lo reanude?")
    } else {
        format!("Quedaron pendientes {list}. ¿Quieres que los reanude?")
    }
}

/// ¿La frase contiene alguna de las palabras (sueltas)?
fn has_word(key: &str, words: &[&str]) -> bool {
    key.split_whitespace().any(|word| words.contains(&word))
}

/// ¿La frase contiene alguna de las frases (en orden)?
fn has_phrase(key: &str, phrases: &[&str]) -> bool {
    phrases
        .iter()
        .any(|phrase| format!(" {key} ").contains(&format!(" {phrase} ")))
}

fn is_discard(key: &str) -> bool {
    has_word(key, &DISCARD_WORDS)
}

fn is_resume_command(key: &str) -> bool {
    has_word(key, &RESUME_COMMANDS)
}

fn is_refuse(key: &str) -> bool {
    has_word(key, &REFUSE_WORDS) || has_phrase(key, &REFUSE_PHRASES)
}

fn is_accept(key: &str) -> bool {
    !is_refuse(key) && (has_word(key, &ACCEPT_WORDS) || has_phrase(key, &ACCEPT_PHRASES))
}

impl VoiceApp {
    /// Al abrir: retiene lo pendiente y pregunta una vez (DX-4).
    ///
    /// No vigila ni lanza nada: solo deja el enjambre guardado en pausa y
    /// escribe la pregunta en el chat (y la dice, si no hay silencio).
    pub(crate) fn begin_pending_resume(&self) {
        if self.inner.agents.pending_resume.lock().is_some() {
            return;
        }
        let pending = self.collect_pending_resume();
        if pending.is_empty() {
            return;
        }
        log(format!("Pendiente al abrir: {}", pending.text));
        *self.inner.agents.pending_resume.lock() = Some(pending.clone());
        self.inner.chat.chat.add_note(&pending.text);
        if timestamp() < *self.inner.session.mute_until.lock() {
            // DECISION: en silencio la pregunta queda solo en el chat.
            return;
        }
        self.queue_announcement(&pending.text, true);
    }

    /// El arranque: retiene lo pendiente y, en producción, levanta el enjambre.
    pub(crate) fn start_pending_resume(&self) {
        self.begin_pending_resume();
        let stack = self.inner.agents.swarm.lock().clone();
        let Some(stack) = stack else {
            return;
        };
        if !self.inner.runtime.background {
            return;
        }
        if let Err(error) = stack.start() {
            log(format!("No pude arrancar el enjambre: {error}"));
        }
    }

    /// Todo lo que quedó pendiente, con el enjambre ya en pausa.
    fn collect_pending_resume(&self) -> PendingResume {
        let tasks: Vec<Map<String, Value>> = self
            .inner
            .agents
            .tasks
            .all()
            .into_iter()
            .filter(|row| !value_text(row, "address").is_empty())
            .collect();
        let chains = self.inner.agents.tasks.chains();
        let swarms = self.hold_saved_swarms();
        let text = resume_question(&tasks, &chains, &swarms);
        PendingResume {
            tasks,
            chains,
            swarms,
            text,
            declined: false,
        }
    }

    /// Deja los enjambres guardados en pausa (el supervisor no los despierta).
    fn hold_saved_swarms(&self) -> Vec<HeldSwarm> {
        let stack = self.inner.agents.swarm.lock().clone();
        let Some(stack) = stack else {
            return Vec::new();
        };
        let live = match stack.store().live() {
            Ok(live) => live,
            Err(error) => {
                log(format!("No pude leer los enjambres guardados: {error}"));
                return Vec::new();
            }
        };
        let mut held = Vec::with_capacity(live.len());
        for info in live {
            if info.status == "active" {
                if let Err(error) = stack.service.pause(&info.id) {
                    log(format!(
                        "No pude dejar el enjambre {} en pausa: {error}",
                        info.id
                    ));
                }
            }
            held.push(HeldSwarm {
                id: info.id,
                goal: info.goal,
            });
        }
        held
    }

    /// Un «sí», «no» o «descártalo» para lo pendiente del arranque.
    ///
    /// `true` si la frase era para esta pregunta; así el turno no sigue al LLM.
    pub(crate) fn answer_pending_resume(&self, text: &str) -> bool {
        let Some(pending) = self.inner.agents.pending_resume.lock().clone() else {
            return false;
        };
        let key = plain_key(text);
        if is_discard(&key) {
            self.discard_pending_resume(&pending);
            return true;
        }
        if pending.declined {
            // Tras un «no», un «sí» suelto puede responder a otra cosa.
            if is_resume_command(&key) {
                self.resume_pending_resume(pending);
                return true;
            }
            return false;
        }
        if is_refuse(&key) {
            if let Some(slot) = self.inner.agents.pending_resume.lock().as_mut() {
                slot.declined = true;
            }
            log("Lo pendiente queda guardado; no vuelvo a preguntar en esta sesión.");
            return true;
        }
        if is_accept(&key) {
            self.resume_pending_resume(pending);
            return true;
        }
        false
    }

    /// Reanuda en un hilo: `check_task_now` y la cadena hablan con Herdr.
    fn resume_pending_resume(&self, pending: PendingResume) {
        *self.inner.agents.pending_resume.lock() = None;
        let app = self.clone();
        self.start_worker("ezagent-resume-pending", move || {
            app.resume_pending_tasks();
            let stack = app.inner.agents.swarm.lock().clone();
            let Some(stack) = stack else {
                return;
            };
            for swarm in &pending.swarms {
                match stack.service.resume(&swarm.id) {
                    Ok(()) => stack.supervisor.poke(),
                    Err(error) => log(format!(
                        "No pude reanudar el enjambre {}: {error}",
                        swarm.id
                    )),
                }
            }
        });
    }

    /// Borra lo pendiente: tareas, cadenas y enjambres guardados.
    fn discard_pending_resume(&self, pending: &PendingResume) {
        let tasks = &self.inner.agents.tasks;
        for row in &pending.tasks {
            let address = value_text(row, "address");
            if address.is_empty() {
                continue;
            }
            if let Err(error) = tasks.remove(&address) {
                log(format!("No pude borrar la tarea de {address}: {error}"));
            }
            self.inner.agents.watched.lock().remove(&address);
            self.inner.agents.deliver.lock().remove(&address);
            self.inner.agents.sent.lock().remove(&address);
        }
        for chain in &pending.chains {
            if let Err(error) = tasks.remove_chain(chain) {
                log(format!("No pude borrar una cadena pendiente: {error}"));
            }
        }
        let stack = self.inner.agents.swarm.lock().clone();
        if let Some(stack) = stack {
            for swarm in &pending.swarms {
                if let Err(error) = stack.service.stop(&swarm.id) {
                    log(format!(
                        "No pude descartar el enjambre {}: {error}",
                        swarm.id
                    ));
                }
            }
        }
        *self.inner.agents.pending_resume.lock() = None;
        log("Descarté lo pendiente del arranque.");
    }
}

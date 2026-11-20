//! Lo pendiente que quedó al cerrar la app: se pregunta y nada se reanuda solo
//! (DX-4).
//!
//! Al abrir, las tareas, las cadenas y el enjambre guardado quedan retenidos
//! hasta que la persona diga «sí». «No» los deja guardados sin volver a
//! preguntar en esta sesión; «descártalo» los borra. El texto de la pregunta lo
//! escribe este código, nunca el LLM.

#[cfg(test)]
mod tests;

use std::sync::Arc;

use ez_core::pytime::timestamp;
use ez_core::pyvalue::{collapse_spaces, head_chars, py_rstrip, py_strip, text_or_empty};
use ez_swarm::stack::SwarmStack;
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
    /// Ya estaba en pausa antes de abrir: el «sí» no lo reactiva.
    pub was_paused: bool,
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
    /// No se pudo inventariar o pausar el enjambre: no se reanuda nada.
    pub blocked: Option<String>,
}

impl PendingResume {
    /// ¿No hay nada que retener?
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
            && self.chains.is_empty()
            && self.swarms.is_empty()
            && self.blocked.is_none()
    }
}

/// Lo que la retención al arrancar necesita del tablero del enjambre.
///
/// Producción lo cumple [`StackHold`]; las pruebas inyectan un doble que falla
/// en `live` o `pause` y demuestra que no se arranca el supervisor.
pub(crate) trait SwarmHold: Send + Sync {
    /// Enjambres vivos (`active` o `paused`) con su pausa previa.
    fn live(&self) -> Result<Vec<HeldSwarm>, String>;
    /// Deja en pausa un enjambre activo.
    fn pause(&self, id: &str) -> Result<(), String>;
    /// Reactiva un enjambre que la app puso en pausa al abrir.
    fn resume(&self, id: &str) -> Result<(), String>;
    /// Detiene un enjambre descartado.
    fn stop(&self, id: &str) -> Result<(), String>;
    /// Adelanta la pasada del supervisor.
    fn poke(&self);
    /// Arranca el servidor y el supervisor.
    fn start(&self) -> Result<(), String>;
    /// Arranca solo el servidor local (sin supervisor).
    fn start_server(&self) -> Result<(), String>;
}

/// Adaptador de producción sobre el [`SwarmStack`] de la app.
struct StackHold(Arc<SwarmStack>);

impl SwarmHold for StackHold {
    fn live(&self) -> Result<Vec<HeldSwarm>, String> {
        self.0
            .store()
            .live()
            .map(|live| {
                live.into_iter()
                    .map(|info| HeldSwarm {
                        id: info.id,
                        goal: info.goal,
                        was_paused: info.status == "paused",
                    })
                    .collect()
            })
            .map_err(|error| format!("No pude leer los enjambres guardados: {error}"))
    }

    fn pause(&self, id: &str) -> Result<(), String> {
        self.0
            .service
            .pause(id)
            .map_err(|error| format!("No pude dejar el enjambre {id} en pausa: {error}"))
    }

    fn resume(&self, id: &str) -> Result<(), String> {
        self.0
            .service
            .resume(id)
            .map_err(|error| format!("No pude reanudar el enjambre {id}: {error}"))
    }

    fn stop(&self, id: &str) -> Result<(), String> {
        self.0
            .service
            .stop(id)
            .map_err(|error| format!("No pude descartar el enjambre {id}: {error}"))
    }

    fn poke(&self) {
        self.0.supervisor.poke();
    }

    fn start(&self) -> Result<(), String> {
        self.0
            .start()
            .map_err(|error| format!("No pude arrancar el enjambre: {error}"))
    }

    fn start_server(&self) -> Result<(), String> {
        self.0
            .server
            .start()
            .map(|_| ())
            .map_err(|error| format!("No pude arrancar el servidor del enjambre: {error}"))
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

/// Las partes que nombran lo pendiente y su total.
fn pending_parts(
    tasks: &[Map<String, Value>],
    chains: &[Map<String, Value>],
    swarms: &[HeldSwarm],
) -> (Vec<String>, usize) {
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
    (parts, total)
}

/// La frase única que nombra todo lo pendiente (código, nunca el LLM).
pub fn resume_question(
    tasks: &[Map<String, Value>],
    chains: &[Map<String, Value>],
    swarms: &[HeldSwarm],
) -> String {
    let (parts, total) = pending_parts(tasks, chains, swarms);
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

/// El aviso de fallo cerrado: no se reanuda nada y lo pendiente sigue guardado.
fn blocked_text(
    reason: &str,
    tasks: &[Map<String, Value>],
    chains: &[Map<String, Value>],
    swarms: &[HeldSwarm],
) -> String {
    let reason = collapse_spaces(reason);
    let reason = py_rstrip(&reason);
    let reason = if reason.chars().count() > 160 {
        format!("{}…", py_rstrip(head_chars(reason, 159)))
    } else {
        reason.to_string()
    };
    let reason = reason.trim_end_matches('.').to_string();
    let (parts, _) = pending_parts(tasks, chains, swarms);
    let list = join_spanish(&parts);
    let tail = if list.is_empty() {
        String::new()
    } else {
        format!(" Sigue pendiente {list}.")
    };
    format!("{reason}. No reanudo nada.{tail}")
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
    /// Devuelve `true` solo si se pudo dejar todo en pausa; con un fallo, el
    /// arranque no debe levantar el supervisor.
    // DECISION: se pregunta como aviso (`announce`) y no como confirmación: así
    // `arm_confirmation` (F48) nunca rearma algo de una sesión anterior.
    // DECISION: no se vigila de forma pasiva hasta el «sí»: una cadena podría
    // lanzar su paso siguiente sola, que es justo lo que se evita.
    // DECISION: si no se puede inventariar o pausar el enjambre, la app falla
    // cerrada: informa el fallo, conserva lo pendiente y no reanuda nada.
    pub(crate) fn begin_pending_resume(&self) -> bool {
        if self.inner.agents.pending_resume.lock().is_some() {
            return self.pending_resume_is_safe();
        }
        let pending = self.collect_pending_resume();
        if pending.is_empty() {
            return true;
        }
        log(format!("Pendiente al abrir: {}", pending.text));
        *self.inner.agents.pending_resume.lock() = Some(pending.clone());
        self.inner.chat.chat.add_note(&pending.text);
        if timestamp() < *self.inner.session.mute_until.lock() {
            // DECISION: en silencio la pregunta queda solo en el chat.
            return pending.blocked.is_none();
        }
        self.queue_announcement(&pending.text, true);
        pending.blocked.is_none()
    }

    /// El arranque: retiene lo pendiente y, en producción, levanta el enjambre.
    ///
    // DECISION: el servidor y el supervisor del enjambre sí arrancan (los usa
    // `ezs` en la sesión), pero los enjambres guardados quedan en pausa antes
    // de que el supervisor haga su primera pasada; solo el «sí» los reactiva.
    // Con un fallo de retención solo arranca el servidor, nunca el supervisor.
    pub(crate) fn start_pending_resume(&self) {
        self.start_pending_resume_with(self.inner.runtime.background);
    }

    /// Igual que [`Self::start_pending_resume`] con el arranque forzado en pruebas.
    pub(crate) fn start_pending_resume_with(&self, background: bool) {
        let safe = self.begin_pending_resume();
        if !background {
            return;
        }
        let Some(hold) = self.swarm_hold() else {
            return;
        };
        let result = if safe {
            hold.start()
        } else {
            hold.start_server()
        };
        if let Err(error) = result {
            log(error);
        }
    }

    /// ¿La retención vigente se pudo asegurar?
    fn pending_resume_is_safe(&self) -> bool {
        self.inner
            .agents
            .pending_resume
            .lock()
            .as_ref()
            .is_none_or(|pending| pending.blocked.is_none())
    }

    /// El tablero del enjambre: el doble inyectado o el `SwarmStack` real.
    fn swarm_hold(&self) -> Option<Arc<dyn SwarmHold>> {
        if let Some(hold) = self.inner.agents.swarm_hold.lock().clone() {
            return Some(hold);
        }
        self.inner
            .agents
            .swarm
            .lock()
            .clone()
            .map(|stack| Arc::new(StackHold(stack)) as Arc<dyn SwarmHold>)
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
        let (swarms, blocked) = self.hold_saved_swarms();
        let text = match &blocked {
            Some(reason) => blocked_text(reason, &tasks, &chains, &swarms),
            None => resume_question(&tasks, &chains, &swarms),
        };
        PendingResume {
            tasks,
            chains,
            swarms,
            text,
            declined: false,
            blocked,
        }
    }

    /// Deja en pausa los enjambres activos; `Err` si no se pudo asegurar.
    fn hold_saved_swarms(&self) -> (Vec<HeldSwarm>, Option<String>) {
        let Some(hold) = self.swarm_hold() else {
            return (Vec::new(), None);
        };
        let live = match hold.live() {
            Ok(live) => live,
            Err(error) => return (Vec::new(), Some(error)),
        };
        let mut failure = None;
        let mut held = Vec::with_capacity(live.len());
        for swarm in live {
            if !swarm.was_paused {
                if let Err(error) = hold.pause(&swarm.id) {
                    failure = Some(error);
                }
            }
            held.push(swarm);
        }
        (held, failure)
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
        if pending.blocked.is_some() {
            // Fail closed: mientras no se pueda asegurar la pausa, no se reanuda.
            if is_refuse(&key) {
                if let Some(slot) = self.inner.agents.pending_resume.lock().as_mut() {
                    slot.declined = true;
                }
                return true;
            }
            if is_accept(&key) || is_resume_command(&key) {
                let text =
                    "No reanudo nada: sigue sin poder asegurarse la pausa del enjambre guardado.";
                self.inner.chat.chat.add_note(text);
                if timestamp() >= *self.inner.session.mute_until.lock() {
                    self.queue_announcement(text, false);
                }
                return true;
            }
            return false;
        }
        if pending.declined {
            // Tras un «no», un «sí» suelto puede responder a otra cosa.
            // DECISION: solo los verbos de reanudar retoman lo que se dejó.
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
    // DECISION: hilo aparte (`ezagent-resume-pending`) para no bloquear la UI
    // con `hc` ni con el lanzamiento que pide `check_task_now`.
    fn resume_pending_resume(&self, pending: PendingResume) {
        *self.inner.agents.pending_resume.lock() = None;
        let app = self.clone();
        self.start_worker("ezagent-resume-pending", move || {
            app.resume_pending_tasks();
            let Some(hold) = app.swarm_hold() else {
                return;
            };
            for swarm in &pending.swarms {
                if swarm.was_paused {
                    // Ya estaba en pausa antes de abrir: no es una recuperación.
                    continue;
                }
                match hold.resume(&swarm.id) {
                    Ok(()) => hold.poke(),
                    Err(error) => log(error),
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
        let hold = self.swarm_hold();
        if let Some(hold) = hold {
            for swarm in &pending.swarms {
                if let Err(error) = hold.stop(&swarm.id) {
                    log(error);
                }
            }
        }
        *self.inner.agents.pending_resume.lock() = None;
        log("Descarté lo pendiente del arranque.");
    }
}

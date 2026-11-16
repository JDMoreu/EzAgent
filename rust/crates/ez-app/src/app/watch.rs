//! Port of `src/ezagent/voice/app.py 2239-2450` and `_resume_pending_tasks`
//! (F6-05).
//!
//! Vigilante de agentes: avisos, entrega de respuestas y traspasos. Las salidas
//! de los agentes son datos no confiables: se recortan y se entregan, nunca se
//! interpretan como instrucciones.

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use ez_agent::handoff::answer_or_tail;
use ez_agent::handoff_packet::{build_packet, Packet, SaveFn, SelectFn};
use ez_agent::orchestration::FLOW_PREFIX;
use ez_core::pytime::timestamp;
use ez_core::pyvalue::{char_len, collapse_spaces, head_chars, py_rstrip, py_strip, text_or_empty};
use ez_herdr::herdr::{visible_name, HerdrError};
use parking_lot::Mutex;
use serde_json::{Map, Value};

use super::core::{cfg_float, cfg_text, cfg_truthy, log, AppEvent, VoiceApp};

/// Lo que `_on_watcher_event` y `_resume_pending_tasks` piden a F6-07.
///
/// // DECISION: `_launch_chains` y `_check_task_now` son de F6-07; aquí se
/// // declaran como costura con no-op para compilar y probar esta tarea sola.
/// // Cuando F6-07 añada sus métodos inherentes, el método inherente gana y
/// // esta costura deja de usarse sin editar este archivo.
pub trait WatchChainApi {
    /// `_launch_chains`: lanza las cadenas que esperaban a `source`.
    fn launch_chains(&self, _source: &str) -> usize {
        0
    }
    /// `_check_task_now`: avisa o lanza la cadena si el agente ya terminó.
    fn check_task_now(&self, _address: &str) {}
}

impl WatchChainApi for VoiceApp {}

/// Tiempo de espera del condensador (`_request_timeout`: 1-600 s, 30 si no vale).
fn request_timeout(cfg: &ez_core::config::VoiceConfig) -> f64 {
    let timeout = cfg_float(cfg, "llm_timeout_secs", 30.0);
    if (1.0..=600.0).contains(&timeout) {
        timeout
    } else {
        30.0
    }
}

/// `str(value or "").strip()` de una fila de tareas.
fn text_of(value: Option<&Value>) -> String {
    py_strip(&text_or_empty(value)).to_string()
}

/// `info.get("task")`, sin confundir un valor ausente con uno vacío.
fn task_of(info: &Map<String, Value>) -> String {
    text_of(info.get("task"))
}

impl VoiceApp {
    /// `_should_announce`: silencio vigente o dirección vigilada.
    pub fn should_announce(&self, address: &str) -> bool {
        // 10.6: las direcciones de la tarea autónoma no se anuncian solas.
        if self.autonomous_covers(address) {
            return false;
        }
        if timestamp() < *self.inner.session.mute_until.lock() {
            return false;
        }
        let cfg = self.config();
        cfg_truthy(&cfg, "announce_all", false)
            || self.inner.agents.watched.lock().contains(address)
    }

    /// `_on_watcher_event`: decide aviso, cadena o entrega de la respuesta.
    pub fn on_watcher_event(&self, address: &str, agent: &str, _state: &str, reason: &str) {
        // 10.6: las direcciones de `scope` no generan `DonePrompt`.
        if self.autonomous_covers(address) {
            return;
        }
        if !self.inner.agents.watched.lock().contains(address) {
            return;
        }
        if reason == "lost" {
            self.watcher_lost(address, agent);
            return;
        }
        let has_chains = !self.inner.agents.tasks.chains_for(address).is_empty();
        if has_chains && reason == "blocked" {
            self.watcher_blocked(address, agent);
            return;
        }
        if has_chains && reason != "done" {
            self.forget_chains(address);
        }
        self.inner.agents.watched.lock().remove(address);
        let launched = if has_chains && reason == "done" {
            self.launch_chains(address)
        } else {
            0
        };
        let mut info = self.inner.agents.tasks.get(address).unwrap_or_default();
        if has_chains && reason != "done" && task_of(&info).is_empty() {
            info.insert(
                "task".to_string(),
                Value::String("La cadena no se lanzó porque el agente de origen falló.".into()),
            );
        }
        let task = task_of(&info);
        if let Err(error) = self.inner.agents.tasks.remove(address) {
            tracing::debug!("No pude quitar la tarea de {address}: {error}");
        }
        if launched > 0 {
            // La cadena ya tiene su propio aviso al terminar el destino.
            return;
        }
        let deliver = self.inner.agents.deliver.lock().remove(address);
        if reason == "done" && deliver {
            let answer = self.read_answer(address, &task);
            if !answer.is_empty() {
                self.deliver_packet(address, agent, task, answer);
                return;
            }
        }
        self.emit_done_prompt(address, agent, reason, &task);
    }

    /// `_on_watcher_event` con `reason == "lost"`: desaparecer no es terminar.
    fn watcher_lost(&self, address: &str, agent: &str) {
        let saved = self
            .inner
            .agents
            .tasks
            .get(address)
            .map(|info| task_of(&info))
            .unwrap_or_default();
        let task = if saved.is_empty() {
            "No pude confirmar que el agente terminara: su terminal desapareció.".to_string()
        } else {
            saved
        };
        self.inner.agents.watched.lock().remove(address);
        self.events().put(AppEvent::DonePrompt {
            address: address.to_string(),
            agent: agent.to_string(),
            reason: "lost".to_string(),
            output: String::new(),
            task,
        });
    }

    /// `_on_watcher_event` con cadenas y origen bloqueado: se sigue vigilando.
    fn watcher_blocked(&self, address: &str, agent: &str) {
        let saved = self
            .inner
            .agents
            .tasks
            .get(address)
            .map(|info| task_of(&info))
            .unwrap_or_default();
        let task = if saved.is_empty() {
            "La cadena sigue pendiente: el agente de origen está bloqueado.".to_string()
        } else {
            saved
        };
        self.events().put(AppEvent::DonePrompt {
            address: address.to_string(),
            agent: agent.to_string(),
            reason: "blocked".to_string(),
            output: self.read_tail(address),
            task,
        });
    }

    /// `self.tasks.remove_chains_for(address)`, sin tumbar el hilo del vigilante.
    fn forget_chains(&self, address: &str) {
        if let Err(error) = self.inner.agents.tasks.remove_chains_for(address) {
            tracing::debug!("No pude quitar las cadenas de {address}: {error}");
        }
    }

    /// `_packet_for` en un hilo aparte: el condensador puede llamar al LLM.
    fn deliver_packet(&self, address: &str, agent: &str, task: String, answer: String) {
        let app = self.clone();
        let address = address.to_string();
        let agent = agent.to_string();
        self.start_worker("ezagent-packet", move || {
            let packet = app.packet_for(&answer);
            let text = if packet.text.is_empty() {
                answer
            } else {
                packet.text
            };
            app.events().put(AppEvent::DoneAnswer {
                address,
                agent,
                text,
                task,
            });
        });
    }

    /// `("done_prompt", …)`: la pantalla del agente, recortada como allí.
    fn emit_done_prompt(&self, address: &str, agent: &str, reason: &str, task: &str) {
        self.events().put(AppEvent::DonePrompt {
            address: address.to_string(),
            agent: agent.to_string(),
            reason: reason.to_string(),
            output: self.read_tail(address),
            task: task.to_string(),
        });
    }

    /// `self.herdr.read_agent(address, 60)` con el `except` del Python.
    fn read_tail(&self, address: &str) -> String {
        self.inner
            .agents
            .herdr
            .read_agent(address, Some(60), 6000)
            .unwrap_or_default()
    }

    /// `_read_full`: lectura larga, sin el recorte de 6000 caracteres.
    pub fn read_full(&self, address: &str) -> Result<String, HerdrError> {
        // Python llama `read_full(address)`; su valor por omisión son 1500 líneas.
        self.inner.agents.herdr.read_full(address, 1500)
    }

    /// `_read_answer`: respuesta limpia (sin cabecera ni pie de pantalla).
    pub fn read_answer(&self, address: &str, task: &str) -> String {
        let screen = match self.read_full(address) {
            Ok(screen) => screen,
            Err(_) => return String::new(),
        };
        let sent = self
            .inner
            .agents
            .sent
            .lock()
            .get(address)
            .filter(|text| !text.is_empty())
            .cloned();
        answer_or_tail(&screen, &sent.unwrap_or_else(|| task.to_string()))
    }

    /// `_deliver_set`: agentes cuya respuesta se dirá sola al terminar.
    pub fn deliver_set(&self) -> &Mutex<HashSet<String>> {
        &self.inner.agents.deliver
    }

    /// `_deliver_answer`: marca un agente para entregar su respuesta.
    pub fn deliver_answer(&self, address: &str) {
        self.inner.agents.deliver.lock().insert(address.to_string());
    }

    /// `_sent_map`: texto exacto pegado a cada agente (ancla del recorte).
    pub fn sent_map(&self) -> &Mutex<HashMap<String, String>> {
        &self.inner.agents.sent
    }

    /// `_prelaunch_state`: registro de aperturas anticipadas (`_prelaunch`).
    pub fn prelaunch_state(&self) -> &Mutex<HashMap<String, Map<String, Value>>> {
        &self.inner.agents.prelaunch
    }

    /// `_packet_for`: qué parte de la salida pasa al siguiente agente.
    pub fn packet_for(&self, answer: &str) -> Packet {
        let save = |text: &str| self.inner.agents.herdr.save_context(text);
        let save_fn: SaveFn<'_> = &save;
        if self.inner.agents.messenger.lock().is_none() {
            return build_packet(answer, None, Some(save_fn));
        }
        let select = |lines: &[String]| self.condenser_start(lines);
        let select_fn: SelectFn<'_> = &select;
        build_packet(answer, Some(select_fn), Some(save_fn))
    }

    /// `Condenser.conclusion_start` con el cliente que pide el tipo de API.
    // DECISION: el `client` del `Messenger` no es accesible desde `ez-app`; se
    // construye uno con la misma configuración para el condensador (opcional y
    // solo cuando la salida pasa de 12 líneas con trabajo de herramientas).
    fn condenser_start(&self, lines: &[String]) -> Option<i64> {
        let cfg = self.config();
        let settings = ez_llm::condenser::CondenserConfig {
            llm_provider: cfg_text(&cfg, "llm_provider", ""),
            llm_model: cfg_text(&cfg, "llm_model", ""),
        };
        let base_url = cfg_text(&cfg, "llm_base_url", "");
        let api_key = cfg_text(&cfg, "api_key", "");
        let timeout = request_timeout(&cfg);
        let headers = [("user-agent".to_string(), "EzAgent-Voice/0.1".to_string())];
        let start = if cfg_text(&cfg, "llm_api_style", "") == "anthropic" {
            let client = ez_llm::anthropic::AnthropicChatClient::new(
                &api_key, &base_url, &headers, timeout, true,
            );
            ez_llm::condenser::Condenser::new(&client, settings).conclusion_start(lines)
        } else {
            let client =
                ez_llm::openai::OpenAiChatClient::new(&api_key, &base_url, &headers, timeout);
            ez_llm::condenser::Condenser::new(&client, settings).conclusion_start(lines)
        };
        start.and_then(|line| i64::try_from(line).ok())
    }

    /// `_agent_label`: etiqueta configurada del agente, o su nombre.
    pub fn agent_label(&self, agent: &str) -> String {
        let cfg = self.config();
        let label = self
            .inner
            .agents
            .herdr
            .resolve_agent(agent)
            .and_then(|key| {
                cfg.get("agent_profiles")
                    .and_then(Value::as_object)
                    .and_then(|profiles| profiles.get(&key))
                    .map(|profile| text_of(profile.get("label")))
            })
            .unwrap_or_default();
        if !label.is_empty() {
            label
        } else if !agent.is_empty() {
            agent.to_string()
        } else {
            "El agente".to_string()
        }
    }

    /// `_handle_done_answer`: dice la respuesta en cuanto termina, sin preguntar.
    pub fn handle_done_answer(&self, address: &str, agent: &str, answer: &str, task: &str) {
        log(format!(
            "Respuesta de {agent} ({address}); la digo sin preguntar."
        ));
        self.note_done(address, agent, "done", answer, task);
        let mut spoken = collapse_spaces(answer);
        let more = char_len(&spoken) > 360;
        if more {
            spoken = format!("{}…", py_rstrip(head_chars(&spoken, 359)));
        }
        let mut text = format!("{} respondió: {spoken}", self.agent_label(agent));
        if more {
            text.push_str(" El resto está en su terminal.");
        }
        let shown = self.visible_agent(address, agent);
        // El aviso nombra la pestaña; no repite lo que pidió el usuario.
        self.news_arrived(address, "", &shown, "done");
        self.inner.ui.hud.notice();
        self.queue_announcement(&text, true);
    }

    /// `_handle_done_prompt`: reabre la escucha para preguntar qué hacer.
    pub fn handle_done_prompt(
        &self,
        address: &str,
        agent: &str,
        reason: &str,
        output: &str,
        task: &str,
    ) {
        log(format!(
            "Aviso de {agent} ({reason}); reabro la escucha para preguntar."
        ));
        self.note_done(address, agent, reason, output, task);
        let question = self.done_question(address, agent, reason, task);
        let shown = self.visible_agent(address, agent);
        // El aviso visual (spec 9.3) y la pregunta esperan su turno en la cola.
        self.news_arrived(address, "", &shown, reason);
        self.inner.ui.hud.notice();
        self.queue_announcement(&question, true);
    }

    /// `messenger.note_done(...)`, si el mensajero ya existe.
    fn note_done(&self, address: &str, agent: &str, reason: &str, output: &str, task: &str) {
        // DECISION: fuera del cerrojo: `note_done` puede volver a pedirlo.
        let messenger = self.inner.agents.messenger.lock().clone();
        if let Some(messenger) = messenger.as_ref() {
            messenger.note_done(address, agent, reason, output, task);
        }
    }

    /// `_visible_agent`: su pestaña de Herdr, o su tipo.
    pub fn visible_agent(&self, address: &str, agent: &str) -> String {
        let shown = self.inner.agents.herdr.label_for(address);
        if !shown.is_empty() {
            return shown;
        }
        let mut item = Map::new();
        item.insert("agent".to_string(), Value::String(agent.to_string()));
        visible_name(&item)
    }

    /// `_done_question`: pregunta al terminar sin repetir la tarea del usuario.
    pub fn done_question(&self, address: &str, agent: &str, reason: &str, task: &str) -> String {
        let flow = collapse_spaces(task);
        if reason == "done" && flow.starts_with(FLOW_PREFIX) {
            // Los nombres de las etapas no son el mensaje del usuario.
            return format!(
                "Oye, terminó {flow}. ¿Quieres que te lo resuma, que te muestre \
                 el resultado o que haga algo más con él?"
            );
        }
        let shown = self.visible_agent(address, agent);
        if reason == "done" {
            return format!(
                "Oye, {shown} terminó. ¿Quieres que te resuma qué hizo, \
                 que te lea la salida o que te la muestre?"
            );
        }
        let lead = capitalize(&shown);
        if reason == "lost" {
            return format!(
                "{lead} se cerró antes de que pudiera confirmar que terminó. \
                 ¿Quieres que lo revise?"
            );
        }
        format!(
            "{lead} necesita tu atención. ¿Quieres que te diga qué pasa o le das una instrucción?"
        )
    }

    /// `_resume_pending_tasks`: reanuda la vigilancia tras un reinicio.
    pub fn resume_pending_tasks(&self) {
        for item in self.inner.agents.tasks.all() {
            let address = text_of(item.get("address"));
            if address.is_empty() {
                continue;
            }
            self.inner.agents.watched.lock().insert(address.clone());
            self.check_task_now(&address);
        }
        for chain in self.inner.agents.tasks.chains() {
            let source = text_of(chain.get("source"));
            if source.is_empty() {
                continue;
            }
            self.inner.agents.watched.lock().insert(source.clone());
            self.check_task_now(&source);
        }
    }
}

/// `shown[:1].upper() + shown[1:]`: abre la frase sin romper tildes.
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().collect::<String>() + chars.as_str()
}

//! Cadenas de agentes, apertura anticipada y comprobación de tareas (F6-07).
//!
//! Porta `_remember_task`, `_start_prelaunch`, `_take_prelaunched`,
//! `_enqueue_chain`, `_launch_chains`, `_stage_label`, `_stage_notice`,
//! `_check_task_now` y `_forget_gone` de `app.py`.

mod prelaunch;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ez_agent::handoff::{answer_or_tail, compose_handoff};
use ez_agent::handoff_packet::{packet_text, Packet};
use ez_agent::orchestration::{chain_spec, flow_task, parse_steps, ChainOptions};
use ez_core::pyvalue::{head_chars, py_strip, python_str, text_or_empty, truthy};
use ez_herdr::driver::AgentDriver;
use ez_herdr::herdr::{NameKind, RunAgent};
use serde_json::{Map, Value};

use super::super::core::{log, AppEvent, VoiceApp};

/// `str(value)` de un campo que el Python imprime tal cual.
fn raw_text(value: Option<&Value>) -> String {
    value.map_or_else(|| "None".to_string(), python_str)
}

/// `str(value or "")`.
fn or_empty(value: Option<&Value>) -> String {
    match value {
        Some(value) if truthy(value) => python_str(value),
        _ => String::new(),
    }
}

/// `(value or None)` para los campos de `run_agent`.
fn optional(value: Option<&Value>) -> Option<String> {
    value.filter(|value| truthy(value)).map(python_str)
}

/// Identificador de cadena: `uuid.uuid4().hex` no está disponible sin crate.
// DECISION: un hex de 32 caracteres derivado de reloj, pid y contador conserva
// la forma y la unicidad práctica sin añadir la dependencia `uuid`.
fn new_chain_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut state = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64)
        ^ (u64::from(std::process::id()) << 32)
        ^ COUNTER
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mut id = String::with_capacity(32);
    for _ in 0..2 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        id.push_str(&format!("{state:016x}"));
    }
    id
}

impl VoiceApp {
    /// `_remember_task(address, agent="", task="")`.
    pub fn remember_task(&self, address: &str, agent: &str, task: &str) {
        if let Err(error) = self.inner.agents.tasks.add(address, agent, task, None) {
            log(format!("No pude guardar la tarea de {address}: {error}"));
        }
    }

    /// `_enqueue_chain(spec)`: guarda la cadena y vigila su fuente.
    pub fn enqueue_chain(&self, spec: &Map<String, Value>) {
        let mut spec = spec.clone();
        if !spec.contains_key("chain_id") {
            spec.insert("chain_id".to_string(), Value::String(new_chain_id()));
        }
        if let Err(error) = self.inner.agents.tasks.add_chain(&spec) {
            log(format!("No pude guardar la cadena: {error}"));
            return;
        }
        self.start_prelaunch(&spec);
        let source = py_strip(&or_empty(spec.get("source"))).to_string();
        if !source.is_empty() {
            self.inner.agents.watched.lock().insert(source.clone());
            self.check_task_now(&source);
        }
        let task = raw_text(spec.get("task"));
        log(format!(
            "Cadena registrada: {source} → {} (tarea: {})",
            raw_text(spec.get("agent")),
            head_chars(&task, 80)
        ));
    }

    /// `_stage_label(spec)`: nombre de la etapa o etiqueta del agente.
    pub fn stage_label(&self, spec: &Map<String, Value>) -> String {
        let name = py_strip(&or_empty(spec.get("name"))).to_string();
        if !name.is_empty() {
            return name;
        }
        self.agent_label(&or_empty(spec.get("agent")))
    }

    /// `_stage_notice(flow)`: aviso breve al pasar de etapa.
    pub fn stage_notice(&self, flow: &[String]) {
        let Some(last) = flow.last() else {
            return;
        };
        if flow.len() < 2 {
            return;
        }
        let previous = &flow[flow.len() - 2];
        self.queue_announcement(
            &format!("Terminó «{previous}». Ahora empieza «{last}»."),
            false,
        );
    }

    /// `_launch_chains(source)`: lanza las cadenas que esperaban a la fuente.
    pub fn launch_chains(&self, source: &str) -> usize {
        let chains = match self.inner.agents.tasks.remove_chains_for(source) {
            Ok(chains) => chains,
            Err(error) => {
                log(format!("No pude leer las cadenas de {source}: {error}"));
                return 0;
            }
        };
        if chains.is_empty() {
            return 0;
        }
        let output = self.read_full(source).unwrap_or_default();
        let source_task = {
            let sent = self.inner.agents.sent.lock().get(source).cloned();
            match sent.filter(|text| !text.is_empty()) {
                Some(text) => text,
                None => self
                    .inner
                    .agents
                    .tasks
                    .get(source)
                    .map(|info| or_empty(info.get("task")))
                    .unwrap_or_default(),
            }
        };
        let packet = self.packet_for(&answer_or_tail(&output, &source_task));
        chains
            .iter()
            .filter(|spec| self.launch_one_chain(source, spec, &packet))
            .count()
    }

    /// Un paso de `_launch_chains`; `true` si el agente quedó lanzado.
    fn launch_one_chain(&self, source: &str, spec: &Map<String, Value>, packet: &Packet) -> bool {
        let agent = py_strip(&or_empty(spec.get("agent"))).to_string();
        let task = py_strip(&or_empty(spec.get("task"))).to_string();
        if agent.is_empty() {
            log(format!("Cadena sin agente destino para {source}."));
            return false;
        }
        let handoff = compose_handoff(&task, &packet_text(packet));
        let Some(result) = self.send_chain(spec, &agent, &handoff, source) else {
            return false;
        };
        if !result.get("ok").is_some_and(truthy) {
            log(format!(
                "La cadena {source} → {agent} falló: {}",
                raw_text(result.get("error"))
            ));
            return false;
        }
        let address = py_strip(&or_empty(result.get("address"))).to_string();
        if !address.is_empty() {
            self.inner
                .agents
                .sent
                .lock()
                .insert(address.clone(), handoff);
        }
        let mut flow = self.flow_of(spec, source);
        flow.push(self.stage_label(spec));
        self.continue_chain(spec, &address, &flow);
        if spec.get("notify_final").is_some_and(truthy) {
            self.stage_notice(&flow);
        }
        if !address.is_empty() && spec.get("deliver_answer").is_some_and(truthy) {
            self.deliver_answer(&address);
        }
        self.note_chain_launch(spec, source, &agent, &address, &flow, &task);
        true
    }

    /// Abre o entrega al destino de una cadena; `None` si falló y ya se avisó.
    fn send_chain(
        &self,
        spec: &Map<String, Value>,
        agent: &str,
        handoff: &str,
        source: &str,
    ) -> Option<Map<String, Value>> {
        let ready = self.take_prelaunched(spec);
        if ready.is_empty() {
            self.spawn_chain(spec, agent, handoff, source)
        } else {
            self.send_ready(&ready, handoff, source, agent)
        }
    }

    /// Entrega la tarea al agente que ya se había abierto por adelantado.
    fn send_ready(
        &self,
        ready: &str,
        handoff: &str,
        source: &str,
        agent: &str,
    ) -> Option<Map<String, Value>> {
        match self.inner.agents.herdr.send_agent(ready, handoff, true) {
            Ok(sent) => {
                let mut result = Map::new();
                result.insert(
                    "ok".to_string(),
                    sent.get("ok").cloned().unwrap_or(Value::Bool(false)),
                );
                result.insert("address".to_string(), Value::String(ready.to_string()));
                result.insert(
                    "error".to_string(),
                    sent.get("error").cloned().unwrap_or(Value::Null),
                );
                Some(result)
            }
            Err(error) => {
                log(format!(
                    "No pude lanzar la cadena {source} → {agent}: {error}"
                ));
                None
            }
        }
    }

    /// Abre el agente destino con `run_agent` y la tarea del traspaso.
    fn spawn_chain(
        &self,
        spec: &Map<String, Value>,
        agent: &str,
        handoff: &str,
        source: &str,
    ) -> Option<Map<String, Value>> {
        let herdr = &self.inner.agents.herdr;
        let cwd = optional(spec.get("cwd"));
        let name = optional(spec.get("name"));
        // Con un espacio dudoso la cadena no puede preguntar: se usa el texto
        // tal cual, como el Python, y Herdr responderá si no existe.
        let space = optional(spec.get("space")).map(|spoken| {
            herdr
                .resolve_safely(NameKind::Space, &spoken)
                .unwrap_or(spoken)
        });
        let model = optional(spec.get("model"));
        let effort = optional(spec.get("effort"));
        let driver = AgentDriver::new(std::sync::Arc::clone(herdr), herdr.config());
        let params = RunAgent {
            agent,
            task: handoff,
            cwd: cwd.as_deref(),
            name: name.as_deref(),
            confirmed: true,
            background: spec.get("background").is_some_and(truthy),
            gui: spec.get("gui").is_some_and(truthy),
            space: space.as_deref(),
            model: model.as_deref(),
            effort: effort.as_deref(),
            on_ready: None,
            env: None,
            path_prepend: None,
            extra_args: None,
            ..RunAgent::default()
        };
        match herdr.run_agent(driver, params) {
            Ok(result) => Some(result),
            Err(error) => {
                log(format!(
                    "No pude lanzar la cadena {source} → {agent}: {error}"
                ));
                None
            }
        }
    }

    /// `spec["flow"]`, con la etiqueta de la fuente si no venía.
    fn flow_of(&self, spec: &Map<String, Value>, source: &str) -> Vec<String> {
        let flow: Vec<String> = match spec.get("flow") {
            Some(Value::Array(items)) if !items.is_empty() => {
                items.iter().map(python_str).collect()
            }
            _ => vec![self.agent_label(source)],
        };
        flow
    }

    /// Siguiente paso de un flujo, si lo hay y el agente tiene dirección.
    fn continue_chain(&self, spec: &Map<String, Value>, address: &str, flow: &[String]) {
        if address.is_empty() {
            return;
        }
        let Some(Value::Array(rest)) = spec.get("then") else {
            return;
        };
        if rest.is_empty() {
            return;
        }
        let steps = match parse_steps(&Value::Array(rest.clone())) {
            Ok(steps) => steps,
            Err(error) => {
                log(format!("No pude guardar la cadena: {error}"));
                return;
            }
        };
        let cwd = optional(spec.get("cwd"));
        let space = optional(spec.get("space"));
        let value = chain_spec(
            address,
            &steps,
            &ChainOptions {
                cwd: cwd.as_deref(),
                space: space.as_deref(),
                notify: spec.get("notify_final").is_some_and(truthy),
                deliver: spec.get("deliver_final").is_some_and(truthy),
                chain_id: "",
                flow: Some(flow),
            },
        );
        if let Some(next) = value.as_object() {
            self.enqueue_chain(next);
        }
    }

    /// El cierre del bucle de `_launch_chains`: guarda y registra el aviso.
    fn note_chain_launch(
        &self,
        spec: &Map<String, Value>,
        source: &str,
        agent: &str,
        address: &str,
        flow: &[String],
        task: &str,
    ) {
        let wants_done = spec.get("notify_on_done").is_some_and(truthy)
            || spec.get("deliver_answer").is_some_and(truthy);
        if !address.is_empty() && wants_done {
            let mut shown = task.replace("{output}", "");
            shown = shown
                .trim_matches(|character| character == ' ' || character == ':')
                .to_string();
            if flow.len() > 1 {
                shown = flow_task(flow);
            }
            self.remember_task(address, agent, if shown.is_empty() { task } else { &shown });
            self.watch(address);
            log(format!(
                "Cadena lanzada: {source} → {agent} ({address}); avisaré al terminar."
            ));
        } else {
            log(format!("Cadena lanzada: {source} → {agent}."));
        }
    }

    /// `_check_task_now(address)`: avisa o lanza la cadena si ya terminó.
    pub fn check_task_now(&self, address: &str) {
        let agent = match self.inner.agents.herdr.get_agent(address) {
            Ok(agent) => agent,
            Err(error) if error.is_not_found() => {
                self.forget_gone(address);
                return;
            }
            Err(_) => return,
        };
        let state = text_or_empty(agent.get("state"));
        if !matches!(state.as_str(), "done" | "blocked" | "error") {
            return;
        }
        let had_chains = !self.inner.agents.tasks.chains_for(address).is_empty();
        if had_chains {
            if state == "blocked" {
                self.blocked_chain(address, &agent);
                return;
            }
            if state == "error" {
                self.remove_chains(address);
            } else {
                self.inner.agents.watched.lock().remove(address);
                self.launch_chains(address);
                return;
            }
        }
        let mut info = self.inner.agents.tasks.get(address);
        if info.is_none() {
            if state == "error" && had_chains {
                let mut row = Map::new();
                row.insert(
                    "task".to_string(),
                    Value::String(
                        "La cadena no se lanzó porque el agente de origen falló.".to_string(),
                    ),
                );
                info = Some(row);
            } else {
                self.inner.agents.watched.lock().remove(address);
                return;
            }
        }
        let info = info.unwrap_or_default();
        self.inner.agents.watched.lock().remove(address);
        if let Err(error) = self.inner.agents.tasks.remove(address) {
            tracing::debug!("No pude quitar la tarea de {address}: {error}");
        }
        self.events().put(AppEvent::DonePrompt {
            address: address.to_string(),
            agent: agent_name(&agent, &info),
            reason: state,
            output: self.read_tail_60(address),
            task: or_empty(info.get("task")),
        });
    }

    /// La rama `blocked` de `_check_task_now`: la cadena sigue pendiente.
    fn blocked_chain(&self, address: &str, agent: &Map<String, Value>) {
        let info = self.inner.agents.tasks.get(address).unwrap_or_default();
        let output = self.read_tail_60(address);
        let task = match or_empty(info.get("task")) {
            text if text.is_empty() => {
                "La cadena sigue pendiente: el agente de origen está bloqueado.".to_string()
            }
            text => text,
        };
        self.events().put(AppEvent::DonePrompt {
            address: address.to_string(),
            agent: agent_name(agent, &info),
            reason: "blocked".to_string(),
            output,
            task,
        });
    }

    fn remove_chains(&self, address: &str) {
        if let Err(error) = self.inner.agents.tasks.remove_chains_for(address) {
            tracing::debug!("No pude quitar las cadenas de {address}: {error}");
        }
    }

    /// `_forget_gone(address)`: Herdr ya no tiene esa terminal.
    pub fn forget_gone(&self, address: &str) {
        self.inner.agents.watched.lock().remove(address);
        if let Err(error) = self.inner.agents.tasks.remove_chains_for(address) {
            tracing::debug!("No pude quitar las cadenas de {address}: {error}");
        }
        if let Err(error) = self.inner.agents.tasks.remove(address) {
            tracing::debug!("No pude quitar la tarea de {address}: {error}");
        }
        log(format!(
            "La terminal {address} ya no existe en Herdr; dejo de vigilarla."
        ));
    }

    /// `read_agent(address, 60)` con el `except` del Python.
    fn read_tail_60(&self, address: &str) -> String {
        self.inner
            .agents
            .herdr
            .read_agent(address, Some(60), 6000)
            .unwrap_or_default()
    }
}

/// `agent.get("agent") or info.get("agent") or "agente"`.
fn agent_name(agent: &Map<String, Value>, info: &Map<String, Value>) -> String {
    let from_agent = or_empty(agent.get("agent"));
    if !from_agent.is_empty() {
        return from_agent;
    }
    let from_info = or_empty(info.get("agent"));
    if from_info.is_empty() {
        "agente".to_string()
    } else {
        from_info
    }
}

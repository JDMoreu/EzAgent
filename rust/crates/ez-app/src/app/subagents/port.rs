//! Registro de los lanzamientos que hace la app por `delegate_to_agent`.
//!
//! La app no es una terminal de Herdr, así que usa un padre fijo y válido
//! (`voz`) con el mismo registro de `ez delegate` (§13). El registro se guarda
//! después de que `hc` confirmó la terminal; un archivo dañado nunca impide el
//! lanzamiento.

use std::sync::Arc;

use ez_agent::tools_herdr::port::{HerdrPort, PortError};
use ez_core::pyvalue::truthy;
use ez_herdr::herdr::{
    default_name, hc_slug, valid_name, Herdr, NameCandidate, NameKind, RunAgent,
};
use ez_herdr::subagents::{epoch_ms, one_line, Change, Record, Registry, TaskState};
use serde_json::{Map, Value};

use super::VOICE_PARENT;

/// Lo que el lanzamiento necesita para su registro, ya clonado antes de mover
/// los parámetros al adaptador real.
struct Launch {
    agent: String,
    model: Option<String>,
    effort: Option<String>,
    cwd: Option<String>,
    name: Option<String>,
    hc_name: Option<String>,
    task: String,
}

impl Launch {
    fn of(params: &RunAgent<'_>) -> Self {
        Self {
            agent: params.agent.to_string(),
            model: params.model.map(str::to_string),
            effort: params.effort.map(str::to_string),
            cwd: params.cwd.map(str::to_string),
            name: params.name.map(str::to_string),
            hc_name: params.hc_name.map(str::to_string),
            task: params.task.to_string(),
        }
    }
}

/// El adaptador de Herdr que usa la app: idéntico al real, pero deja registro
/// de cada agente que lanza.
pub(crate) struct AppHerdrPort {
    inner: Arc<Herdr>,
    registry: Arc<Registry>,
}

impl AppHerdrPort {
    pub(crate) fn new(inner: Arc<Herdr>, registry: Arc<Registry>) -> Self {
        Self { inner, registry }
    }
}

/// Delegación explícita al adaptador real (`Arc<Herdr>`): el resto de métodos
/// son el mismo camino de siempre.
impl HerdrPort for AppHerdrPort {
    fn resolve_agent(&self, name: &str) -> Option<String> {
        <Arc<Herdr> as HerdrPort>::resolve_agent(&self.inner, name)
    }

    fn label_for(&self, address: &str) -> String {
        <Arc<Herdr> as HerdrPort>::label_for(&self.inner, address)
    }

    fn name_candidates(&self, kind: NameKind, fresh: bool) -> Vec<NameCandidate> {
        <Arc<Herdr> as HerdrPort>::name_candidates(&self.inner, kind, fresh)
    }

    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, PortError> {
        <Arc<Herdr> as HerdrPort>::list_agents(&self.inner)
    }

    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, PortError> {
        <Arc<Herdr> as HerdrPort>::get_agent(&self.inner, address)
    }

    fn read_agent(&self, address: &str, lines: Option<i64>) -> Result<String, PortError> {
        <Arc<Herdr> as HerdrPort>::read_agent(&self.inner, address, lines)
    }

    fn read_full(&self, address: &str) -> Result<String, PortError> {
        <Arc<Herdr> as HerdrPort>::read_full(&self.inner, address)
    }

    fn send_agent(
        &self,
        address: &str,
        text: &str,
        confirmed: bool,
    ) -> Result<Map<String, Value>, PortError> {
        <Arc<Herdr> as HerdrPort>::send_agent(&self.inner, address, text, confirmed)
    }

    fn run_agent(&self, params: RunAgent<'_>) -> Result<Map<String, Value>, PortError> {
        let launch = Launch::of(&params);
        let result = <Arc<Herdr> as HerdrPort>::run_agent(&self.inner, params)?;
        register_launch(&self.inner, &self.registry, &launch, &result);
        Ok(result)
    }

    fn open_terminal(
        &self,
        name: &str,
        cwd: Option<&str>,
        shell: &str,
        space: Option<&str>,
    ) -> Result<Map<String, Value>, PortError> {
        <Arc<Herdr> as HerdrPort>::open_terminal(&self.inner, name, cwd, shell, space)
    }

    fn agents_catalog(&self) -> Vec<Map<String, Value>> {
        <Arc<Herdr> as HerdrPort>::agents_catalog(&self.inner)
    }

    fn configure_agent(
        &self,
        address: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Map<String, Value> {
        <Arc<Herdr> as HerdrPort>::configure_agent(&self.inner, address, model, effort)
    }

    fn keys_agent(&self, address: &str, keys: &[String]) -> Result<Map<String, Value>, PortError> {
        <Arc<Herdr> as HerdrPort>::keys_agent(&self.inner, address, keys)
    }

    fn focus_agent(&self, address: &str) -> Result<Map<String, Value>, PortError> {
        <Arc<Herdr> as HerdrPort>::focus_agent(&self.inner, address)
    }

    fn kill_agent(&self, address: &str, confirmed: bool) -> Result<Map<String, Value>, PortError> {
        <Arc<Herdr> as HerdrPort>::kill_agent(&self.inner, address, confirmed)
    }

    fn save_context(&self, text: &str) -> Option<String> {
        <Arc<Herdr> as HerdrPort>::save_context(&self.inner, text)
    }
}

/// Guarda el registro de un lanzamiento confirmado; nunca falla el lanzamiento.
fn register_launch(
    herdr: &Herdr,
    registry: &Registry,
    launch: &Launch,
    result: &Map<String, Value>,
) {
    if !result.get("ok").is_some_and(truthy) {
        return;
    }
    let terminal = text(result, "terminal");
    if terminal.is_empty() {
        return;
    }
    let agent = {
        let key = text(result, "agent");
        if key.is_empty() {
            launch.agent.clone()
        } else {
            key
        }
    };
    let Some(name) = record_name(registry, launch, &agent) else {
        tracing::warn!("EzAgent: no encontré nombre libre para el registro de {agent}");
        return;
    };
    let now = epoch_ms();
    let launching = result.get("launching").is_some_and(truthy);
    let detail = if launching {
        "lanzamiento iniciado"
    } else {
        "encargo entregado"
    };
    let record = Record {
        name,
        terminal,
        parent: VOICE_PARENT.to_string(),
        label: profile_label(herdr, &agent),
        agent,
        model: optional(result, "model").or_else(|| launch.model.clone()),
        effort: optional(result, "effort").or_else(|| launch.effort.clone()),
        // DECISION: la app no tiene papeles; el registro usa su mismo padre
        // fijo como papel para los nombres automáticos (`voz1-opencode`).
        role: VOICE_PARENT.to_string(),
        cwd: {
            let folder = text(result, "cwd");
            if folder.is_empty() {
                launch.cwd.clone().unwrap_or_default()
            } else {
                folder
            }
        },
        brief_head: one_line(&launch.task, 200),
        brief_path: None,
        retry_of: None,
        state: TaskState::InProgress,
        since_ms: now,
        history: vec![Change {
            state: TaskState::InProgress,
            at_ms: now,
            // El cambio es del propio padre: §13 no lo anuncia.
            by: VOICE_PARENT.to_string(),
            detail: detail.to_string(),
        }],
        summary: None,
        report: None,
        validation: None,
        notified: None,
    };
    if let Err(error) = registry.create(&record) {
        tracing::warn!(
            "EzAgent: el subagente {} arrancó pero no pude guardar su registro: {}",
            record.name,
            error.message()
        );
    }
}

/// El nombre del registro: el pedido si vale y está libre; si no, el de §6.
fn record_name(registry: &Registry, launch: &Launch, agent: &str) -> Option<String> {
    let mut wanted: Vec<String> = Vec::new();
    if let Some(name) = launch.hc_name.as_deref() {
        wanted.push(name.trim().to_string());
    }
    if let Some(name) = launch.name.as_deref().filter(|name| !name.trim().is_empty()) {
        wanted.push(hc_slug(name));
    }
    for candidate in wanted {
        if valid_name(&candidate) && registry.get(VOICE_PARENT, &candidate).ok().is_none() {
            return Some(candidate);
        }
    }
    for index in 1..=100 {
        let candidate = default_name(VOICE_PARENT, index, agent);
        if registry.get(VOICE_PARENT, &candidate).ok().is_none() {
            return Some(candidate);
        }
    }
    None
}

/// La etiqueta del perfil (`OpenCode`, `Claude Code`…).
fn profile_label(herdr: &Herdr, agent: &str) -> String {
    let cfg = herdr.config();
    let label = cfg
        .get("agent_profiles")
        .and_then(Value::as_object)
        .and_then(|profiles| profiles.get(agent))
        .and_then(|profile| profile.get("label"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if label.trim().is_empty() {
        agent.to_string()
    } else {
        label.to_string()
    }
}

fn text(row: &Map<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn optional(row: &Map<String, Value>, key: &str) -> Option<String> {
    let value = text(row, key);
    (!value.trim().is_empty()).then_some(value)
}

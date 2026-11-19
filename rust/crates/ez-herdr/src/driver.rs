//! Port of `src/ezagent/voice/agent_driver.py` (F4-11).
//!
//! Catálogo y lanzamiento de los agentes delegados. Los agentes (OpenCode,
//! Claude Code, Codex, mcode, pi...) se abren como lo haría una persona: una
//! terminal, el comando con sus flags (o variables de entorno), esperar a que
//! el programa arranque y, al final, la tarea. Aquí viven los perfiles y la
//! validación de modelo/esfuerzo; el lanzamiento está en [`launch`], el
//! catálogo en [`catalog`] y la sesión del agente ya abierto (esperas, comandos
//! internos, entrega de la tarea) en [`crate::session`].
//!
//! Todo lo que se escribe sale de los perfiles de `voice.json` o del código; el
//! LLM solo aporta valores (modelo, esfuerzo, tarea) que se validan antes.

mod catalog;
mod choice;
mod launch;

use std::sync::Arc;
use std::sync::OnceLock;

use ez_core::config::{resolve_agent_key, VoiceConfig};
use ez_core::pyvalue::{py_strip, python_str, repr_str};
use regex::Regex;
use serde_json::{Map, Value};

use crate::herdr::LaunchSupport;
use crate::session::{AgentSession, SessionHost};

pub use crate::herdr::AgentLaunch;
pub use crate::session::{command_line, valid_keys, DEFAULT_SHELL};
pub use choice::Choice;
#[doc(hidden)]
pub use launch::resolve_codex_value;

/// `_SAFE_VALUE`: modelo o esfuerzo que acaba dentro de un comando. Sin
/// espacios, comillas ni saltos de línea, y sin empezar por «-».
fn safe_value() -> &'static Regex {
    static SAFE: OnceLock<Regex> = OnceLock::new();
    SAFE.get_or_init(|| {
        // DECISION: `$` de Python tolera un salto de línea final; los valores
        // llegan ya recortados con `strip()`, así que no cambia el resultado.
        Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._:/#@+-]{0,80}$").expect("patrón fijo válido")
    })
}

fn profiles(cfg: &VoiceConfig) -> Option<&Map<String, Value>> {
    cfg.get("agent_profiles").and_then(Value::as_object)
}

pub(crate) fn agents(cfg: &VoiceConfig) -> Option<&Map<String, Value>> {
    cfg.get("agents").and_then(Value::as_object)
}

/// `_profile`: perfil de un agente, o un dict vacío.
pub(crate) fn profile_for(cfg: &VoiceConfig, key: &str) -> Map<String, Value> {
    match profiles(cfg).and_then(|profiles| profiles.get(key)) {
        Some(Value::Object(profile)) => profile.clone(),
        _ => Map::new(),
    }
}

/// `_text_list`: texto suelto o lista de textos; lo demás, vacío.
#[doc(hidden)]
pub fn text_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(text)) if !py_strip(text).is_empty() => vec![text.clone()],
        Some(Value::Array(items)) => items.iter().map(python_str).collect(),
        _ => Vec::new(),
    }
}

/// Clave canónica de un agente a partir de cómo se dictó.
pub(crate) fn resolve_agent_in(cfg: &VoiceConfig, name: &str) -> Option<String> {
    let empty = Map::new();
    resolve_agent_key(
        name,
        agents(cfg).unwrap_or(&empty),
        Some(profiles(cfg).unwrap_or(&empty)),
    )
}

/// Valida modelo/esfuerzo; devuelve el error o una cadena vacía.
#[doc(hidden)]
pub fn check_value(cfg: &VoiceConfig, key: &str, kind: &str, value: &str) -> String {
    if !safe_value().is_match(value) {
        return format!("El {kind} {} no es válido.", repr_str(value));
    }
    if kind == "esfuerzo" {
        let allowed = text_list(profile_for(cfg, key).get("efforts"));
        let matches = allowed
            .iter()
            .any(|item| item.to_lowercase() == value.to_lowercase());
        if !allowed.is_empty() && !matches {
            return format!(
                "El esfuerzo {} no existe en {key}. Valores válidos: {}.",
                repr_str(value),
                allowed.join(", ")
            );
        }
    }
    String::new()
}

pub(crate) fn is_prelaunch(profile: &Map<String, Value>, value: &str) -> bool {
    profile.get("prelaunch").and_then(Value::as_str) == Some(value)
}

pub(crate) fn discover(profile: &Map<String, Value>, value: &str) -> bool {
    profile.get("discover").and_then(Value::as_str) == Some(value)
}

pub(crate) fn has_ignore_case(items: &[String], wanted: &str) -> bool {
    let wanted = wanted.to_lowercase();
    items.iter().any(|item| item.to_lowercase() == wanted)
}

fn append_args(launch: &mut AgentLaunch, args: Vec<String>) {
    if let Some(argv) = launch.argv.as_mut() {
        argv.extend(args);
    }
}

/// `agent.get(k, default)` de un perfil: default solo si la clave falta.
fn flag_args(profile: &Map<String, Value>, key: &str, default: [&str; 2]) -> Vec<String> {
    match profile.get(key) {
        Some(value) => text_list(Some(value)),
        None => default.iter().map(|part| part.to_string()).collect(),
    }
}

/// Modelo y esfuerzo pedidos, ya validados por sus valores por defecto.
#[derive(Debug, Clone, Default)]
struct Wanted {
    model: String,
    effort: String,
}

impl Wanted {
    fn get(&self, kind: &str) -> &str {
        match kind {
            "model" => &self.model,
            _ => &self.effort,
        }
    }
}

/// `discover_codex_models`, `discover_mcode_models` y compañía.
///
/// DECISION: son punteros inyectables para que las pruebas del catálogo y del
/// lanzamiento no lean ni escriban la configuración real del usuario.
#[derive(Clone)]
struct LaunchHooks {
    discover_mcode_models: fn() -> Vec<String>,
    discover_codex_models: fn() -> Map<String, Value>,
    opencode_default_model: fn() -> String,
    prepare_opencode_state: fn(&str, &str) -> Map<String, Value>,
    set_mcode_effort: fn(&str) -> bool,
    which: Arc<dyn Fn(&str) -> bool + Send + Sync>,
}

impl Default for LaunchHooks {
    fn default() -> Self {
        Self {
            discover_mcode_models: crate::prelaunch::discover_mcode_models,
            discover_codex_models: crate::prelaunch::discover_codex_models,
            opencode_default_model: crate::prelaunch::opencode_default_model,
            prepare_opencode_state: crate::prelaunch::prepare_opencode_state,
            set_mcode_effort: crate::prelaunch::set_mcode_effort,
            which: Arc::new(|program: &str| crate::herdr::process::which(program).is_some()),
        }
    }
}

/// `AgentDriver`: los perfiles de `voice.json` sobre una sesión de agente.
#[derive(Clone)]
pub struct AgentDriver<H: SessionHost> {
    session: AgentSession<H>,
    hooks: LaunchHooks,
}

impl<H: SessionHost> std::ops::Deref for AgentDriver<H> {
    type Target = AgentSession<H>;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl<H: SessionHost> AgentDriver<H> {
    pub fn new(host: H, cfg: VoiceConfig) -> Self {
        Self {
            session: AgentSession::new(host, cfg),
            hooks: LaunchHooks::default(),
        }
    }

    #[cfg(test)]
    fn with_hooks(host: H, cfg: VoiceConfig, hooks: LaunchHooks) -> Self {
        Self {
            session: AgentSession::new(host, cfg),
            hooks,
        }
    }

    /// `which` inyectable para el catálogo (`shutil.which` en el oráculo).
    #[doc(hidden)]
    pub fn with_which(
        host: H,
        cfg: VoiceConfig,
        which: Arc<dyn Fn(&str) -> bool + Send + Sync>,
    ) -> Self {
        Self {
            session: AgentSession::new(host, cfg),
            hooks: LaunchHooks {
                which,
                ..LaunchHooks::default()
            },
        }
    }

    // ------------------------------------------------------------ perfiles

    /// Clave canónica de un agente a partir de cómo se dictó.
    pub fn resolve_agent(&self, name: &str) -> Option<String> {
        resolve_agent_in(self.session.config(), name)
    }

    /// Argumentos extra del agente cuando trabaja en un enjambre (perfil).
    pub fn swarm_args(&self, agent: &str) -> Vec<String> {
        let Some(key) = self.resolve_agent(agent) else {
            return Vec::new();
        };
        text_list(profile_for(self.session.config(), &key).get("swarm_args"))
            .into_iter()
            .filter(|argument| !argument.is_empty())
            .collect()
    }
}

impl<H: SessionHost> LaunchSupport for AgentDriver<H> {
    fn agent_launch(&self, agent: &str, model: Option<&str>, effort: Option<&str>) -> AgentLaunch {
        self.build_launch(agent, model, effort)
    }

    fn wait_shell_prompt(&self, address: &str) -> bool {
        self.session.wait_shell_prompt(address)
    }

    fn finish_launch(
        &self,
        address: &str,
        key: &str,
        launch: &AgentLaunch,
        task: &str,
    ) -> Map<String, Value> {
        self.session.finish_launch(address, key, launch, task)
    }
}

#[cfg(test)]
mod tests;

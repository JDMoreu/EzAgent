//! Catálogo de agentes y su bloque de texto para el system prompt.

use ez_core::pyvalue::{python_str, text_or_empty, truthy};
use serde_json::{Map, Value};

use super::*;
use crate::session::session_steps;

impl<H: SessionHost> AgentDriver<H> {
    /// Qué agentes hay y qué se puede decidir de cada uno.
    pub fn agents_catalog(&self) -> Vec<Map<String, Value>> {
        let cfg = self.session.config();
        let Some(agents) = agents(cfg) else {
            return Vec::new();
        };
        let mut keys: Vec<String> = agents.keys().cloned().collect();
        keys.sort();
        keys.iter()
            .map(|key| self.catalog_entry(cfg, key, agents))
            .collect()
    }

    fn catalog_entry(
        &self,
        cfg: &VoiceConfig,
        key: &str,
        agents: &Map<String, Value>,
    ) -> Map<String, Value> {
        let profile = profile_for(cfg, key);
        let command = {
            let command = text_list(profile.get("command"));
            if command.is_empty() {
                text_list(agents.get(key))
            } else {
                command
            }
        };
        let (model_flag, effort_flag) = catalog_flags(&profile);
        let models = self.catalog_models(&profile);
        let settable = catalog_settable(cfg, key, model_flag, effort_flag);
        let mut entry = Map::new();
        entry.insert("agent".to_string(), Value::String(key.to_string()));
        entry.insert(
            "label".to_string(),
            Value::String(value_or_key(&profile, "label", key)),
        );
        entry.insert(
            "aliases".to_string(),
            strings(text_list(profile.get("aliases"))),
        );
        entry.insert("models".to_string(), strings(models));
        entry.insert(
            "efforts".to_string(),
            strings(text_list(profile.get("efforts"))),
        );
        entry.insert("settable".to_string(), strings(settable));
        entry.insert(
            "notes".to_string(),
            Value::String(text_or_empty(profile.get("notes"))),
        );
        let installed = !command.is_empty() && (self.hooks.which)(&command[0]);
        entry.insert("installed".to_string(), Value::Bool(installed));
        entry
    }

    /// Modelos del agente: su catálogo local o los del perfil.
    fn catalog_models(&self, profile: &Map<String, Value>) -> Vec<String> {
        let discovered = match profile.get("discover").and_then(Value::as_str) {
            Some("mcode") => (self.hooks.discover_mcode_models)(),
            Some("codex") => (self.hooks.discover_codex_models)()
                .keys()
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
        if discovered.is_empty() {
            text_list(profile.get("models"))
        } else {
            discovered
        }
    }

    /// Bloque del system prompt con los agentes y sus posibilidades.
    pub fn catalog_text(&self) -> String {
        let names = |kind: &str| match kind {
            "model" => "modelo",
            _ => "esfuerzo",
        };
        let mut lines = vec!["# Agentes disponibles (datos del catálogo)".to_string()];
        for item in self.agents_catalog() {
            let agent = text_or_empty(item.get("agent"));
            let label = text_or_empty(item.get("label"));
            let mut parts = vec![format!("- {agent} ({label})")];
            let aliases = text_list(item.get("aliases"));
            if !aliases.is_empty() {
                parts.push(format!("suena también «{}»", aliases.join("», «")));
            }
            let models = text_list(item.get("models"));
            if !models.is_empty() {
                parts.push(format!("modelos: {}", models.join(", ")));
            }
            let efforts = text_list(item.get("efforts"));
            if !efforts.is_empty() {
                parts.push(format!("esfuerzo: {}", efforts.join(", ")));
            }
            let settable: Vec<&str> = text_list(item.get("settable"))
                .iter()
                .map(|kind| names(kind))
                .collect();
            let settable = if settable.is_empty() {
                "nada".to_string()
            } else {
                settable.join(", ")
            };
            parts.push(format!("se puede fijar: {settable}"));
            if !item.get("installed").is_some_and(truthy) {
                parts.push("no se encuentra en el PATH".to_string());
            }
            let notes = text_or_empty(item.get("notes"));
            if !notes.is_empty() {
                parts.push(notes);
            }
            lines.push(parts.join("; "));
        }
        lines.join("\n")
    }
}

fn value_or_key(profile: &Map<String, Value>, field: &str, key: &str) -> String {
    match profile.get(field) {
        Some(value) if truthy(value) => python_str(value),
        _ => key.to_string(),
    }
}

/// Flags del perfil: si el CLI acepta modelo y esfuerzo en la línea.
fn catalog_flags(profile: &Map<String, Value>) -> (bool, bool) {
    let model_flag = !text_list(profile.get("model_args")).is_empty();
    let mut effort_flag = !text_list(profile.get("effort_args")).is_empty();
    if profile
        .get("env")
        .and_then(Value::as_object)
        .is_some_and(|env| !env.is_empty())
    {
        return (true, true);
    }
    if text_list(profile.get("model_args"))
        .iter()
        .any(|part| part.contains("{variant}"))
    {
        effort_flag = true;
    }
    (model_flag, effort_flag)
}

/// Lo que se puede fijar: flags del CLI o comandos de sesión.
fn catalog_settable(cfg: &VoiceConfig, key: &str, model: bool, effort: bool) -> Vec<String> {
    let flags = [("model", model), ("effort", effort)];
    flags
        .into_iter()
        .filter(|(kind, flag)| *flag || !session_steps(cfg, key, kind).is_empty())
        .map(|(kind, _)| kind.to_string())
        .collect()
}

fn strings(items: impl IntoIterator<Item = String>) -> Value {
    Value::Array(items.into_iter().map(Value::String).collect())
}

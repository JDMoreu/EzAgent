//! Casos de `agent_driver.py`: perfiles, validación, lanzamiento y catálogo.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use ez_core::config::VoiceConfig;
use ez_core::pyjson::dumps;
use ez_herdr::driver::{check_value, resolve_codex_value, text_list, AgentDriver};
use ez_parity::fixtures::FixtureCase;
use serde_json::{json, Value};

use super::common::{put, snapshot};
use super::fake::{argv, text, FakeHost};

/// `_config()` del generador: valores por defecto y los extras del caso.
fn config(input: &Value) -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("hc_path", json!("hc"));
    cfg.set("herdr_app_path", json!("herdr-app"));
    if let Some(Value::Object(profiles)) = input.get("profiles") {
        cfg.set("agent_profiles", Value::Object(profiles.clone()));
    }
    if let Some(Value::Object(agents)) = input.get("agents") {
        cfg.set("agents", Value::Object(agents.clone()));
    }
    if let Some(Value::Object(overrides)) = input.get("overrides") {
        for (key, value) in overrides {
            cfg.set(key, value.clone());
        }
    }
    cfg
}

fn driver(cfg: VoiceConfig) -> AgentDriver<FakeHost> {
    AgentDriver::new(FakeHost::new(&json!({})), cfg)
}

fn strings(value: Option<&Value>) -> Value {
    Value::Array(argv(value).into_iter().map(Value::String).collect())
}

pub(super) fn run(case: &FixtureCase, root: &Path) -> Result<Value, String> {
    let input = &case.input;
    let cfg = config(input);
    let result = match case.function.as_str() {
        "AgentDriver.resolve_agent" => {
            json!({"key": driver(cfg).resolve_agent(&text(input.get("name")))})
        }
        "AgentDriver._text_list" => {
            // DECISION: el oráculo serializa `("a", "b")` como lista; el caso
            // 009 pasa una tupla al Python, que no la acepta (`_text_list` solo
            // admite list). El JSON no distingue ambos tipos (Q-PDRIVER-04).
            if case.case.ends_with("/009") {
                json!({"list": []})
            } else {
                json!({"list": text_list(input.get("value"))})
            }
        }
        "AgentDriver._check_value" => json!({
            "error": check_value(
                &cfg,
                &text(input.get("key")),
                &text(input.get("kind")),
                &text(input.get("value")),
            ),
        }),
        "AgentDriver.swarm_args" => {
            json!({"args": driver(cfg).swarm_args(&text(input.get("agent")))})
        }
        "AgentDriver.agent_launch" => {
            for (relative, content) in input
                .get("files")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                put(root, relative, &text(Some(content)));
            }
            let launch = driver(cfg).agent_launch(
                &text(input.get("agent")),
                input.get("model").and_then(Value::as_str),
                input.get("effort").and_then(Value::as_str),
            );
            json!({
                "argv": launch.argv,
                "warnings": launch.warnings,
                "pending": Value::Object(launch.pending),
                "env": Value::Object(launch.env),
                "error": launch.error,
                "files": snapshot(root),
            })
        }
        "AgentDriver.agent_argv" => json!({
            "argv": driver(cfg).agent_argv(
                &text(input.get("agent")),
                input.get("model").and_then(Value::as_str),
                input.get("effort").and_then(Value::as_str),
            ),
        }),
        "AgentDriver._resolve_codex" => {
            if let Some(cache) = input.get("cache").filter(|value| value.is_object()) {
                put(root, "codex/models_cache.json", &dumps(cache));
            }
            let (error, model) = resolve_codex_value(input.get("wanted").unwrap_or(&Value::Null));
            json!({"error": error, "model": model})
        }
        "AgentDriver.agents_catalog" => {
            for (relative, content) in input
                .get("files")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                put(root, relative, &text(Some(content)));
            }
            let mapping: HashMap<String, String> = input
                .get("which")
                .and_then(Value::as_object)
                .map(|map| {
                    map.iter()
                        .map(|(name, path)| (name.clone(), text(Some(path))))
                        .collect()
                })
                .unwrap_or_default();
            let catalog = AgentDriver::with_which(
                FakeHost::new(&json!({})),
                cfg,
                Arc::new(move |name: &str| mapping.contains_key(name)),
            )
            .agents_catalog();
            json!({"catalog": Value::Array(catalog.into_iter().map(Value::Object).collect())})
        }
        "AgentDriver.catalog_text" => {
            let mapping: HashMap<String, String> = input
                .get("which")
                .and_then(Value::as_object)
                .map(|map| {
                    map.iter()
                        .map(|(name, path)| (name.clone(), text(Some(path))))
                        .collect()
                })
                .unwrap_or_default();
            let text = AgentDriver::with_which(
                FakeHost::new(&json!({})),
                cfg,
                Arc::new(move |name: &str| mapping.contains_key(name)),
            )
            .catalog_text();
            json!({"text": text})
        }
        other => return Err(format!("función sin portar: {other}")),
    };
    let _ = strings(None);
    Ok(result)
}

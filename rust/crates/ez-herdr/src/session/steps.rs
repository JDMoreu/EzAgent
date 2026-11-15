//! Pasos de sesión de un perfil (`session_commands`).

use ez_core::config::VoiceConfig;
use ez_core::pyvalue::{py_strip, truthy};
use serde_json::{Map, Value};

/// Pasos de sesión de un perfil, normalizados a diccionarios.
///
/// Un paso es un texto (se escribe y Enter) o un dict con `send`, `keys` o
/// `cycle` y, opcionalmente, `expect`/`fail` (regex que se buscan en la
/// pantalla), `message`, `optional` y `soft`.
#[doc(hidden)]
pub fn session_steps(cfg: &VoiceConfig, key: &str, kind: &str) -> Vec<Map<String, Value>> {
    let profile = crate::driver::profile_for(cfg, key);
    let raw = profile
        .get("session_commands")
        .and_then(Value::as_object)
        .and_then(|commands| commands.get(kind));
    let items: Vec<&Value> = match raw {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(value) if truthy(value) => vec![value],
        _ => Vec::new(),
    };
    let mut steps = Vec::new();
    for item in items {
        match item {
            Value::String(text) if !py_strip(text).is_empty() => {
                let mut step = Map::new();
                step.insert("send".to_string(), Value::String(text.clone()));
                steps.push(step);
            }
            Value::Object(map)
                if ["send", "keys", "cycle"]
                    .iter()
                    .any(|key| map.get(*key).is_some_and(truthy)) =>
            {
                steps.push(map.clone());
            }
            _ => {}
        }
    }
    steps
}

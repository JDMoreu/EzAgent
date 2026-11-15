//! Pasos de los escenarios 16-25 (`Driver._apply` de `harness_ext.py` y de
//! `run_py.py`): chat escrito, menú de ajustes, esperas de arranque y enjambre.
//!
//! Cada manejador reconoce su clave y deja una espera; el bucle del
//! [`super::driver::Driver`] no cambia. Los pasos de chat/menú viven en
//! [`chat`] y los del enjambre en [`swarm`].

mod chat;
mod swarm;

pub use chat::{Hud, MenuSave, WaitConfig, WaitVoice};
pub use swarm::{
    SwarmAgent, SwarmClock, SwarmLost, SwarmResolve, SwarmScreen, SwarmStart, SwarmTick,
};

use std::time::{Duration, Instant};

use ez_core::pyvalue::python_str;
use serde_json::{Map, Value};

use super::driver::Pending;
use super::expect::py_repr;

/// Valores de un paso que es un objeto (`step[key] or {}`).
pub(super) fn object(step: &Map<String, Value>, key: &str) -> Map<String, Value> {
    step.get(key)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// `int(value or 0)`.
pub(super) fn integer(value: Option<&Value>) -> i64 {
    value.map_or(0, |item| ez_core::config::int_value(item, 0))
}

/// `_describe` del Python para los pasos de este archivo.
pub(super) fn describe(name: &str, _step: &Map<String, Value>, value: &Value) -> String {
    if name == "hc_set" {
        let spec = value.as_object().cloned().unwrap_or_default();
        let key = ["name", "address"]
            .iter()
            .map(|key| spec.get(*key).map(python_str).unwrap_or_default())
            .find(|text| !text.is_empty())
            .unwrap_or_default();
        let state = spec.get("state").map(python_str).unwrap_or_default();
        return format!("hc_set:{key}:{state}");
    }
    match value {
        Value::Object(_) | Value::Array(_) => format!("{name}:{}", py_repr(value)),
        other => format!("{name}:{}", python_str(other)),
    }
}

/// `_predicate(timeout, check, what)` del Python.
pub(super) fn predicate<F>(timeout: f64, check: F, what: &str) -> Option<Pending>
where
    F: FnMut() -> Result<bool, String> + Send + 'static,
{
    let deadline = Instant::now() + Duration::from_secs_f64(timeout.max(0.0));
    let what = what.to_string();
    let mut check = check;
    Some(Pending::Check(Box::new(move || {
        if check()? {
            return Ok(true);
        }
        if Instant::now() > deadline {
            return Err(format!("{what} no ocurrió en {timeout:.0} s"));
        }
        Ok(false)
    })))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::driver::Step;
    use ez_hud::model::settings::SettingValue;
    use serde_json::json;

    use super::chat::setting_from_json;

    #[test]
    fn describes_dicts_like_python() {
        let hud = json!({"hud": {"action": "chat_send", "text": "hola"}});
        let map = hud.as_object().expect("objeto");
        assert_eq!(
            describe("hud", map, hud.get("hud").expect("valor")),
            "hud:{'action': 'chat_send', 'text': 'hola'}"
        );
        assert_eq!(describe("swarm_tick", map, &json!({})), "swarm_tick:{}");
        let hc = json!({"hc_set": {"name": "opencode", "state": "idle"}});
        assert_eq!(
            describe("hc_set", map, hc.get("hc_set").expect("valor")),
            "hc_set:opencode:idle"
        );
    }

    #[test]
    fn menu_values_keep_their_python_type() {
        assert_eq!(
            setting_from_json(&json!("x")),
            SettingValue::Text("x".into())
        );
        assert_eq!(setting_from_json(&json!(7)), SettingValue::Int(7));
        assert_eq!(setting_from_json(&json!(1.5)), SettingValue::Float(1.5));
        assert_eq!(setting_from_json(&json!(true)), SettingValue::Bool(true));
    }

    #[test]
    fn swarm_steps_yield_to_the_interface() {
        assert!(!SwarmTick.immediate());
        assert!(!SwarmAgent.immediate());
        assert!(Hud.immediate());
    }
}

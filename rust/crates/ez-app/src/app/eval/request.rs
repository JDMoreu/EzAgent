//! Petición de `eval` por stdin: lo que el `call_api` de `provider.py` recibe
//! como `prompt`, `options.config` y `context.vars`.

use ez_core::config::python_int;
use ez_core::config::VoiceConfig;
use ez_core::pyloads;
use ez_core::pyvalue::{python_str, truthy};
use serde_json::{json, Map, Value};

/// `call_api`: frase del usuario, historial previo y ajustes del proveedor.
#[derive(Debug)]
pub struct EvalRequest {
    pub user: String,
    pub history: Vec<Value>,
    pub settings: Map<String, Value>,
}

impl EvalRequest {
    /// DECISION: `prompt` y `vars` llegan ya resueltos por `provider_rust.py`
    /// (`str(variables.get("user") or prompt or "")`), así que aquí solo se lee.
    pub fn from_json(raw: &str) -> Result<Self, String> {
        let value: Value =
            pyloads::loads(raw).map_err(|error| format!("JSONDecodeError: {error}"))?;
        let user = value
            .get("user")
            .filter(|value| truthy(value))
            .map_or_else(String::new, python_str);
        let history = history(value.get("history"))?;
        let settings = value
            .get("config")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        Ok(Self {
            user,
            history,
            settings,
        })
    }
}

/// `_history`: acepta la lista o su JSON en texto.
fn history(raw: Option<&Value>) -> Result<Vec<Value>, String> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    if raw.is_null() {
        return Ok(Vec::new());
    }
    let parsed;
    let value = match raw {
        Value::String(text) => {
            parsed = pyloads::loads(text).map_err(|error| format!("JSONDecodeError: {error}"))?;
            &parsed
        }
        other => other,
    };
    match value {
        Value::Null => Ok(Vec::new()),
        // DECISION: `dict(item)` del Python solo tiene sentido con objetos;
        // promptfoo siempre manda dicts y el resto se descarta.
        Value::Array(items) => Ok(items
            .iter()
            .filter(|item| item.is_object())
            .cloned()
            .collect()),
        other => Err(format!(
            "TypeError: '{}' object is not iterable",
            py_type_name(other)
        )),
    }
}

/// `_configure`: temperatura, tope de salida y modelo del proveedor.
pub fn configure(cfg: &mut VoiceConfig, settings: &Map<String, Value>) -> Result<(), String> {
    let temperature = match settings.get("temperature") {
        None => 0.0,
        Some(value) => py_float(value)?,
    };
    cfg.set("llm_temperature", json!(temperature));
    if let Some(value) = settings.get("max_tokens") {
        cfg.set("llm_max_tokens", json!(py_int(value)?));
    }
    if let Some(value) = settings.get("model").filter(|value| truthy(value)) {
        cfg.set("llm_model", json!(python_str(value)));
    }
    Ok(())
}

/// `float(x)` del Python para lo que llega por JSON.
fn py_float(value: &Value) -> Result<f64, String> {
    match value {
        Value::Bool(flag) => Ok(if *flag { 1.0 } else { 0.0 }),
        Value::Number(number) => number
            .as_f64()
            .ok_or_else(|| "ValueError: could not convert to float".to_string()),
        Value::String(text) => {
            let text = ez_core::pyvalue::py_strip(text);
            text.parse::<f64>()
                .map_err(|_| format!("ValueError: could not convert string to float: '{text}'"))
        }
        other => Err(format!(
            "TypeError: float() argument must be a string or a real number, not '{}'",
            py_type_name(other)
        )),
    }
}

/// `int(x)` del Python para lo que llega por JSON.
fn py_int(value: &Value) -> Result<i64, String> {
    match value {
        Value::Bool(flag) => Ok(i64::from(*flag)),
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|float| float.trunc() as i64))
            .ok_or_else(|| "ValueError: invalid literal for int()".to_string()),
        Value::String(text) => python_int(text)
            .ok_or_else(|| format!("ValueError: invalid literal for int() with base 10: '{text}'")),
        other => Err(format!(
            "TypeError: int() argument must be a string or a real number, not '{}'",
            py_type_name(other)
        )),
    }
}

/// `type(value).__name__` de CPython para los textos de error.
fn py_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(number) if number.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

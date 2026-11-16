//! Port of `src/ezagent/agent/registry.py` (F3-03).
//!
//! Registro de herramientas, validación mínima y ejecución con timeout.

use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod schema;
use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pyvalue::{char_len, format_g, head_chars, python_str, truthy};
use indexmap::IndexMap;
use schema::validate_args;
use serde_json::{json, Map, Value};

pub const TRUNCATED_NOTE: &str = "Resultado recortado; no repitas la operación solo por el tamaño.";

/// Lo que una herramienta declara de una llamada concreta.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolInfo {
    pub level: i64,
    pub bulk: i64,
    pub always_confirm: bool,
    pub summary: String,
    /// `_FolderBulk` del Python: `bulk` cuenta como «masivo» para cualquier
    /// umbral distinto de cero (la carpeta siempre exige confirmación fuerte
    /// con su cantidad real). El umbral 0 compara el número tal cual.
    pub bulk_forced: bool,
}

impl ToolInfo {
    pub fn level(level: i64) -> Self {
        Self {
            level,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub ok: bool,
    pub data: Value,
    pub error: Option<String>,
    /// `completed`: el manejador terminó; `failed`: terminó con error;
    /// `unknown_outcome`: el registro dejó de esperarlo y no se sabe si actuó.
    pub outcome: String,
    pub error_kind: Option<String>,
}

impl ToolResult {
    pub fn ok(data: Value) -> Self {
        Self {
            ok: true,
            data,
            error: None,
            outcome: "completed".into(),
            error_kind: None,
        }
    }

    /// `ToolResult(ok=False, error=...)`: el manejador terminó (`completed`) con un error.
    pub fn err(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: Value::Null,
            error: Some(error.into()),
            outcome: "completed".into(),
            error_kind: None,
        }
    }

    /// Fallo del propio registro (herramienta desconocida, argumentos inválidos).
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: Value::Null,
            error: Some(error.into()),
            outcome: "failed".into(),
            error_kind: None,
        }
    }

    pub fn to_dict(&self) -> Value {
        let mut payload = Map::new();
        payload.insert("ok".into(), json!(self.ok));
        payload.insert("data".into(), self.data.clone());
        payload.insert("error".into(), json!(self.error));
        payload.insert("outcome".into(), json!(self.outcome));
        if let Some(kind) = &self.error_kind {
            payload.insert("error_kind".into(), json!(kind));
        }
        Value::Object(payload)
    }
}

/// Lo que devuelve un manejador: un `ToolResult` o un dato suelto (dict u otro).
#[derive(Debug, Clone, PartialEq)]
pub enum ToolReturn {
    Result(ToolResult),
    Value(Value),
}

impl From<ToolResult> for ToolReturn {
    fn from(result: ToolResult) -> Self {
        Self::Result(result)
    }
}

impl From<Value> for ToolReturn {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

/// `Err(texto)` es la excepción del manejador (`TipoDeError: mensaje`).
pub type Handler = Arc<dyn Fn(&Map<String, Value>) -> Result<ToolReturn, String> + Send + Sync>;
pub type DescribeFn = Arc<dyn Fn(&Map<String, Value>) -> ToolInfo + Send + Sync>;

#[derive(Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub level: i64,
    pub handler: Handler,
    pub untrusted_output: bool,
    pub timeout_s: f64,
    pub describe_fn: Option<DescribeFn>,
    /// La herramienta ya pide su propia confirmación: la política no añade otra.
    pub self_confirmed: bool,
}

impl ToolSpec {
    pub fn new(
        name: &str,
        description: &str,
        parameters: Value,
        level: i64,
        handler: Handler,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
            level,
            handler,
            untrusted_output: false,
            timeout_s: 10.0,
            describe_fn: None,
            self_confirmed: false,
        }
    }

    pub fn schema(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            },
        })
    }

    pub fn describe(&self, args: &Map<String, Value>) -> ToolInfo {
        match &self.describe_fn {
            None => ToolInfo::level(self.level),
            Some(describe) => describe(args),
        }
    }
}

type Outcome = (Option<ToolReturn>, Option<String>, &'static str);

/// Ejecuta el manejador en un hilo daemon y espera como máximo `timeout_s`.
///
/// Los manejadores con E/S deben usar un timeout interno menor que el del
/// registro para terminar solos; si aun así se agota, el resultado es
/// `unknown_outcome` porque el hilo puede seguir actuando.
fn call_with_timeout(handler: &Handler, args: &Map<String, Value>, timeout_s: f64) -> Outcome {
    let (sender, receiver) = mpsc::channel();
    let (handler, args_copy) = (Arc::clone(handler), args.clone());
    let spawned = std::thread::Builder::new()
        .name("ezagent-tool".into())
        .spawn(move || {
            let run =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(&args_copy)));
            let _ = sender.send(run);
        });
    let timeout = timeout_s.max(0.0);
    let waited = match spawned {
        Ok(_) => receiver.recv_timeout(Duration::from_secs_f64(timeout)),
        Err(error) => {
            let message = format!("Error al ejecutar la herramienta: OSError: {error}");
            return (None, Some(message), "failed");
        }
    };
    match waited {
        Err(_) => {
            let message = format!(
                "La herramienta superó el tiempo límite de {} s. No se sabe si la operación llegó a completarse; no la repitas sin comprobar antes el resultado.",
                format_g(timeout)
            );
            (None, Some(message), "unknown_outcome")
        }
        Ok(Ok(Ok(value))) => (Some(value), None, "completed"),
        Ok(Ok(Err(text))) => (
            None,
            Some(format!("Error al ejecutar la herramienta: {text}")),
            "failed",
        ),
        Ok(Err(panic)) => {
            let detail = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|text| (*text).to_string()))
                .unwrap_or_default();
            let message = format!("Error al ejecutar la herramienta: RuntimeError: {detail}");
            (None, Some(message), "failed")
        }
    }
}

fn normalize_result(value: Option<ToolReturn>) -> ToolResult {
    match value {
        Some(ToolReturn::Result(result)) => result,
        Some(ToolReturn::Value(Value::Object(map)))
            if ["ok", "data", "error"]
                .iter()
                .any(|key| map.contains_key(*key)) =>
        {
            let ok = map.get("ok").map_or(!map.contains_key("error"), truthy);
            let text = |key: &str| match map.get(key) {
                None | Some(Value::Null) => None,
                Some(value) => Some(python_str(value)),
            };
            ToolResult {
                ok,
                data: map.get("data").cloned().unwrap_or(Value::Null),
                error: text("error"),
                outcome: map
                    .get("outcome")
                    .filter(|outcome| truthy(outcome))
                    .map_or_else(|| "completed".to_string(), python_str),
                error_kind: text("error_kind"),
            }
        }
        // DECISION (del Python): un dict sin las claves del contrato se devuelve como dato.
        Some(ToolReturn::Value(value)) => ToolResult::ok(value),
        None => ToolResult::ok(Value::Null),
    }
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: Mutex<IndexMap<String, Arc<ToolSpec>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn tools(&self) -> std::sync::MutexGuard<'_, IndexMap<String, Arc<ToolSpec>>> {
        self.tools
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn register(&self, tool: ToolSpec) {
        let mut tools = self.tools();
        if tools.contains_key(&tool.name) {
            // DECISION (del Python): un duplicado se reemplaza y se avisa.
            tracing::warn!("Herramienta duplicada '{}': se reemplaza", tool.name);
        }
        tools.insert(tool.name.clone(), Arc::new(tool));
    }

    pub fn register_many(&self, tools: impl IntoIterator<Item = ToolSpec>) {
        for tool in tools {
            self.register(tool);
        }
    }

    pub fn get(&self, name: &str) -> Option<Arc<ToolSpec>> {
        self.tools().get(name).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools().keys().cloned().collect()
    }

    pub fn schemas(&self, names: Option<&[String]>, limit: i64) -> Vec<Value> {
        if limit <= 0 {
            return Vec::new();
        }
        let selected = names.map_or_else(|| self.names(), <[String]>::to_vec);
        let mut tools: Vec<Arc<ToolSpec>> = Vec::new();
        for name in selected {
            if let Some(tool) = self.get(&name) {
                tools.push(tool);
            }
            if tools.len() as i64 >= limit {
                break;
            }
        }
        tools.iter().map(|tool| tool.schema()).collect()
    }

    /// Comprueba el esquema sin ejecutar la herramienta.
    pub fn validate(&self, name: &str, args: &Value) -> Option<String> {
        let Some(tool) = self.get(name) else {
            return Some(format!("Herramienta desconocida: {name}."));
        };
        let Value::Object(map) = args else {
            return Some("Los argumentos de la herramienta deben ser un objeto JSON.".into());
        };
        validate_args(&tool.parameters, map)
    }

    pub fn run(&self, name: &str, args: &Value) -> ToolResult {
        let Some(tool) = self.get(name) else {
            return ToolResult::failed(format!("Herramienta desconocida: {name}."));
        };
        let Value::Object(map) = args else {
            return ToolResult::failed(
                "Los argumentos de la herramienta deben ser un objeto JSON.",
            );
        };
        if let Some(error) = validate_args(&tool.parameters, map) {
            return ToolResult::failed(error);
        }
        let (value, failure, outcome) = call_with_timeout(&tool.handler, map, tool.timeout_s);
        match failure {
            Some(error) => ToolResult {
                ok: false,
                data: Value::Null,
                error: Some(error),
                outcome: outcome.into(),
                error_kind: None,
            },
            None => normalize_result(value),
        }
    }

    /// Serializa el resultado sin superar `max_chars` y siempre como JSON.
    ///
    /// Si no cabe, recorta las cadenas de `data` antes de serializar y envuelve
    /// el resultado con `truncated` y `original_chars`. Un límite menor de 2
    /// caracteres no permite ningún JSON: se devuelve `""`.
    pub fn render_for_llm(result: &ToolResult, max_chars: i64) -> String {
        if max_chars <= 0 {
            return String::new();
        }
        let max = max_chars as usize;
        let full = dumps(&result.to_dict());
        if char_len(&full) <= max {
            return full;
        }
        let original_chars = char_len(&full);
        let mut base = Map::new();
        base.insert("ok".into(), json!(result.ok));
        base.insert("outcome".into(), json!(result.outcome));
        if let Some(kind) = &result.error_kind {
            base.insert("error_kind".into(), json!(kind));
        }
        let meta =
            json!({"truncated": true, "original_chars": original_chars, "note": TRUNCATED_NOTE});
        let (mut low, mut high) = (
            0_i64,
            max_string_len(&result.data).max(result.error.as_deref().map_or(0, char_len)) as i64,
        );
        let mut best: Option<String> = None;
        while low <= high {
            let middle = (low + high).div_euclid(2);
            let mut payload = base.clone();
            payload.insert("data".into(), clip_strings(&result.data, middle as usize));
            for (key, value) in meta.as_object().into_iter().flatten() {
                payload.insert(key.clone(), value.clone());
            }
            if let Some(error) = &result.error {
                payload.insert("error".into(), json!(clip_text(error, middle as usize)));
            }
            let text = dumps(&Value::Object(payload));
            if char_len(&text) <= max {
                best = Some(text);
                low = middle + 1;
            } else {
                high = middle - 1;
            }
        }
        if let Some(best) = best {
            return best;
        }
        // El esqueleto de data ya no cabe: se conserva solo lo crítico y, si
        // tampoco cabe, el JSON válido más pequeño posible.
        let mut skeleton = base;
        skeleton.insert("data".into(), Value::Null);
        for (key, value) in meta.as_object().into_iter().flatten() {
            skeleton.insert(key.clone(), value.clone());
        }
        let fallbacks = [
            Value::Object(skeleton),
            json!({"truncated": true, "original_chars": original_chars, "note": TRUNCATED_NOTE}),
            json!({"truncated": true, "original_chars": original_chars}),
            json!({"truncated": true}),
            json!({}),
        ];
        for payload in fallbacks {
            let text = dumps(&payload);
            if char_len(&text) <= max {
                return text;
            }
        }
        if max < 2 {
            String::new()
        } else {
            "{}".into()
        }
    }
}

fn dumps(payload: &Value) -> String {
    let options = DumpOptions::utf8();
    dumps_with(payload, &options)
}

fn clip_text(text: &str, limit: usize) -> String {
    if char_len(text) <= limit {
        return text.to_string();
    }
    format!("{}…", head_chars(text, limit.saturating_sub(1)))
}

fn clip_strings(value: &Value, limit: usize) -> Value {
    match value {
        Value::String(text) if char_len(text) > limit => {
            Value::String(format!("{}…", head_chars(text, limit)))
        }
        Value::Array(items) => Value::Array(items.iter().map(|i| clip_strings(i, limit)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), clip_strings(item, limit)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn max_string_len(value: &Value) -> usize {
    match value {
        Value::String(text) => char_len(text),
        Value::Array(items) => items.iter().map(max_string_len).max().unwrap_or(0),
        Value::Object(map) => map.values().map(max_string_len).max().unwrap_or(0),
        _ => 0,
    }
}

/// Nombres únicos conservando el orden (`dict.fromkeys`).
pub fn unique_names(names: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    names
        .iter()
        .filter(|name| seen.insert((*name).clone()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;

//! Port of `src/ezagent/agent/orchestration.py` (F3-02).
//!
//! Piezas de orquestación multiagente: pasos encadenados y guía fija del LLM.

use ez_core::pyvalue::{py_strip, text_or_empty};
use indexmap::IndexMap;
use serde_json::{json, Map, Value};

pub const MAX_STEPS: usize = 5;
const OPTIONAL_STEP_KEYS: [&str; 4] = ["model", "effort", "cwd", "name"];

/// Bloque fijo del system prompt. Vive en código para que un `system.md`
/// antiguo del usuario no deje al orquestador sin esta guía.
pub const ORCHESTRATION_HINT: &str = include_str!("text/orchestration_hint.txt");

pub const FLOW_PREFIX: &str = "todo el flujo: ";

/// Un paso de `then`: `agent`, `task` y, si se dieron, `model`/`effort`/`cwd`/`name`.
pub type Step = IndexMap<String, String>;

// DECISION: las guías se incrustan desde `src/ezagent/agent/agent_guides` (una
// sola fuente de verdad mientras convivan Python y Rust).
const GUIDES: [(&str, &str); 6] = [
    (
        "claude",
        include_str!("../../../../src/ezagent/agent/agent_guides/claude.guide.md"),
    ),
    (
        "codex",
        include_str!("../../../../src/ezagent/agent/agent_guides/codex.guide.md"),
    ),
    (
        "mcode",
        include_str!("../../../../src/ezagent/agent/agent_guides/mcode.guide.md"),
    ),
    (
        "opencode",
        include_str!("../../../../src/ezagent/agent/agent_guides/opencode.guide.md"),
    ),
    (
        "pi",
        include_str!("../../../../src/ezagent/agent/agent_guides/pi.guide.md"),
    ),
    (
        "qwen",
        include_str!("../../../../src/ezagent/agent/agent_guides/qwen.guide.md"),
    ),
];

/// Manual resumido de un agente (`agent_guides/<agente>.guide.md`).
pub fn load_guide(agent_key: &str) -> Option<String> {
    let valid = !agent_key.is_empty()
        && agent_key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !valid {
        return None;
    }
    GUIDES
        .iter()
        .find(|(name, _)| *name == agent_key)
        // `Path.read_text` traduce CRLF a LF; los archivos del repo se editan
        // en Windows y `include_str!` conserva el CRLF del checkout.
        .map(|(_, text)| text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Valida `then`; el `Err` es el mensaje legible de Python.
pub fn parse_steps(raw: &Value) -> Result<Vec<Step>, String> {
    let items = match raw {
        Value::Null => return Ok(Vec::new()),
        Value::Array(items) => items,
        _ => return Err("El parámetro «then» debe ser una lista de pasos.".into()),
    };
    if items.len() > MAX_STEPS {
        return Err(format!("«then» admite como máximo {MAX_STEPS} pasos."));
    }
    let mut steps = Vec::new();
    for (position, item) in items.iter().enumerate() {
        let index = position + 1;
        let Value::Object(map) = item else {
            return Err(format!("El paso {index} de «then» debe ser un objeto."));
        };
        let agent = py_strip(&text_or_empty(map.get("agent"))).to_string();
        // DECISION (del Python): sin `task` el paso recibe la respuesta anterior
        // tal cual, sin instrucción añadida.
        let task = py_strip(&text_or_empty(map.get("task"))).to_string();
        let task = if task.is_empty() {
            "{output}".to_string()
        } else {
            task
        };
        if agent.is_empty() {
            return Err(format!("El paso {index} de «then» necesita «agent»."));
        }
        let mut step = Step::new();
        step.insert("agent".into(), agent);
        step.insert("task".into(), task);
        for key in OPTIONAL_STEP_KEYS {
            if let Some(Value::String(value)) = map.get(key) {
                if !py_strip(value).is_empty() {
                    step.insert(key.into(), py_strip(value).to_string());
                }
            }
        }
        steps.push(step);
    }
    Ok(steps)
}

/// Opciones de `chain_spec`.
#[derive(Debug, Clone, Default)]
pub struct ChainOptions<'a> {
    pub cwd: Option<&'a str>,
    pub space: Option<&'a str>,
    pub notify: bool,
    pub deliver: bool,
    pub chain_id: &'a str,
    pub flow: Option<&'a [String]>,
}

fn step_value(step: &Step) -> Value {
    Value::Object(
        step.iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect::<Map<String, Value>>(),
    )
}

fn optional(text: Option<&str>) -> Value {
    text.map_or(Value::Null, |text| Value::String(text.to_string()))
}

/// Especificación de cadena para el primer paso; el resto va en `then`.
///
/// `flow` son los nombres de las etapas ya lanzadas (la fuente incluida): sirven
/// para avisar «terminó X, empieza Y» y, al final, «terminó todo el flujo».
pub fn chain_spec(source: &str, steps: &[Step], options: &ChainOptions<'_>) -> Value {
    let (first, rest) = steps
        .split_first()
        .map_or((None, &[][..]), |(f, r)| (Some(f), r));
    let get = |key: &str| first.and_then(|step| step.get(key)).map(String::as_str);
    let non_empty = |key: &str| get(key).filter(|text| !text.is_empty());
    let mut spec = Map::new();
    spec.insert("source".into(), json!(source));
    spec.insert("agent".into(), json!(get("agent").unwrap_or_default()));
    spec.insert("task".into(), json!(get("task").unwrap_or_default()));
    spec.insert(
        "cwd".into(),
        optional(non_empty("cwd").or(options.cwd.filter(|text| !text.is_empty()))),
    );
    spec.insert("name".into(), optional(non_empty("name")));
    spec.insert("background".into(), json!(false));
    spec.insert("gui".into(), json!(false));
    spec.insert("space".into(), optional(options.space));
    spec.insert("model".into(), json!(get("model").unwrap_or_default()));
    spec.insert("effort".into(), json!(get("effort").unwrap_or_default()));
    // Solo el último paso avisa; los intermedios no interrumpen al usuario.
    spec.insert(
        "notify_on_done".into(),
        json!(options.notify && rest.is_empty()),
    );
    spec.insert("notify_final".into(), json!(options.notify));
    spec.insert(
        "deliver_answer".into(),
        json!(options.deliver && rest.is_empty()),
    );
    spec.insert("deliver_final".into(), json!(options.deliver));
    if !options.chain_id.is_empty() {
        spec.insert("chain_id".into(), json!(options.chain_id));
    }
    let names: Vec<&String> = options
        .flow
        .into_iter()
        .flatten()
        .filter(|name| !py_strip(name).is_empty())
        .collect();
    if !names.is_empty() {
        spec.insert("flow".into(), json!(names));
    }
    if !rest.is_empty() {
        spec.insert(
            "then".into(),
            Value::Array(rest.iter().map(step_value).collect()),
        );
    }
    Value::Object(spec)
}

/// Texto de la tarea final de un flujo: «todo el flujo: a → b → c».
pub fn flow_task(names: &[String]) -> String {
    format!("{FLOW_PREFIX}{}", names.join(" → "))
}

/// Texto corto de los pasos, para el resumen de confirmación.
pub fn describe_steps(steps: &[Step]) -> String {
    let parts: Vec<String> = steps
        .iter()
        .map(|step| {
            let extra: Vec<&str> = ["model", "effort"]
                .iter()
                .filter_map(|key| step.get(*key).map(String::as_str))
                .filter(|value| !value.is_empty())
                .collect();
            let suffix = if extra.is_empty() {
                String::new()
            } else {
                format!(" ({})", extra.join(", "))
            };
            format!("{}{suffix}", step.get("agent").map_or("", String::as_str))
        })
        .collect();
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_validates_steps() {
        let steps = parse_steps(&json!([
            {"agent": " opencode ", "model": "m", "effort": " "},
            {"agent": "codex", "task": "haz {output}", "name": "x"}
        ]))
        .expect("pasos válidos");
        assert_eq!(steps[0]["task"], "{output}");
        assert_eq!(steps[0]["model"], "m");
        assert!(!steps[0].contains_key("effort"));
        assert_eq!(describe_steps(&steps), "opencode (m), codex");
        assert_eq!(
            parse_steps(&json!("x")),
            Err("El parámetro «then» debe ser una lista de pasos.".into())
        );
        assert_eq!(
            parse_steps(&json!([{"task": "a"}])),
            Err("El paso 1 de «then» necesita «agent».".into())
        );
        assert!(parse_steps(&json!([1])).is_err());
        assert!(parse_steps(&json!([{}, {}, {}, {}, {}, {}])).is_err());
        assert_eq!(parse_steps(&Value::Null), Ok(Vec::new()));
    }

    #[test]
    fn builds_chain_specs() {
        let steps =
            parse_steps(&json!([{"agent": "a"}, {"agent": "b", "model": "m"}])).expect("pasos");
        let options = ChainOptions {
            cwd: Some("C:/x"),
            notify: true,
            flow: Some(&["uno".to_string(), " ".to_string()]),
            ..ChainOptions::default()
        };
        let spec = chain_spec("src", &steps, &options);
        assert_eq!(spec["cwd"], "C:/x");
        assert_eq!(spec["notify_on_done"], false);
        assert_eq!(spec["notify_final"], true);
        assert_eq!(spec["flow"], json!(["uno"]));
        assert_eq!(spec["then"][0]["model"], "m");
        assert_eq!(flow_task(&["a".into(), "b".into()]), "todo el flujo: a → b");
    }

    #[test]
    fn guides_and_hint_are_embedded() {
        assert!(ORCHESTRATION_HINT.starts_with("# Orquestación de agentes"));
        assert!(load_guide("claude").is_some());
        assert!(load_guide("../x").is_none());
        assert!(load_guide("nadie").is_none());
    }
}

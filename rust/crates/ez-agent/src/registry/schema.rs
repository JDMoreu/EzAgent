//! Validación mínima de argumentos contra el esquema JSON de una herramienta.

use ez_core::pyvalue::{char_len, format_g, py_eq, python_repr, python_str};
use serde_json::{Map, Value};

const JSON_TYPES: [&str; 7] = [
    "string", "integer", "number", "boolean", "array", "object", "null",
];
fn type_label(expected: &Value) -> String {
    match expected {
        Value::Array(items) => items.iter().map(type_label).collect::<Vec<_>>().join(" o "),
        Value::String(name) => match name.as_str() {
            "string" => "texto".into(),
            "integer" => "entero".into(),
            "number" => "número".into(),
            "boolean" => "booleano".into(),
            "array" => "lista".into(),
            "object" => "objeto".into(),
            "null" => "nulo".into(),
            other => other.to_string(),
        },
        other => python_str(other),
    }
}

fn is_number(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(number)) => number.as_f64(),
        _ => None,
    }
}

fn matches_type(value: &Value, expected: &Value) -> bool {
    if let Value::Array(options) = expected {
        return options
            .iter()
            .any(|option| option.is_string() && matches_type(value, option));
    }
    let Some(name) = expected.as_str().filter(|name| JSON_TYPES.contains(name)) else {
        return true; // tipo desconocido: no se valida
    };
    match name {
        "string" => value.is_string(),
        "integer" => match value {
            Value::Number(number) => {
                number.is_i64()
                    || number.is_u64()
                    || number.as_f64().is_some_and(|f| f.fract() == 0.0)
            }
            _ => false,
        },
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => value.is_null(),
    }
}

fn join_name(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}.{child}")
    }
}

fn validate_object(name: &str, value: &Map<String, Value>, spec: &Value) -> Option<String> {
    let empty = Map::new();
    let properties = spec
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    if let Some(Value::Array(required)) = spec.get("required") {
        for key in required.iter().filter_map(Value::as_str) {
            if !value.contains_key(key) {
                return Some(format!(
                    "Falta el parámetro obligatorio: '{}'.",
                    join_name(name, key)
                ));
            }
        }
    }
    if spec.get("additionalProperties") == Some(&Value::Bool(false)) {
        if let Some(key) = value.keys().find(|key| !properties.contains_key(*key)) {
            return Some(format!(
                "El parámetro '{}' no está permitido.",
                join_name(name, key)
            ));
        }
    }
    for (key, item) in value {
        if let Some(child) = properties.get(key).filter(|child| child.is_object()) {
            if let Some(error) = validate_value(&join_name(name, key), item, child) {
                return Some(error);
            }
        }
    }
    None
}

fn validate_scalar(name: &str, value: &Value, spec: &Value) -> Option<String> {
    if let Some(expected) = spec.get("type").filter(|expected| !expected.is_null()) {
        if !matches_type(value, expected) {
            return Some(format!(
                "El parámetro '{name}' debe ser de tipo {}.",
                type_label(expected)
            ));
        }
    }
    if let Some(Value::Array(options)) = spec.get("enum") {
        if !options.iter().any(|option| py_eq(option, value)) {
            let shown: Vec<String> = options.iter().map(python_repr).collect();
            return Some(format!(
                "El parámetro '{name}' debe ser uno de: {}.",
                shown.join(", ")
            ));
        }
    }
    if let Value::String(text) = value {
        let length = char_len(text) as f64;
        if let Some(min) = is_number(spec.get("minLength")).filter(|min| length < *min) {
            return Some(format!(
                "El parámetro '{name}' debe tener al menos {} caracteres.",
                format_g(min)
            ));
        }
        if let Some(max) = is_number(spec.get("maxLength")).filter(|max| length > *max) {
            return Some(format!(
                "El parámetro '{name}' debe tener como máximo {} caracteres.",
                format_g(max)
            ));
        }
    }
    if let Value::Number(number) = value {
        let amount = number.as_f64().unwrap_or(0.0);
        if is_number(spec.get("minimum")).is_some_and(|min| amount < min) {
            return Some(format!(
                "El parámetro '{name}' debe ser mayor o igual que {}.",
                python_str(&spec["minimum"])
            ));
        }
        if is_number(spec.get("maximum")).is_some_and(|max| amount > max) {
            return Some(format!(
                "El parámetro '{name}' debe ser menor o igual que {}.",
                python_str(&spec["maximum"])
            ));
        }
    }
    None
}

fn validate_list(name: &str, items: &[Value], spec: &Value) -> Option<String> {
    let count = items.len() as f64;
    if let Some(min) = is_number(spec.get("minItems")).filter(|min| count < *min) {
        return Some(format!(
            "El parámetro '{name}' debe tener al menos {} elementos.",
            format_g(min)
        ));
    }
    if let Some(max) = is_number(spec.get("maxItems")).filter(|max| count > *max) {
        return Some(format!(
            "El parámetro '{name}' debe tener como máximo {} elementos.",
            format_g(max)
        ));
    }
    let item_spec = spec.get("items").filter(|spec| spec.is_object())?;
    items
        .iter()
        .enumerate()
        .find_map(|(index, item)| validate_value(&format!("{name}[{index}]"), item, item_spec))
}

fn validate_value(name: &str, value: &Value, spec: &Value) -> Option<String> {
    if let Some(error) = validate_scalar(name, value, spec) {
        return Some(error);
    }
    match value {
        Value::Array(items) => validate_list(name, items, spec),
        Value::Object(map) => validate_object(name, map, spec),
        _ => None,
    }
}

pub(super) fn validate_args(schema: &Value, args: &Map<String, Value>) -> Option<String> {
    validate_object("", args, schema)
}

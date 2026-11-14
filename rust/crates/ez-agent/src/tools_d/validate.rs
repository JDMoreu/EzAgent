//! Validación de los valores de los parámetros de una herramienta de `tools.d`.

use ez_core::llm_providers::urlsplit;
use ez_core::paths::resolve_safe;
use ez_core::pyvalue::{py_strip, python_int, python_str};
use serde_json::{json, Value};

use super::{int_text_re, ParamSpec, RunContext};

fn check_bounds(name: &str, number: f64, spec: &ParamSpec) -> Result<(), String> {
    if let Some(minimum) = &spec.minimum {
        if minimum.as_f64().is_some_and(|min| number < min) {
            return Err(format!(
                "«{name}» debe ser mayor o igual que {}.",
                python_str(minimum)
            ));
        }
    }
    if let Some(maximum) = &spec.maximum {
        if maximum.as_f64().is_some_and(|max| number > max) {
            return Err(format!(
                "«{name}» debe ser menor o igual que {}.",
                python_str(maximum)
            ));
        }
    }
    Ok(())
}

fn integer_of(name: &str, value: &Value) -> Result<i64, String> {
    let bad = || format!("«{name}» debe ser un entero.");
    match value {
        Value::Number(number) if number.is_i64() => number.as_i64().ok_or_else(bad),
        Value::String(text) if int_text_re().is_match(py_strip(text)) => {
            python_int(py_strip(text)).ok_or_else(bad)
        }
        _ => Err(bad()),
    }
}

pub(super) fn validate_value(
    name: &str,
    value: &Value,
    spec: &ParamSpec,
    ctx: &RunContext,
) -> Result<String, String> {
    let mut value = value.clone();
    match spec.kind.as_str() {
        "integer" => {
            let number = integer_of(name, &value)?;
            check_bounds(name, number as f64, spec)?;
            value = json!(number);
        }
        "number" => {
            let Some(number) = value.as_f64().filter(|_| value.is_number()) else {
                return Err(format!("«{name}» debe ser un número."));
            };
            check_bounds(name, number, spec)?;
        }
        "boolean" => {
            if !value.is_boolean() {
                return Err(format!("«{name}» debe ser true o false."));
            }
        }
        _ => {
            if !value.is_string() {
                return Err(format!("«{name}» debe ser texto."));
            }
        }
    }
    let validator = spec.validate.as_deref();
    if validator == Some("integer") {
        return Ok(integer_of(name, &value)?.to_string());
    }
    let text = match &value {
        Value::Bool(flag) => if *flag { "true" } else { "false" }.to_string(),
        other => python_str(other),
    };
    match validator {
        Some(kind @ ("path_in_read_roots" | "path_in_write_roots")) => {
            let roots = if kind == "path_in_read_roots" {
                &ctx.read_roots
            } else {
                &ctx.write_roots
            };
            resolve_safe(&text, &ctx.workspace, roots, &ctx.deny, &ctx.deny_globs)
                .map(|path| path.display().to_string())
                .map_err(|error| format!("ruta no permitida: {error}"))
        }
        Some("url_http") => {
            let parts = urlsplit(&text)
                .map_err(|_| format!("«{name}» debe ser una URL http o https válida."))?;
            if !matches!(parts.scheme.as_str(), "http" | "https") || parts.hostname.is_none() {
                return Err(format!("«{name}» debe ser una URL http o https válida."));
            }
            if parts.has_userinfo {
                return Err(format!("«{name}» no puede incluir credenciales en la URL."));
            }
            Ok(text)
        }
        Some("enum") => {
            let choices = spec.choices.clone().unwrap_or_default();
            if choices.contains(&text) {
                Ok(text)
            } else {
                Err(format!("«{name}» debe ser uno de: {}", choices.join(", ")))
            }
        }
        _ => Ok(text),
    }
}

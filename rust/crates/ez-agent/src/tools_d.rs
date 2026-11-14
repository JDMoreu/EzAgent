//! Port of `src/ezagent/agent/tools_d.py` (F3-08).
//!
//! Herramientas declarativas de `tools.d/*.toml`. Permiten añadir herramientas
//! sin escribir código. Reglas de seguridad obligatorias:
//!
//! 1. `command` es una lista de argumentos, nunca una cadena.
//! 2. Los parámetros se sustituyen después de validar, cada uno como un
//!    elemento independiente de la lista.
//! 3. El ejecutable debe estar en `[tools_d] allowed_executables`.
//! 4. Nivel mínimo 1; si falta `level` se asume 2.
//! 5. Validadores: `path_in_read_roots`, `path_in_write_roots`, `integer`,
//!    `url_http` y `enum:a,b,c`.
//! 6. Un archivo inválido se ignora y se registra, sin tumbar el arranque.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use ez_core::agent_config::AgentConfig;
use ez_core::pyvalue::{casefold, format_g, head_chars, py_strip, python_repr};
use indexmap::IndexMap;
use regex::{Captures, Regex};
use serde_json::{json, Map, Value};

use crate::registry::{ToolInfo, ToolResult, ToolSpec};

pub const DEFAULT_LEVEL: i64 = 2;
pub const DEFAULT_TIMEOUT_S: f64 = 10.0;
pub const MAX_OUTPUT_CHARS: usize = 8000;

const ALLOWED_TYPES: [&str; 4] = ["string", "integer", "number", "boolean"];

fn param_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("patrón válido"))
}

fn ident_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").expect("patrón válido"))
}

fn tool_name_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^[a-z][a-z0-9_]{0,63}$").expect("patrón válido"))
}

fn int_text_re() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"^[+-]?\d+$").expect("patrón válido"))
}

mod process;
pub use process::{clean_environment, run_captured, Completed, RunError};

/// `tomllib` → JSON: las fechas quedan como texto.
fn toml_to_json(value: &toml::Value) -> Value {
    match value {
        toml::Value::String(text) => Value::String(text.clone()),
        toml::Value::Integer(number) => json!(number),
        toml::Value::Float(number) => json!(number),
        toml::Value::Boolean(flag) => json!(flag),
        toml::Value::Datetime(moment) => Value::String(moment.to_string()),
        toml::Value::Array(items) => Value::Array(items.iter().map(toml_to_json).collect()),
        toml::Value::Table(table) => Value::Object(
            table
                .iter()
                .map(|(key, item)| (key.clone(), toml_to_json(item)))
                .collect(),
        ),
    }
}

/// Carga todas las herramientas válidas de `config.tools_d_dir`.
pub fn load_tools_d(config: &AgentConfig) -> Vec<ToolSpec> {
    let directory = &config.tools_d_dir;
    if !directory.is_dir() {
        return Vec::new();
    }
    let Ok(listing) = std::fs::read_dir(directory) else {
        tracing::warn!("tools.d: no se pudo leer {}", directory.display());
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = listing
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        })
        .collect();
    files.sort_by_key(|path| name_of(path).to_lowercase());
    let allowed: HashSet<String> = config
        .tools_d
        .allowed_executables
        .iter()
        .map(|entry| py_strip(entry))
        .filter(|entry| !entry.is_empty())
        .map(casefold)
        .collect();
    let mut tools: Vec<ToolSpec> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for path in files {
        let filename = name_of(&path);
        let parsed = std::fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|text| {
                text.parse::<toml::Table>()
                    .map_err(|error| error.to_string())
            });
        let table = match parsed {
            Ok(table) => table,
            Err(error) => {
                tracing::warn!("tools.d: se ignora {filename}: TOML inválido o ilegible ({error})");
                continue;
            }
        };
        let data = toml_to_json(&toml::Value::Table(table));
        let Some(tool) = build_tool(&data, config, &allowed, &filename) else {
            continue;
        };
        if !seen.insert(tool.name.clone()) {
            tracing::warn!(
                "tools.d: el nombre «{}» está duplicado; se ignora {filename}",
                tool.name
            );
            continue;
        }
        tools.push(tool);
    }
    tools
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

fn reject(filename: &str, reason: &str) -> Option<ToolSpec> {
    tracing::warn!("tools.d: se ignora {filename}: {reason}");
    None
}

#[derive(Debug, Clone)]
struct ParamSpec {
    kind: String,
    required: bool,
    validate: Option<String>,
    choices: Option<Vec<String>>,
    minimum: Option<Value>,
    maximum: Option<Value>,
}

type Parsed = (IndexMap<String, ParamSpec>, Map<String, Value>, Vec<String>);

fn is_number(value: &Value) -> bool {
    value.is_number()
}

fn build_tool(
    data: &Value,
    config: &AgentConfig,
    allowed: &HashSet<String>,
    filename: &str,
) -> Option<ToolSpec> {
    let name = data.get("name").and_then(Value::as_str).map(py_strip);
    let Some(name) = name.filter(|name| tool_name_re().is_match(name)) else {
        return reject(
            filename,
            "falta un «name» válido (minúsculas, números y guion bajo)",
        );
    };
    let description = data
        .get("description")
        .and_then(Value::as_str)
        .map(py_strip);
    let Some(description) = description.filter(|text| !text.is_empty()) else {
        return reject(filename, "falta «description»");
    };
    let command: Vec<String> = match data.get("command") {
        Some(Value::Array(items)) if !items.is_empty() => {
            let texts: Vec<&str> = items.iter().filter_map(Value::as_str).collect();
            if texts.len() != items.len() || texts.iter().any(|item| py_strip(item).is_empty()) {
                return reject(
                    filename,
                    "«command» solo admite argumentos de texto no vacíos",
                );
            }
            texts.iter().map(|item| (*item).to_string()).collect()
        }
        _ => {
            return reject(
                filename,
                "«command» debe ser una lista de argumentos, nunca una cadena",
            )
        }
    };
    if command.iter().any(|item| item.contains('\0')) {
        return reject(filename, "«command» contiene bytes nulos");
    }
    let executable = py_strip(&command[0]).to_string();
    // DECISION (del Python): se compara el nombre declarado tal cual (sin
    // resolverlo por PATH); una ruta completa no puede suplantar a uno listado.
    if !allowed.contains(&casefold(&executable)) {
        return reject(
            filename,
            &format!("el ejecutable «{executable}» no está en [tools_d] allowed_executables"),
        );
    }
    let level = match data.get("level") {
        None => DEFAULT_LEVEL,
        Some(Value::Number(number)) if number.is_i64() => number.as_i64().unwrap_or(DEFAULT_LEVEL),
        Some(_) => return reject(filename, "«level» debe ser un entero"),
    };
    let level = if level < 1 {
        tracing::warn!("tools.d: {filename} declara level={level}; se usa el nivel mínimo 1");
        1
    } else {
        level
    };
    let timeout_s = match data.get("timeout_s") {
        None => DEFAULT_TIMEOUT_S,
        Some(Value::Number(number)) if number.as_f64().is_some_and(|value| value > 0.0) => {
            number.as_f64().unwrap_or(DEFAULT_TIMEOUT_S)
        }
        Some(_) => return reject(filename, "«timeout_s» debe ser un número mayor que cero"),
    };
    let empty = Map::new();
    let params_data = match data.get("params") {
        None => &empty,
        Some(Value::Object(map)) => map,
        Some(_) => return reject(filename, "«params» debe ser una tabla"),
    };
    let (params, properties, required) = parse_params(params_data, filename)?;
    for element in &command {
        for found in param_re().captures_iter(element) {
            if !params.contains_key(&found[1]) {
                return reject(
                    filename,
                    &format!("«command» usa un parámetro no declarado: {{{}}}", &found[1]),
                );
            }
        }
    }
    // DECISION (del Python): sin taint por defecto; `untrusted_output = true` lo activa.
    let untrusted = match data.get("untrusted_output") {
        None => false,
        Some(Value::Bool(flag)) => *flag,
        Some(_) => return reject(filename, "«untrusted_output» debe ser true o false"),
    };
    let ctx = Arc::new(RunContext {
        tool_name: name.to_string(),
        command,
        params,
        workspace: config.workspace.clone(),
        read_roots: config.paths.read_roots.clone(),
        write_roots: config.paths.write_roots.clone(),
        deny: config.paths.deny.clone(),
        deny_globs: config.paths.deny_globs.clone(),
        timeout_s,
    });
    let handler_ctx = Arc::clone(&ctx);
    let describe_summary = format!("Ejecutar «{executable}» con una herramienta de tools.d");
    let schema = json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    });
    let mut spec = ToolSpec::new(
        name,
        description,
        schema,
        level,
        Arc::new(move |args| Ok(run_declarative_tool(&handler_ctx, args).into())),
    );
    spec.untrusted_output = untrusted;
    // DECISION (del Python): un segundo de margen para que el subprocess agote su timeout.
    spec.timeout_s = timeout_s + 1.0;
    spec.describe_fn = Some(Arc::new(move |_| ToolInfo {
        level,
        summary: describe_summary.clone(),
        ..ToolInfo::default()
    }));
    Some(spec)
}

fn parse_params(params_data: &Map<String, Value>, filename: &str) -> Option<Parsed> {
    let mut params: IndexMap<String, ParamSpec> = IndexMap::new();
    let mut properties = Map::new();
    let mut required: Vec<String> = Vec::new();
    for (raw_name, raw_spec) in params_data {
        if !ident_re().is_match(raw_name) {
            reject(
                filename,
                &format!(
                    "nombre de parámetro inválido: {}",
                    python_repr(&json!(raw_name))
                ),
            );
            return None;
        }
        let Value::Object(raw_spec) = raw_spec else {
            reject(
                filename,
                &format!("el parámetro «{raw_name}» debe ser una tabla"),
            );
            return None;
        };
        let kind = match raw_spec.get("type") {
            None => "string".to_string(),
            Some(Value::String(kind)) if ALLOWED_TYPES.contains(&kind.as_str()) => kind.clone(),
            Some(other) => {
                reject(
                    filename,
                    &format!(
                        "tipo inválido en el parámetro «{raw_name}»: {}",
                        python_repr(other)
                    ),
                );
                return None;
            }
        };
        let description = match raw_spec.get("description") {
            None => "",
            Some(Value::String(text)) => text,
            Some(_) => {
                reject(
                    filename,
                    &format!("«description» del parámetro «{raw_name}» debe ser texto"),
                );
                return None;
            }
        };
        let required_flag = match raw_spec.get("required") {
            None => true,
            Some(Value::Bool(flag)) => *flag,
            Some(_) => {
                reject(
                    filename,
                    &format!("«required» del parámetro «{raw_name}» debe ser true o false"),
                );
                return None;
            }
        };
        let (validate, choices) = parse_validator(raw_spec, raw_name, filename)?;
        let (minimum, maximum) = (raw_spec.get("minimum"), raw_spec.get("maximum"));
        for (bound_name, bound) in [("minimum", minimum), ("maximum", maximum)] {
            if bound.is_some_and(|bound| !bound.is_null() && !is_number(bound)) {
                reject(
                    filename,
                    &format!("«{bound_name}» del parámetro «{raw_name}» debe ser numérico"),
                );
                return None;
            }
        }
        let mut prop = Map::new();
        prop.insert("type".into(), json!(kind));
        if !py_strip(description).is_empty() {
            prop.insert("description".into(), json!(py_strip(description)));
        }
        if let Some(choices) = &choices {
            prop.insert("enum".into(), json!(choices));
        }
        for (key, bound) in [("minimum", minimum), ("maximum", maximum)] {
            if let Some(bound) = bound.filter(|bound| !bound.is_null()) {
                prop.insert(key.into(), bound.clone());
            }
        }
        properties.insert(raw_name.clone(), Value::Object(prop));
        params.insert(
            raw_name.clone(),
            ParamSpec {
                kind,
                required: required_flag,
                validate,
                choices,
                minimum: minimum.filter(|bound| !bound.is_null()).cloned(),
                maximum: maximum.filter(|bound| !bound.is_null()).cloned(),
            },
        );
        if required_flag {
            required.push(raw_name.clone());
        }
    }
    Some((params, properties, required))
}

type Validator = (Option<String>, Option<Vec<String>>);

fn parse_validator(
    raw_spec: &Map<String, Value>,
    param_name: &str,
    filename: &str,
) -> Option<Validator> {
    let Some(raw) = raw_spec.get("validate").filter(|raw| !raw.is_null()) else {
        return Some((None, None));
    };
    let Some(text) = raw.as_str() else {
        reject(
            filename,
            &format!("«validate» del parámetro «{param_name}» debe ser texto"),
        );
        return None;
    };
    let value = py_strip(text);
    if [
        "path_in_read_roots",
        "path_in_write_roots",
        "integer",
        "url_http",
    ]
    .contains(&value)
    {
        return Some((Some(value.to_string()), None));
    }
    if let Some(list) = value.strip_prefix("enum:") {
        let choices: Vec<String> = list
            .split(',')
            .map(py_strip)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect();
        if choices.is_empty() {
            reject(
                filename,
                &format!("el validador enum del parámetro «{param_name}» no tiene valores"),
            );
            return None;
        }
        return Some((Some("enum".into()), Some(choices)));
    }
    reject(
        filename,
        &format!("validador desconocido en el parámetro «{param_name}»: «{value}»"),
    );
    None
}

struct RunContext {
    tool_name: String,
    command: Vec<String>,
    params: IndexMap<String, ParamSpec>,
    workspace: PathBuf,
    read_roots: Vec<String>,
    write_roots: Vec<String>,
    deny: Vec<String>,
    deny_globs: Vec<String>,
    timeout_s: f64,
}

fn run_declarative_tool(ctx: &RunContext, args: &Map<String, Value>) -> ToolResult {
    let mut extra: Vec<&String> = args
        .keys()
        .filter(|key| !ctx.params.contains_key(*key))
        .collect();
    extra.sort();
    if !extra.is_empty() {
        let names: Vec<&str> = extra.iter().map(|name| name.as_str()).collect();
        return ToolResult::err(format!("Parámetros no admitidos: {}", names.join(", ")));
    }
    let mut values: IndexMap<String, String> = IndexMap::new();
    for (param_name, spec) in &ctx.params {
        let value = args.get(param_name).filter(|value| !value.is_null());
        let Some(value) = value else {
            if spec.required {
                return ToolResult::err(format!("Falta el parámetro obligatorio «{param_name}»."));
            }
            continue;
        };
        match validate_value(param_name, value, spec, ctx) {
            Ok(text) => {
                values.insert(param_name.clone(), text);
            }
            Err(message) => return ToolResult::err(format!("{}: {message}", ctx.tool_name)),
        }
    }
    let mut argv: Vec<String> = Vec::new();
    for element in &ctx.command {
        let replaced = param_re()
            .replace_all(element, |found: &Captures<'_>| {
                values
                    .get(&found[1])
                    .cloned()
                    .unwrap_or_else(|| found[0].to_string())
            })
            .into_owned();
        if let Some(unresolved) = param_re().captures(&replaced) {
            return ToolResult::err(format!(
                "Falta el valor del parámetro «{}» para ejecutar la herramienta.",
                &unresolved[1]
            ));
        }
        argv.push(replaced);
    }
    match run_captured(&argv, &ctx.workspace, &clean_environment(), ctx.timeout_s) {
        Err(RunError::Timeout { .. }) => ToolResult::err(format!(
            "La herramienta superó el tiempo máximo ({} s).",
            format_g(ctx.timeout_s)
        )),
        Err(RunError::Os(error)) => {
            ToolResult::err(format!("No se pudo ejecutar «{}»: {error}", ctx.command[0]))
        }
        Ok(done) => {
            let (stdout, stderr) = (clip(&done.stdout), clip(&done.stderr));
            let ok = done.returncode == 0;
            let error = (!ok).then(|| {
                let text = py_strip(&stderr);
                if text.is_empty() {
                    format!("La herramienta terminó con código {}.", done.returncode)
                } else {
                    text.to_string()
                }
            });
            ToolResult {
                ok,
                data: json!({"returncode": done.returncode, "stdout": stdout, "stderr": stderr}),
                error,
                outcome: "completed".into(),
                error_kind: None,
            }
        }
    }
}

fn clip(text: &str) -> String {
    head_chars(text, MAX_OUTPUT_CHARS).to_string()
}

mod validate;
use validate::validate_value;

#[cfg(test)]
mod tests;

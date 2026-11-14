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
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use ez_core::agent_config::AgentConfig;
use ez_core::llm_providers::urlsplit;
use ez_core::paths::resolve_safe;
use ez_core::pyvalue::{
    casefold, char_len, format_g, head_chars, py_strip, python_int, python_repr, python_str,
};
use indexmap::IndexMap;
use regex::{Captures, Regex};
use serde_json::{json, Map, Value};

use crate::registry::{ToolInfo, ToolResult, ToolReturn, ToolSpec};

pub const DEFAULT_LEVEL: i64 = 2;
pub const DEFAULT_TIMEOUT_S: f64 = 10.0;
pub const MAX_OUTPUT_CHARS: usize = 8000;

const ALLOWED_TYPES: [&str; 4] = ["string", "integer", "number", "boolean"];
const ENV_NAMES: [&str; 3] = ["PATH", "SYSTEMROOT", "TEMP"];

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

/// Entorno mínimo para subprocess: solo `PATH`, `SYSTEMROOT` y `TEMP`.
/// Nunca se heredan claves de API, tokens ni variables del proyecto.
pub fn clean_environment() -> Vec<(String, String)> {
    ENV_NAMES
        .iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| ((*name).to_string(), value))
        })
        .collect()
}

/// Resultado de un subproceso terminado.
#[derive(Debug, Clone)]
pub struct Completed {
    pub stdout: String,
    pub stderr: String,
    pub returncode: i64,
}

/// Por qué no se obtuvo un `Completed`.
#[derive(Debug, Clone)]
pub enum RunError {
    /// `subprocess.TimeoutExpired`, con la salida que alcanzó a escribir.
    Timeout { stdout: String },
    /// `OSError` al lanzar el proceso.
    Os(String),
}

fn drain<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        buffer
    })
}

/// Decodifica como `text=True, encoding="utf-8", errors="replace"` (finales de línea universales).
fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// `subprocess.run(argv, shell=False, cwd, env, timeout, capture_output=True)`.
///
/// `argv` nunca pasa por un shell; el entorno se reemplaza por completo.
pub fn run_captured(
    argv: &[String],
    cwd: &Path,
    env: &[(String, String)],
    timeout_s: f64,
) -> Result<Completed, RunError> {
    let Some((program, rest)) = argv.split_first() else {
        return Err(RunError::Os("comando vacío".into()));
    };
    let mut child = Command::new(program)
        .args(rest)
        .current_dir(cwd)
        .env_clear()
        .envs(
            env.iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| RunError::Os(error.to_string()))?;
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_s.max(0.0));
    let join = |handle: Option<std::thread::JoinHandle<Vec<u8>>>| {
        handle
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default()
    };
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(Completed {
                    stdout: decode(&join(stdout)),
                    stderr: decode(&join(stderr)),
                    returncode: i64::from(status.code().unwrap_or(-1)),
                });
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Timeout {
                    stdout: decode(&join(stdout)),
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => return Err(RunError::Os(error.to_string())),
        }
    }
}

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

fn validate_value(
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

/// Lo que devuelve un manejador de `tools.d` (para pruebas y otros módulos).
pub fn into_return(result: ToolResult) -> ToolReturn {
    ToolReturn::Result(result)
}

/// Longitud en caracteres, para las pruebas de recorte.
pub fn output_len(text: &str) -> usize {
    char_len(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(dir: &Path, exe: &str) -> AgentConfig {
        let file = dir.join("permissions.toml");
        let _ = std::fs::write(
            &file,
            format!(
                "[tools_d]\nallowed_executables = ['{exe}']\n[paths]\nread_roots = ['{}']\n",
                dir.display().to_string().replace('\\', "/")
            ),
        );
        let (mut config, _) = AgentConfig::load(Some(&file));
        config.workspace = dir.to_path_buf();
        config
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ez-toolsd-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("tools.d"));
        dir
    }

    #[test]
    fn loads_valid_tools_and_rejects_bad_ones() {
        let dir = scratch("load");
        let tools = dir.join("tools.d");
        let echo = if cfg!(windows) { "ping" } else { "echo" };
        let _ = std::fs::write(tools.join("a.toml"), format!(
            "name='saludar'\ndescription='Saluda'\ncommand=['{echo}', '{{texto}}']\nlevel=0\n[params.texto]\ntype='string'\nvalidate='enum:hola,adios'\n[params.n]\ntype='integer'\nrequired=false\nminimum=1\n"));
        let _ = std::fs::write(
            tools.join("b.toml"),
            "name='malo'\ndescription='x'\ncommand='echo hola'\n",
        );
        let _ = std::fs::write(
            tools.join("c.toml"),
            "name='libre'\ndescription='x'\ncommand=['powershell', 'x']\n",
        );
        let config = config(&dir, echo);
        let loaded = load_tools_d(&config);
        assert_eq!(loaded.len(), 1);
        let tool = &loaded[0];
        assert_eq!(tool.level, 1, "el nivel mínimo es 1");
        assert_eq!(tool.timeout_s, 11.0);
        assert_eq!(tool.parameters["required"], json!(["texto"]));
        assert_eq!(
            tool.parameters["properties"]["texto"]["enum"],
            json!(["hola", "adios"])
        );
        assert_eq!(tool.parameters["properties"]["n"]["minimum"], 1);
        assert_eq!(output_len("añ"), 2);
    }

    #[test]
    fn validates_arguments_before_running() {
        let dir = scratch("validate");
        let config = config(&dir, "ping");
        let ctx = RunContext {
            tool_name: "t".into(),
            command: vec!["ping".into(), "{u}".into()],
            params: IndexMap::from([
                (
                    "u".to_string(),
                    ParamSpec {
                        kind: "string".into(),
                        required: true,
                        validate: Some("url_http".into()),
                        choices: None,
                        minimum: None,
                        maximum: None,
                    },
                ),
                (
                    "n".to_string(),
                    ParamSpec {
                        kind: "integer".into(),
                        required: false,
                        validate: None,
                        choices: None,
                        minimum: Some(json!(1)),
                        maximum: Some(json!(5)),
                    },
                ),
            ]),
            workspace: config.workspace.clone(),
            read_roots: vec![],
            write_roots: vec![],
            deny: vec![],
            deny_globs: vec![],
            timeout_s: 2.0,
        };
        let run = |args: Value| run_declarative_tool(&ctx, args.as_object().expect("objeto"));
        assert_eq!(
            run(json!({})).error.as_deref(),
            Some("Falta el parámetro obligatorio «u».")
        );
        assert_eq!(
            run(json!({"u": "x", "z": 1})).error.as_deref(),
            Some("Parámetros no admitidos: z")
        );
        assert_eq!(
            run(json!({"u": "ftp://a"})).error.as_deref(),
            Some("t: «u» debe ser una URL http o https válida.")
        );
        assert_eq!(
            run(json!({"u": "http://a:b@c/"})).error.as_deref(),
            Some("t: «u» no puede incluir credenciales en la URL.")
        );
        assert_eq!(
            run(json!({"u": "http://a/", "n": "9"})).error.as_deref(),
            Some("t: «n» debe ser menor o igual que 5.")
        );
        assert_eq!(
            run(json!({"u": "http://a/", "n": true})).error.as_deref(),
            Some("t: «n» debe ser un entero.")
        );
    }

    #[test]
    fn captures_output_and_times_out() {
        let dir = std::env::temp_dir();
        let env = clean_environment();
        let (quick, slow): (Vec<String>, Vec<String>) = if cfg!(windows) {
            (
                ["ping", "-n", "1", "127.0.0.1"].map(String::from).to_vec(),
                ["ping", "-n", "8", "127.0.0.1"].map(String::from).to_vec(),
            )
        } else {
            (
                ["echo", "127.0.0.1"].map(String::from).to_vec(),
                ["sleep", "3"].map(String::from).to_vec(),
            )
        };
        let out = run_captured(&quick, &dir, &env, 10.0).expect("termina");
        assert_eq!(out.returncode, 0);
        assert!(out.stdout.contains("127.0.0.1"));
        assert!(matches!(
            run_captured(&slow, &dir, &env, 0.3),
            Err(RunError::Timeout { .. })
        ));
        let missing = ["no-existe-ez".to_string()];
        assert!(matches!(
            run_captured(&missing, &dir, &env, 1.0),
            Err(RunError::Os(_))
        ));
    }
}

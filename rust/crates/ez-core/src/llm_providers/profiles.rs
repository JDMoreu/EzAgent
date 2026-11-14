//! Resolución de proveedor, `llm.json` y catálogo del menú (F2-03).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde_json::{Map, Value};

use super::{
    normalize_api_style, normalize_provider, provider_spec, py_strip, ProviderSpec,
    CUSTOM_PROVIDER, DEFAULT_LLM_MODEL, OPENCODE_BASE_URL, PROVIDERS,
};
use crate::casefold::casefold;
use crate::config::{python_str, truthy};
use crate::dirs;
use crate::pyjson::{dumps_with, DumpOptions};
use crate::pyloads;

/// Resultado de `resolve_llm`. Ni `Debug` ni logs enseñan la clave.
#[derive(Clone, PartialEq, Eq)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub source: &'static str,
    pub api_style: &'static str,
}

impl LlmConfig {
    /// Etiqueta del preset, o el nombre del proveedor si no es uno integrado.
    pub fn label(&self) -> String {
        provider_spec(Some(&self.provider))
            .map_or_else(|| self.provider.clone(), |s| s.label.into())
    }
}

impl std::fmt::Debug for LlmConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LlmConfig")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("source", &self.source)
            .field("api_style", &self.api_style)
            .finish_non_exhaustive()
    }
}

/// Argumentos de `resolve_llm`; `None` es «no se pasó» (y vacío cuenta igual).
#[derive(Default, Clone)]
pub struct ResolveArgs<'a> {
    pub provider: Option<&'a str>,
    pub model: Option<&'a str>,
    pub base_url: Option<&'a str>,
    pub api_key: Option<&'a str>,
    pub api_style: Option<&'a str>,
    /// `None` usa el entorno del proceso, como `environ=None`.
    pub environ: Option<&'a HashMap<String, String>>,
    pub config_file: Option<&'a Path>,
}

/// Entrada del catálogo del menú (`load_provider_catalog`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub label: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub api_style: String,
    /// Solo en configuraciones guardadas: el proveedor al que pertenecen.
    pub provider: Option<String>,
    pub saved: bool,
}

/// Argumentos de `save_provider_profile`.
#[derive(Default, Clone)]
pub struct SaveArgs<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub base_url: &'a str,
    pub api_key: &'a str,
    pub api_style: &'a str,
    pub profile_name: &'a str,
    pub config_file: Option<&'a Path>,
}

fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|text| !text.is_empty())
}

/// `Path(text)` de Windows: separadores `\`, sin duplicados ni `.` ni cola.
fn py_path(text: &str) -> PathBuf {
    if !cfg!(windows) {
        return PathBuf::from(text);
    }
    let unc = text.starts_with("\\\\") || text.starts_with("//");
    let rooted = text.starts_with(['/', '\\']);
    let parts: Vec<&str> = text
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    let body = parts.join("\\");
    let joined = match (unc, rooted) {
        (true, _) => format!("\\\\{body}"),
        (false, true) => format!("\\{body}"),
        _ => body,
    };
    PathBuf::from(if joined.is_empty() {
        ".".into()
    } else {
        joined
    })
}

/// `Path(text).expanduser()` para `~` y `~/...`.
fn expand_user(text: &str) -> PathBuf {
    match text.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            py_path(&format!("{}{rest}", dirs::home().display()))
        }
        _ => py_path(text),
    }
}

/// `config_path`: `EZ_LLM_CONFIG`, o `<APPDATA|XDG_CONFIG_HOME|~/.config>/EzAgent/llm.json`.
pub fn config_path() -> PathBuf {
    if let Some(explicit) = process_env("EZ_LLM_CONFIG") {
        return expand_user(&explicit);
    }
    let base = process_env("APPDATA")
        .or_else(|| process_env("XDG_CONFIG_HOME"))
        .unwrap_or_else(|| format!("{}/.config", dirs::home().display()));
    py_path(&format!("{base}/EzAgent/llm.json"))
}

/// Mensaje de `UnicodeDecodeError` de Python para un UTF-8 inválido.
fn utf8_message(bytes: &[u8], error: &std::str::Utf8Error) -> String {
    let position = error.valid_up_to();
    let byte = bytes[position];
    let reason = match error.error_len() {
        None => "unexpected end of data",
        Some(_) if matches!(byte, 0x80..=0xC1 | 0xF5..=0xFF) => "invalid start byte",
        Some(_) => "invalid continuation byte",
    };
    format!("'utf-8' codec can't decode byte 0x{byte:02x} in position {position}: {reason}")
}

/// `Path.read_text(encoding="utf-8")`: traduce `\r\n` y `\r` a `\n`.
fn read_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let text = std::str::from_utf8(&bytes).map_err(|error| utf8_message(&bytes, &error))?;
    Ok(text.replace("\r\n", "\n").replace('\r', "\n"))
}

fn invalid_file(path: &Path, detail: &str) -> String {
    format!("llm.json inválido en {}: {detail}", path.display())
}

/// `json.loads(path.read_text())` con los textos de error de `_load_file`.
fn read_json(path: &Path) -> Result<Value, String> {
    let text = read_text(path).map_err(|detail| invalid_file(path, &detail))?;
    pyloads::loads(&text).map_err(|error| invalid_file(path, &error.to_string()))
}

fn load_file(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        if process_env("EZ_LLM_CONFIG").is_some() {
            return Err(format!("No existe EZ_LLM_CONFIG: {}", path.display()));
        }
        return Ok(Map::new());
    }
    Ok(match read_json(path)? {
        Value::Object(map) => map,
        _ => Map::new(),
    })
}

fn providers_of(data: &Map<String, Value>) -> Map<String, Value> {
    match data.get("providers") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    }
}

/// `_profile_for`: perfil por id canónico, tolerando alias como `claude`.
fn profile_for(profiles: &Map<String, Value>, selected: &str) -> Map<String, Value> {
    if let Some(Value::Object(direct)) = profiles.get(selected) {
        return direct.clone();
    }
    for (name, raw) in profiles {
        if let Value::Object(profile) = raw {
            if normalize_provider(Some(name)) == selected {
                return profile.clone();
            }
        }
    }
    Map::new()
}

fn text_value(value: Option<&str>) -> Value {
    value.map_or(Value::Null, |text| Value::String(text.to_string()))
}

/// `a or b or c`: el primer valor verdadero, o el último si ninguno lo es.
fn first_truthy(candidates: &[Value]) -> Value {
    candidates
        .iter()
        .find(|value| truthy(value))
        .or(candidates.last())
        .cloned()
        .unwrap_or(Value::Null)
}

struct EnvView<'a>(Option<&'a HashMap<String, String>>);

impl EnvView<'_> {
    fn get(&self, name: &str) -> Value {
        let found = match self.0 {
            Some(map) => map.get(name).cloned(),
            None => std::env::var(name).ok(),
        };
        found.map_or(Value::Null, Value::String)
    }
}

/// Preset (propio o sintético para un proveedor desconocido).
struct SpecView {
    base_url: String,
    api_key_env: String,
    default_model: String,
    api_style: String,
}

fn spec_view(selected: &str) -> SpecView {
    let owned = |spec: &ProviderSpec| SpecView {
        base_url: spec.base_url.into(),
        api_key_env: spec.api_key_env.into(),
        default_model: spec.default_model.into(),
        api_style: spec.api_style.into(),
    };
    match provider_spec(Some(selected)) {
        Some(spec) => owned(spec),
        None => owned(&ProviderSpec {
            name: "",
            label: "",
            base_url: "",
            api_key_env: "LLM_API_KEY",
            default_model: "",
            api_style: "openai",
        }),
    }
}

/// `repr(str)` de Python para el nombre del proveedor en el error.
fn py_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// `resolve_llm`: argumentos > entorno > perfil del archivo > preset.
pub fn resolve_llm(args: &ResolveArgs<'_>) -> Result<LlmConfig, String> {
    let env = EnvView(args.environ);
    let file_path = args.config_file.map_or_else(config_path, Path::to_path_buf);
    let data = load_file(&file_path)?;
    let profiles = providers_of(&data);
    let chosen = first_truthy(&[
        text_value(args.provider),
        env.get("LLM_PROVIDER"),
        data.get("provider").cloned().unwrap_or(Value::Null),
        Value::String("opencode".into()),
    ]);
    let selected = normalize_provider(Some(&python_str(&chosen)));
    let spec = spec_view(&selected);
    let profile = profile_for(&profiles, &selected);
    let get = |source: &Map<String, Value>, key: &str| source.get(key).cloned().unwrap_or_default();

    let (mut env_base, mut env_model) = (env.get("LLM_BASE_URL"), env.get("LLM_MODEL"));
    if selected != "opencode" {
        // Los valores de fábrica de OpenCode en .env no deben apuntar al endpoint equivocado.
        if matches!(&env_base, Value::String(s) if !s.is_empty() && s.trim_end_matches('/') == OPENCODE_BASE_URL)
        {
            env_base = Value::Null;
        }
        if env_model == Value::String(DEFAULT_LLM_MODEL.into()) {
            env_model = Value::Null;
        }
    }
    let base = first_truthy(&[
        text_value(args.base_url),
        env_base,
        get(&profile, "base_url"),
        get(&data, "base_url"),
        Value::String(spec.base_url),
    ]);
    let model = first_truthy(&[
        text_value(args.model),
        env_model,
        get(&profile, "model"),
        get(&data, "model"),
        Value::String(spec.default_model),
    ]);
    let key = first_truthy(&[
        text_value(args.api_key),
        env.get("LLM_API_KEY"),
        get(&profile, "api_key"),
        get(&data, "api_key"),
        env.get(&spec.api_key_env),
        Value::String(String::new()),
    ]);
    let style = first_truthy(&[
        text_value(args.api_style),
        env.get("LLM_API_STYLE"),
        get(&profile, "api_style"),
        Value::String(spec.api_style),
    ]);
    let base = py_strip(&python_str(&base)).to_string();
    if base.is_empty() {
        return Err(format!(
            "El proveedor {} necesita LLM_BASE_URL o base_url en llm.json.",
            py_repr(&selected)
        ));
    }
    let from_env = ["LLM_PROVIDER", "LLM_MODEL", "LLM_BASE_URL", "LLM_API_KEY"]
        .iter()
        .any(|name| truthy(&env.get(name)));
    let source = match (from_env, data.is_empty()) {
        (true, _) => "env",
        (false, false) => "file",
        (false, true) => "defaults",
    };
    Ok(LlmConfig {
        provider: selected,
        model: py_strip(&python_str(&model)).to_string(),
        base_url: base,
        api_key: py_strip(&python_str(&key)).to_string(),
        source,
        api_style: normalize_api_style(Some(&python_str(&style))),
    })
}

/// `str.title()` para ASCII y letras con caja.
fn py_title(text: &str) -> String {
    let mut out = String::new();
    let mut previous_cased = false;
    for c in text.chars() {
        let cased = c.is_uppercase() || c.is_lowercase();
        if cased && previous_cased {
            out.extend(c.to_lowercase());
        } else if cased {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        previous_cased = cased;
    }
    out
}

fn string_of(map: &Map<String, Value>, key: &str) -> Value {
    map.get(key).cloned().unwrap_or(Value::Null)
}

/// `str(a or b or "")` sobre varios candidatos.
fn text_of(candidates: &[Value]) -> String {
    python_str(&first_truthy(candidates))
}

fn builtin_entry(spec: &ProviderSpec, env: &EnvView<'_>) -> CatalogEntry {
    CatalogEntry {
        label: spec.label.into(),
        base_url: spec.base_url.into(),
        model: spec.default_model.into(),
        api_key: python_str(&first_truthy(&[
            env.get(spec.api_key_env),
            Value::String(String::new()),
        ])),
        api_style: spec.api_style.into(),
        provider: None,
        saved: false,
    }
}

fn profile_entry(
    name: &str,
    raw: &Map<String, Value>,
    base: Option<&CatalogEntry>,
) -> CatalogEntry {
    let spec = provider_spec(Some(name));
    let builtin = spec.is_some() && name != CUSTOM_PROVIDER;
    let from_base = |field: fn(&CatalogEntry) -> &str| {
        base.map_or(Value::Null, |entry| Value::String(field(entry).into()))
    };
    let label = if builtin {
        Value::String(spec.map_or(String::new(), |s| s.label.into()))
    } else {
        string_of(raw, "label")
    };
    let default_model = spec.map_or("", |s| s.default_model);
    let saved = truthy(&string_of(raw, "saved"));
    CatalogEntry {
        label: text_of(&[label, Value::String(py_title(name))]),
        api_style: normalize_api_style(Some(&python_str(&first_truthy(&[
            string_of(raw, "api_style"),
            from_base(|e| &e.api_style),
        ]))))
        .into(),
        base_url: text_of(&[
            string_of(raw, "base_url"),
            from_base(|e| &e.base_url),
            Value::String(String::new()),
        ]),
        model: text_of(&[
            string_of(raw, "model"),
            Value::String(default_model.into()),
            from_base(|e| &e.model),
            Value::String(String::new()),
        ]),
        api_key: text_of(&[
            string_of(raw, "api_key"),
            from_base(|e| &e.api_key),
            Value::String(String::new()),
        ]),
        provider: saved.then(|| {
            text_of(&[
                string_of(raw, "provider"),
                Value::String(CUSTOM_PROVIDER.into()),
            ])
        }),
        saved,
    }
}

/// `load_provider_catalog`: integrados más perfiles guardados, en orden.
pub fn load_provider_catalog(
    environ: Option<&HashMap<String, String>>,
    config_file: Option<&Path>,
) -> Result<IndexMap<String, CatalogEntry>, String> {
    let env = EnvView(environ);
    let data = load_file(&config_file.map_or_else(config_path, Path::to_path_buf))?;
    let mut catalog: IndexMap<String, CatalogEntry> = PROVIDERS
        .iter()
        .map(|spec| (spec.name.to_string(), builtin_entry(spec, &env)))
        .collect();
    for (raw_name, raw) in &providers_of(&data) {
        let raw_key = py_strip(raw_name);
        let Value::Object(raw) = raw else { continue };
        if raw_key.is_empty() {
            continue;
        }
        let name = normalize_provider(Some(raw_key));
        let entry = profile_entry(&name, raw, catalog.get(&name));
        catalog.insert(name, entry);
    }
    Ok(catalog)
}

/// Contenido del perfil que se guarda para un proveedor.
fn profile_value(args: &SaveArgs<'_>, label: String) -> Map<String, Value> {
    let mut profile = Map::new();
    profile.insert("label".into(), Value::String(label));
    profile.insert(
        "base_url".into(),
        Value::String(py_strip(args.base_url).into()),
    );
    profile.insert("model".into(), Value::String(py_strip(args.model).into()));
    profile.insert(
        "api_key".into(),
        Value::String(py_strip(args.api_key).into()),
    );
    profile.insert(
        "api_style".into(),
        Value::String(normalize_api_style(Some(args.api_style)).into()),
    );
    profile
}

fn previous_label(providers: &Map<String, Value>, name: &str) -> String {
    if let Some(Value::Object(previous)) = providers.get(name) {
        if truthy(&string_of(previous, "label")) {
            return python_str(&string_of(previous, "label"));
        }
    }
    provider_spec(Some(name)).map_or_else(|| py_title(name), |spec| spec.label.into())
}

/// `save_provider_profile`: guarda el perfil y lo deja como proveedor activo.
pub fn save_provider_profile(args: &SaveArgs<'_>) -> Result<String, String> {
    let name = normalize_provider(Some(args.provider));
    let profile_name = py_strip(args.profile_name);
    if profile_name.chars().count() > 80 {
        return Err("El nombre de la configuración admite hasta 80 caracteres.".into());
    }
    let path = args.config_file.map_or_else(config_path, Path::to_path_buf);
    let mut data = Map::new();
    if path.exists() {
        if let Value::Object(loaded) = read_json(&path)? {
            data = loaded;
        }
    }
    let mut providers = match data.get("providers") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    let profile = profile_value(args, previous_label(&providers, &name));
    providers.insert(name.clone(), Value::Object(profile.clone()));
    if !profile_name.is_empty() {
        // DECISION: las copias con nombre conviven con el proveedor; guardar
        // otro modelo para el mismo proveedor no pisa los presets viejos.
        let mut saved = profile;
        saved.insert("label".into(), Value::String(profile_name.into()));
        saved.insert("provider".into(), Value::String(name.clone()));
        saved.insert("saved".into(), Value::Bool(true));
        let key = format!("saved:{}", casefold(profile_name));
        providers.insert(key, Value::Object(saved));
    }
    data.insert("providers".into(), Value::Object(providers));
    data.insert("provider".into(), Value::String(name.clone()));
    write_atomic(&path, &data)?;
    Ok(name)
}

/// `json.dumps(..., ensure_ascii=False, indent=2) + "\n"` con `tmp` + `replace`.
fn write_atomic(path: &Path, data: &Map<String, Value>) -> Result<(), String> {
    let mut options = DumpOptions::default().with_indent(2);
    options.ensure_ascii = false;
    let body = dumps_with(&Value::Object(data.clone()), &options);
    // `Path.write_text` traduce los finales de línea a los del sistema.
    let text = if cfg!(windows) {
        format!("{body}\n").replace('\n', "\r\n")
    } else {
        format!("{body}\n")
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    std::fs::write(&tmp, text).map_err(|error| error.to_string())?;
    std::fs::rename(&tmp, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_and_repr_follow_python() {
        assert_eq!(py_title("saved:trabajo"), "Saved:Trabajo");
        assert_eq!(py_title("open-ai"), "Open-Ai");
        assert_eq!(py_repr("mistral"), "'mistral'");
        assert_eq!(py_repr("o'k"), "\"o'k\"");
    }

    #[test]
    fn utf8_errors_use_python_wording() {
        let bytes: Vec<u8> = vec![b'a', 0xff];
        let error = std::str::from_utf8(&bytes).unwrap_err();
        assert_eq!(
            utf8_message(&bytes, &error),
            "'utf-8' codec can't decode byte 0xff in position 1: invalid start byte"
        );
    }
}

//! Paridad de `llm_providers.py` contra `parity/fixtures/llm_providers.jsonl` (F2-03).
//!
//! Un solo `#[test]`: los casos tocan el entorno y el directorio actual, que
//! son del proceso.

use std::collections::HashMap;
use std::path::Path;

use ez_core::llm_providers::{
    capabilities_for, config_path, is_local_endpoint, load_provider_catalog, match_model,
    models_url, needs_api_key, normalize_api_style, normalize_provider, provider_models,
    provider_spec, resolve_llm, save_provider_profile, ResolveArgs, SaveArgs,
};
use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/llm_providers.jsonl";
const CONFIG_FILE: &str = "llm.json";
const ENV_NAMES: [&str; 5] = [
    "EZ_LLM_CONFIG",
    "APPDATA",
    "XDG_CONFIG_HOME",
    "USERPROFILE",
    "HOME",
];

/// El texto o `None`; un valor que no es texto se trata como `str(value)`.
fn text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        other => Some(other.to_string()),
    }
}

fn field<'a>(input: &'a Value, name: &str) -> &'a Value {
    input.get(name).unwrap_or(&Value::Null)
}

fn string_map(value: &Value) -> HashMap<String, String> {
    value
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(key, value)| (key.clone(), value.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Escribe el `llm.json` del caso como lo hace `_put_file` del oráculo.
fn put_file(file: &Value) {
    let path = Path::new(CONFIG_FILE);
    let _ = std::fs::remove_file(path);
    let body = match file {
        Value::Null => return,
        Value::Array(items) if items.first() == Some(&json!("raw")) => {
            items[1].as_str().unwrap_or_default().to_string()
        }
        other => {
            let mut options = DumpOptions::default().with_indent(2);
            options.ensure_ascii = false;
            let body = format!("{}\n", dumps_with(other, &options));
            if cfg!(windows) {
                body.replace('\n', "\r\n")
            } else {
                body
            }
        }
    };
    std::fs::write(path, body).expect("llm.json del caso");
}

fn mark(text: &str, temp: &str) -> String {
    text.replace(temp, "<tmp>")
}

fn run_case(case: &FixtureCase, temp: &str) -> Result<Value, String> {
    let input = &case.input;
    let mark = |value: String| mark(&value, temp);
    Ok(match case.function.as_str() {
        "normalize_api_style" => {
            let value = text(field(input, "value"));
            json!({"style": normalize_api_style(value.as_deref())})
        }
        "normalize_provider" => {
            let name = text(field(input, "name"));
            json!({"provider": normalize_provider(name.as_deref())})
        }
        "provider_spec" => {
            let name = text(field(input, "name"));
            let spec = provider_spec(name.as_deref()).map(|spec| {
                json!({"name": spec.name, "label": spec.label, "base_url": spec.base_url,
                    "api_key_env": spec.api_key_env, "default_model": spec.default_model,
                    "api_style": spec.api_style})
            });
            json!({"spec": spec})
        }
        "provider_models" => {
            let provider = text(field(input, "provider"));
            let models: Vec<Value> = provider_models(provider.as_deref())
                .iter()
                .map(|(id, label)| json!({"id": id, "label": label}))
                .collect();
            json!({"models": models})
        }
        "is_local_endpoint" => {
            json!({"local": is_local_endpoint(field(input, "base_url").as_str().unwrap_or(""))})
        }
        "needs_api_key" => {
            let provider = text(field(input, "provider"));
            let base = field(input, "base_url").as_str().unwrap_or("");
            json!({"needed": needs_api_key(provider.as_deref(), base)})
        }
        "capabilities_for" => {
            let model = text(field(input, "model"));
            let caps = capabilities_for(Some("openai"), model.as_deref(), "openai");
            json!({"capabilities": {"tools": caps.tools, "temperature": caps.temperature,
                "max_output": caps.max_output, "reasoning": caps.reasoning}})
        }
        "match_model" => {
            let model = text(field(input, "model"));
            let available: Vec<String> = field(input, "available")
                .as_array()
                .map(|items| items.iter().filter_map(text).collect())
                .unwrap_or_default();
            json!({"model": match_model(model.as_deref(), &available)})
        }
        "models_url" => {
            let base = field(input, "base_url").as_str().unwrap_or("");
            let style = field(input, "api_style").as_str().unwrap_or("");
            json!({"url": models_url(base, style)})
        }
        "resolve_llm" => resolve_case(input, &mark)?,
        "load_provider_catalog" => catalog_case(input, &mark)?,
        "save_provider_profile" => save_case(input, &mark)?,
        "config_path" => path_case(input, temp),
        other => return Err(format!("función sin portar: {other}")),
    })
}

fn resolve_case(input: &Value, mark: &dyn Fn(String) -> String) -> Result<Value, String> {
    put_file(field(input, "file"));
    let kwargs = field(input, "kwargs");
    let pick = |name: &str| kwargs.get(name).and_then(Value::as_str);
    let environ = string_map(field(input, "environ"));
    let file = Path::new(CONFIG_FILE);
    let args = ResolveArgs {
        provider: pick("provider"),
        model: pick("model"),
        base_url: pick("base_url"),
        api_key: pick("api_key"),
        api_style: pick("api_style"),
        environ: Some(&environ),
        config_file: pick("config_file").map(|_| file),
    };
    let result = resolve_llm(&args).map_err(mark)?;
    Ok(json!({"provider": result.provider, "model": result.model,
        "base_url": result.base_url, "api_key": result.api_key, "source": result.source,
        "api_style": result.api_style, "label": result.label()}))
}

fn catalog_case(input: &Value, mark: &dyn Fn(String) -> String) -> Result<Value, String> {
    put_file(field(input, "file"));
    let environ = string_map(field(input, "environ"));
    let catalog =
        load_provider_catalog(Some(&environ), Some(Path::new(CONFIG_FILE))).map_err(mark)?;
    let mut out = Map::new();
    for (name, entry) in catalog {
        let mut item = Map::new();
        item.insert("label".into(), json!(entry.label));
        item.insert("api_style".into(), json!(entry.api_style));
        item.insert("base_url".into(), json!(entry.base_url));
        item.insert("model".into(), json!(entry.model));
        item.insert("api_key".into(), json!(entry.api_key));
        if let Some(provider) = entry.provider {
            item.insert("provider".into(), json!(provider));
        }
        if entry.saved {
            item.insert("saved".into(), json!("true"));
        }
        out.insert(name, Value::Object(item));
    }
    Ok(json!({"catalog": out}))
}

fn save_case(input: &Value, mark: &dyn Fn(String) -> String) -> Result<Value, String> {
    put_file(field(input, "file"));
    let kwargs = field(input, "kwargs");
    let pick = |name: &str| kwargs.get(name).and_then(Value::as_str).unwrap_or("");
    let style = if kwargs.get("api_style").is_some() {
        pick("api_style")
    } else {
        "openai"
    };
    let args = SaveArgs {
        provider: pick("provider"),
        model: pick("model"),
        base_url: pick("base_url"),
        api_key: pick("api_key"),
        api_style: style,
        profile_name: pick("profile_name"),
        config_file: Some(Path::new(CONFIG_FILE)),
    };
    let name = save_provider_profile(&args).map_err(mark)?;
    let bytes = std::fs::read(CONFIG_FILE).expect("llm.json guardado");
    let body = String::from_utf8(bytes).expect("UTF-8");
    let newline = if body.contains('\r') { "crlf" } else { "lf" };
    let data: Value = serde_json::from_str(&body).expect("JSON guardado");
    Ok(json!({"name": name, "text": mark(body), "newline": newline, "data": data}))
}

fn path_case(input: &Value, temp: &str) -> Value {
    for (name, value) in field(input, "env").as_object().expect("env del caso") {
        match value.as_str() {
            Some(text) => std::env::set_var(name, text.replace("<tmp>", temp)),
            None => std::env::remove_var(name),
        }
    }
    json!({"path": mark(&config_path().display().to_string(), temp)})
}

fn clear_env() {
    for name in ENV_NAMES {
        if name != "USERPROFILE" && name != "HOME" {
            std::env::remove_var(name);
        }
    }
}

#[test]
fn every_llm_providers_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures legibles");
    let temp = std::env::temp_dir().join(format!("ez-llm-parity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("carpeta temporal");
    let temp_text = temp.display().to_string();
    let original = std::env::current_dir().expect("directorio actual");
    let saved: Vec<_> = ENV_NAMES
        .iter()
        .map(|n| (*n, std::env::var_os(n)))
        .collect();
    std::env::set_current_dir(&temp).expect("directorio temporal");
    std::env::set_var("USERPROFILE", &temp);
    std::env::set_var("HOME", &temp);
    let mut failures = Vec::new();
    for case in &cases {
        clear_env();
        if let Some(path) = field(&case.input, "env_file").as_str() {
            std::env::set_var("EZ_LLM_CONFIG", path.replace("<tmp>", &temp_text));
        }
        let got = run_case(case, &temp_text);
        let ok = match (&got, &case.error) {
            (Ok(value), None) => value == &case.output,
            (Err(message), Some(expected)) => message == expected,
            _ => false,
        };
        if !ok {
            failures.push(format!(
                "{}: {got:?} != {:?} / {:?}",
                case.case, case.output, case.error
            ));
        }
    }
    std::env::set_current_dir(original).expect("restaurar directorio");
    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
    let _ = std::fs::remove_dir_all(&temp);
    assert!(
        failures.is_empty(),
        "{} fallos:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(cases.len(), 228);
}

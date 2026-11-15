//! Paridad de `hud/settings.py`, `settings_window.py` y `hotkey_field.py`
//! contra `parity/fixtures/hud_settings.jsonl` (P-HUD-B).

use ez_core::pyjson::{dumps, float_repr};
use ez_core::pyvalue::py_eq;
use ez_hud::model::settings::{
    Device, MenuSettings, OptionInput, OptionSource, ProviderEntry, SelectOption, SettingValue,
    SettingsError, SettingsValues,
};
use ez_parity::fixtures::FixtureCase;

// -------------------------------------------------------------------- JSON

pub(super) fn s(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn o(pairs: &[(&str, String)]) -> String {
    let body: Vec<String> = pairs
        .iter()
        .map(|(key, value)| format!("{}: {value}", s(key)))
        .collect();
    format!("{{{}}}", body.join(", "))
}

pub(super) fn a(items: &[String]) -> String {
    format!("[{}]", items.join(", "))
}

pub(super) fn b(flag: bool) -> String {
    flag.to_string()
}

pub(super) fn i(number: i64) -> String {
    number.to_string()
}

pub(super) fn f(number: f64) -> String {
    float_repr(number)
}

pub(super) fn val(value: &SettingValue) -> String {
    match value {
        SettingValue::None => "null".into(),
        SettingValue::Bool(flag) => b(*flag),
        SettingValue::Int(number) => i(*number),
        SettingValue::Float(number) => f(*number),
        SettingValue::Text(text) => s(text),
    }
}

pub(super) fn vals(values: &SettingsValues) -> String {
    let pairs: Vec<(&str, String)> = values
        .iter()
        .map(|(key, value)| (key, val(value)))
        .collect();
    o(&pairs)
}

pub(super) fn opts(options: &[SelectOption]) -> String {
    a(&options
        .iter()
        .map(|option| o(&[("value", val(&option.value)), ("label", s(&option.label))]))
        .collect::<Vec<_>>())
}

pub(super) fn names(keys: &[&str]) -> String {
    a(&keys.iter().map(|key| s(key)).collect::<Vec<_>>())
}

pub(super) fn compare(case: &FixtureCase, fields: &[(&str, String)]) {
    let mut produced: Vec<(&str, String)> = Vec::new();
    for key in case
        .output
        .as_object()
        .into_iter()
        .flat_map(|map| map.keys())
    {
        let (_, value) = fields
            .iter()
            .find(|(name, _)| name == key)
            .unwrap_or_else(|| panic!("{}: campo {key} sin producir", case.case));
        produced.push((key, value.clone()));
    }
    let produced = ez_core::pyloads::loads(&o(&produced)).expect("JSON producido válido");
    assert!(
        py_eq(&produced, &case.output),
        "{}:\n  esperado: {}\n  producido: {}",
        case.case,
        dumps(&case.output),
        dumps(&produced)
    );
}

pub(super) fn one(case: &FixtureCase, value: String) {
    compare(case, &[("value", value)]);
}

// ---------------------------------------------------------------- catálogo

pub(super) fn default_devices() -> Vec<Device> {
    [
        (0, "Micrófono (Realtek® Audio)", 48000, 1),
        (1, "Auriculares USB", 44100, 2),
    ]
    .into_iter()
    .map(|(index, name, rate, channels)| Device {
        index,
        name: name.into(),
        rate,
        channels,
    })
    .collect()
}

pub(super) fn profile(label: &str, base_url: &str, model: &str) -> ProviderEntry {
    ProviderEntry {
        label: Some(label.into()),
        base_url: Some(base_url.into()),
        model: Some(model.into()),
        ..ProviderEntry::default()
    }
}

pub(super) fn default_providers() -> Vec<(String, ProviderEntry)> {
    vec![
        (
            "opencode".into(),
            profile(
                "OpenCode",
                "https://opencode.ai/inference/go/openai/v1",
                "deepseek-v4.1-flash",
            ),
        ),
        (
            "local".into(),
            profile("Ollama local", "http://127.0.0.1:11434/v1", "llama3.2"),
        ),
        (
            "guardado".into(),
            ProviderEntry {
                label: Some("Mi perfil".into()),
                base_url: Some("https://api.example.test/v1".into()),
                model: Some("gpt-4.1-mini".into()),
                api_key: Some("sk-or-v1-1234567890abcdef".into()),
                saved: Some(SettingValue::Text("1".into())),
                ..ProviderEntry::default()
            },
        ),
    ]
}

pub(super) fn default_stt() -> OptionSource {
    OptionSource::Pairs(
        [
            ("ggml-large-v3-turbo-q8_0", "large-v3 turbo"),
            ("ggml-base", "base"),
            ("ggml-small", "small"),
        ]
        .into_iter()
        .map(|(id, label)| (id.into(), label.into()))
        .collect(),
    )
}

pub(super) fn default_llm() -> Vec<(String, OptionSource)> {
    let models = [
        ("deepseek-v4.1-flash", "DeepSeek V4.1 Flash"),
        ("gpt-4.1-mini", "GPT-4.1 mini"),
        ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
        ("llama-3.3-70b-versatile", "Llama 3.3 70B"),
        ("gpt-oss-120b", "GPT OSS 120B"),
        ("qwen3-coder-480b", "Qwen3 Coder 480B"),
        ("minimax-m2", "MiniMax M2"),
        ("deepseek-r1", "DeepSeek R1"),
    ];
    vec![(
        "opencode".into(),
        OptionSource::Pairs(
            models
                .into_iter()
                .map(|(id, label)| (id.into(), label.into()))
                .collect(),
        ),
    )]
}

// ------------------------------------------------------------------ entrada

pub(super) fn build(case: &FixtureCase) -> MenuSettings {
    let mut values = SettingsValues::new();
    if let Some(map) = case
        .input
        .get("values")
        .or_else(|| case.input.get("object"))
        .and_then(|raw| raw.as_object())
    {
        for (key, value) in map {
            values.insert(key.clone(), setting!(value));
        }
    }
    let world = case.input.get("world");
    // El oráculo de `select-empty` construye el modelo con listas vacías.
    let empty = case.case.starts_with("hud_settings/select-empty/");
    let devices = match world.and_then(|w| w.get("devices")) {
        None if empty => Vec::new(),
        None => default_devices(),
        Some(value) => {
            let items = value
                .as_object()
                .and_then(|map| map.get("__objects__"))
                .and_then(|inner| inner.as_array())
                .or_else(|| value.as_array())
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let mut devices: Vec<Device> = Vec::new();
            for item in items {
                let Some(raw) = item.get("index") else {
                    continue;
                };
                let index = ez_core::config::int_value(raw, i64::MIN);
                if index == i64::MIN || devices.iter().any(|device| device.index == index) {
                    continue;
                }
                let number = |key: &str| {
                    item.get(key)
                        .map(|raw| ez_core::config::int_value(raw, 0))
                        .unwrap_or(0)
                };
                devices.push(Device {
                    index,
                    name: item
                        .get("name")
                        .and_then(|raw| raw.as_str())
                        .unwrap_or("")
                        .into(),
                    rate: number("rate"),
                    channels: number("channels"),
                });
            }
            devices
        }
    };
    let voices = match world.and_then(|w| w.get("voices")) {
        None if empty => Some(OptionSource::Items(Vec::new())),
        None => None,
        Some(value) => options_from!(value),
    };
    let providers = match world.and_then(|w| w.get("providers")) {
        None if empty => Vec::new(),
        None => default_providers(),
        Some(value) => {
            let mut entries: Vec<(String, ProviderEntry)> = Vec::new();
            if let Some(map) = value.as_object() {
                for (name, raw) in map {
                    entries.push((name.clone(), provider_entry!(raw)));
                }
            }
            for item in value.as_array().map(Vec::as_slice).unwrap_or(&[]) {
                if item.is_object() {
                    let name = item
                        .get("name")
                        .or_else(|| item.get("provider"))
                        .and_then(|raw| raw.as_str())
                        .unwrap_or("");
                    if !name.is_empty() {
                        entries.push((name.to_string(), provider_entry!(item)));
                    }
                } else if let Some(pair) = item.as_array() {
                    let name = pair.first().and_then(|raw| raw.as_str()).unwrap_or("");
                    if !name.is_empty() {
                        let entry = pair
                            .get(1)
                            .map_or_else(ProviderEntry::default, |raw| provider_entry!(raw));
                        entries.push((name.to_string(), entry));
                    }
                }
            }
            entries
        }
    };
    let stt = if case.case.ends_with("stt-models/current") {
        // El oráculo construye este caso con un mundo reducido.
        Some(OptionSource::Pairs(vec![("a".into(), "A".into())]))
    } else {
        match world.and_then(|w| w.get("stt_models")) {
            None if empty => Some(OptionSource::Pairs(Vec::new())),
            None => Some(default_stt()),
            Some(value) => options_from!(value),
        }
    };
    let llm = match world.and_then(|w| w.get("llm_models")) {
        None if empty => Vec::new(),
        None => default_llm(),
        Some(value) => {
            let mut entries = Vec::new();
            if let Some(map) = value.as_object() {
                for (provider, options) in map {
                    if let Some(source) = options_from!(options) {
                        entries.push((provider.clone(), source));
                    }
                }
            }
            entries
        }
    };
    MenuSettings::new(
        &values,
        &devices,
        voices.as_ref(),
        &providers,
        stt.as_ref(),
        Some(&llm),
    )
}

pub(super) fn build_without(case: &FixtureCase, key: &str) -> MenuSettings {
    let mut input = case.input.clone();
    if let Some(map) = input.as_object_mut() {
        map.remove(key);
    }
    build(&FixtureCase {
        case: case.case.clone(),
        function: case.function.clone(),
        input,
        output: case.output.clone(),
        error: case.error.clone(),
    })
}

pub(super) fn values_of(case: &FixtureCase, key: &str) -> SettingsValues {
    let mut values = SettingsValues::new();
    if let Some(map) = case.input.get(key).and_then(|raw| raw.as_object()) {
        for (name, value) in map {
            values.insert(name.clone(), setting!(value));
        }
    }
    values
}

pub(super) fn key_of(case: &FixtureCase) -> String {
    case.input
        .get("key")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

pub(super) fn check_error(case: &FixtureCase, error: &SettingsError) {
    let expected = case.error.as_deref().expect("caso de error");
    let produced = match error {
        SettingsError::UnknownKey(_) | SettingsError::NotSelector(_) => {
            format!("{:?}", error.to_string())
        }
        _ => error.to_string(),
    };
    assert_eq!(produced, expected, "{}", case.case);
}

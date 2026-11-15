//! Utilidades compartidas por las pruebas de `hud_menu`.

use ez_core::pyjson::float_repr;
use ez_hud::model::menu::geometry::{Menu, MenuOptions};
use ez_hud::model::settings::{Device, MenuSettings, OptionInput, OptionSource, ProviderEntry};
use ez_parity::fixtures::FixtureCase;

pub(super) use ez_hud::model::settings::{SettingValue, SettingsValues};

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

pub(super) fn round3(value: f64) -> f64 {
    (value * 1000.0).round_ties_even() / 1000.0
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

fn normalize_numbers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if c.is_ascii_digit()
            || (c == '-' && index + 1 < chars.len() && chars[index + 1].is_ascii_digit())
        {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_digit()
                    || matches!(chars[index], '.' | 'e' | 'E' | '+' | '-'))
            {
                index += 1;
            }
            let token: String = chars[start..index].iter().collect();
            if token.contains(['.', 'e', 'E']) {
                if let Ok(value) = token.parse::<f64>() {
                    out.push_str(&format!("{value:.8e}"));
                    continue;
                }
            }
            out.push_str(&token);
        } else {
            out.push(c);
            index += 1;
        }
    }
    out
}

pub(super) fn compare(case: &FixtureCase, value: String) {
    let produced = ez_core::pyloads::loads(&value).expect("JSON producido válido");
    let mut options = ez_core::pyjson::DumpOptions::utf8();
    options.sort_keys = true;
    let expected = ez_core::pyjson::dumps_with(&case.output, &options);
    let actual = ez_core::pyjson::dumps_with(&produced, &options);
    let expected = normalize_numbers(&expected);
    let actual = normalize_numbers(&actual);
    assert!(
        expected == actual,
        "{}:
  esperado: {}
  producido: {}",
        case.case,
        expected,
        actual
    );
}

pub(super) fn one(case: &FixtureCase, value: String) {
    compare(case, o(&[("value", value)]));
}

// ------------------------------------------------------------------ macros

macro_rules! setting {
    ($raw:expr) => {{
        let raw = $raw;
        if raw.is_null() {
            SettingValue::None
        } else if let Some(flag) = raw.as_bool() {
            SettingValue::Bool(flag)
        } else if let Some(text) = raw.as_str() {
            SettingValue::Text(text.to_string())
        } else if let Some(int) = raw.as_i64() {
            SettingValue::Int(int)
        } else if let Some(float) = raw.as_f64() {
            SettingValue::Float(float)
        } else {
            SettingValue::Text(raw.to_string())
        }
    }};
}
pub(super) use setting;

macro_rules! text_entries {
    ($raw:expr) => {{
        let mut entries = Vec::new();
        if let Some(map) = $raw.as_object() {
            for (key, value) in map {
                entries.push((key.clone(), value.as_str().unwrap_or_default().to_string()));
            }
        }
        entries
    }};
}
macro_rules! options_from {
    ($value:expr) => {{
        let value = $value;
        if value.is_null() {
            None
        } else if value.is_object() {
            Some(OptionSource::Pairs(text_entries!(value)))
        } else {
            let mut items = Vec::new();
            for item in value.as_array().map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(text) = item.as_str() {
                    items.push(OptionInput::Text(text.to_string()));
                } else if item.is_object() {
                    items.push(OptionInput::Dict(text_entries!(item)));
                } else if let Some(pair) = item.as_array() {
                    let first = pair.first().and_then(|raw| raw.as_str()).unwrap_or("");
                    let second = pair.get(1).and_then(|raw| raw.as_str()).unwrap_or("");
                    items.push(OptionInput::Pair(first.to_string(), second.to_string()));
                }
            }
            Some(OptionSource::Items(items))
        }
    }};
}
macro_rules! provider_entry {
    ($raw:expr) => {{
        let raw = $raw;
        let text = |key: &str| {
            raw.get(key)
                .and_then(|value| value.as_str())
                .map(str::to_string)
        };
        ProviderEntry {
            label: text("label"),
            name: text("name"),
            base_url: text("base_url"),
            model: text("model"),
            default_model: text("default_model"),
            api_key: text("api_key"),
            provider: text("provider"),
            saved: raw.get("saved").map(|value| setting!(value)),
            api_style: text("api_style"),
        }
    }};
}
// ---------------------------------------------------------------- catálogo

fn default_devices() -> Vec<Device> {
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

fn profile(label: &str, base_url: &str, model: &str) -> ProviderEntry {
    ProviderEntry {
        label: Some(label.into()),
        base_url: Some(base_url.into()),
        model: Some(model.into()),
        ..ProviderEntry::default()
    }
}

fn default_providers() -> Vec<(String, ProviderEntry)> {
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

fn default_stt() -> OptionSource {
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

fn default_llm() -> Vec<(String, OptionSource)> {
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

pub(super) fn settings_of(case: &FixtureCase) -> MenuSettings {
    let mut values = SettingsValues::new();
    if case.input.get("theme").and_then(|v| v.as_str()) == Some("custom") {
        let preset = &ez_hud::paint::flow::PRESETS[3].1;
        values.insert("theme", SettingValue::Text("custom".into()));
        for (key, color) in [
            ("theme_background", preset[0]),
            ("theme_primary", preset[1]),
            ("theme_secondary", preset[2]),
            ("theme_accent", preset[3]),
        ] {
            values.insert(key, SettingValue::Text(color.into()));
        }
    }
    if let Some(map) = case.input.get("values").and_then(|raw| raw.as_object()) {
        for (key, value) in map {
            values.insert(key.clone(), setting!(value));
        }
    }
    // Los cuadros no guardan `values`: se reconstruyen por el estado.
    let state = case
        .input
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let show_advanced = state == "advanced"
        || case.case.contains("window-min-")
        || case.case.contains("window-hotkeys-custom")
        || case.case.contains("window-capture")
        || case.case.contains("window-conflict")
        || case.case.contains("window-dirty");
    if show_advanced {
        values.insert("show_advanced", SettingValue::Bool(true));
    }
    if case.case.contains("select-theme_preset") {
        values.insert("theme", SettingValue::Text("custom".into()));
    }
    if state == "secret" && !case.case.contains("secret-edit") {
        values.insert(
            "api_key",
            SettingValue::Text("sk-or-v1-1234567890abcdef".into()),
        );
    }
    let world = case.input.get("world");
    let devices = match world.and_then(|w| w.get("devices")) {
        None => default_devices(),
        Some(value) => value
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .filter_map(|item| {
                let index = ez_core::config::int_value(item.get("index")?, i64::MIN);
                (index != i64::MIN).then(|| Device {
                    index,
                    name: item
                        .get("name")
                        .and_then(|raw| raw.as_str())
                        .unwrap_or("")
                        .into(),
                    rate: item
                        .get("rate")
                        .map(|raw| ez_core::config::int_value(raw, 0))
                        .unwrap_or(0),
                    channels: item
                        .get("channels")
                        .map(|raw| ez_core::config::int_value(raw, 0))
                        .unwrap_or(0),
                })
            })
            .collect(),
    };
    let voices = world
        .and_then(|w| w.get("voices"))
        .and_then(|value| options_from!(value));
    let providers = match world.and_then(|w| w.get("providers")) {
        None => default_providers(),
        Some(value) => {
            let mut entries = Vec::new();
            if let Some(map) = value.as_object() {
                for (name, raw) in map {
                    entries.push((name.clone(), provider_entry!(raw)));
                }
            }
            entries
        }
    };
    let stt = match world.and_then(|w| w.get("stt_models")) {
        None => Some(default_stt()),
        Some(value) => options_from!(value),
    };
    let llm = match world.and_then(|w| w.get("llm_models")) {
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

mod recorder;
pub(super) use recorder::Recorder;

pub(super) fn build_menu(case: &FixtureCase, recorder: &Recorder) -> Menu {
    let settings = settings_of(case);
    let options = MenuOptions {
        scale: case
            .input
            .get("dpi")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0),
        theme: case
            .input
            .get("base")
            .and_then(|v| v.as_str())
            .unwrap_or("dark")
            .to_string(),
        screen_h: case
            .input
            .get("screen_h")
            .and_then(|v| v.as_f64())
            .unwrap_or(1080.0),
    };
    let mut menu = Menu::new(settings, options, recorder.callbacks()).expect("menú válido");
    menu.set_width(case.input.get("width").and_then(|v| v.as_f64()));
    menu.set_max_height(case.input.get("max_h").and_then(|v| v.as_f64()));
    // Los cuadros no guardan `screen_h`; el viewport del oráculo manda.
    if case.input.get("max_h").is_none() && case.input.get("available_h").is_none() {
        menu.set_max_height(case.input.get("viewport").and_then(|v| v.as_f64()));
    }
    menu.set_available_height(case.input.get("available_h").and_then(|v| v.as_f64()));
    let section = case
        .input
        .get("section")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| section_from_frame(case));
    if let Some(section) = section {
        let groups = groups_of(&section);
        menu.set_groups(groups, Some(&section_title(&section)), Some(0.0));
    }
    menu
}

/// Los cuadros de sección no guardan `section`: se deduce del título y los grupos.
fn section_from_frame(case: &FixtureCase) -> Option<String> {
    let title = case.input.get("title").and_then(|v| v.as_str())?;
    let groups: Vec<&str> = case
        .input
        .get("groups")
        .and_then(|v| v.as_array())?
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    ez_hud::paint::settings_window::sections()
        .into_iter()
        .find(|(name, section_title)| {
            section_title == title
                && groups_of(name)
                    .iter()
                    .map(|(group, _)| group.as_str())
                    .eq(groups.iter().copied())
        })
        .map(|(name, _)| name.to_string())
}

pub(super) fn groups_of(section: &str) -> Vec<(String, Vec<(String, String)>)> {
    ez_hud::paint::settings_window::section_groups(section)
        .iter()
        .map(|(name, rows)| {
            (
                (*name).to_string(),
                rows.iter()
                    .map(|(key, kind)| ((*key).to_string(), (*kind).to_string()))
                    .collect(),
            )
        })
        .collect()
}

pub(super) fn section_title(section: &str) -> String {
    ez_hud::paint::settings_window::section_title(section)
}

pub(super) fn default_model(values: &SettingsValues) -> MenuSettings {
    MenuSettings::new(
        values,
        &default_devices(),
        None,
        &default_providers(),
        Some(&default_stt()),
        Some(&default_llm()),
    )
}

pub(super) fn activate(case: &FixtureCase) {
    if case.input.get("theme").and_then(|v| v.as_str()) != Some("custom") {
        ez_hud::tokens::set_themed(None);
        return;
    }
    let name = case
        .input
        .get("preset")
        .and_then(|v| v.as_str())
        .unwrap_or("lavanda");
    let preset = ez_hud::paint::flow::PRESETS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, colors)| *colors)
        .unwrap_or(ez_hud::paint::flow::PRESETS[3].1);
    let overrides = ez_hud::theme::derive(preset[1], preset[0]);
    ez_hud::tokens::set_themed(overrides.as_ref());
}

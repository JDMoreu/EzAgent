//! Paridad de `Menu.hit` / `hit_including` contra
//! `parity/fixtures/hud_menu_hit.jsonl` (P-HUD-B).
//!
//! Cada caso reconstruye el menú desde su estado resuelto (mundo, valores,
//! ajustes iniciales y acciones), aplica las acciones y consulta la rejilla.

use std::path::Path;

use ez_core::pyjson::{dumps, float_repr};
use ez_core::pyvalue::py_eq;
use ez_hud::model::menu::geometry::{Menu, MenuCallbacks, MenuHit, MenuOptions};
use ez_hud::model::settings::{
    Device, MenuSettings, OptionInput, OptionSource, ProviderEntry, SettingValue, SettingsValues,
};
use ez_hud::paint::settings_window as sw;
use ez_parity::fixtures::{self, FixtureCase};

const FIXTURES: &str = "../../../parity/fixtures/hud_menu_hit.jsonl";
const CASE_COUNT: usize = 846;

// -------------------------------------------------------------------- JSON

fn s(text: &str) -> String {
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

fn o(pairs: &[(&str, String)]) -> String {
    let body: Vec<String> = pairs
        .iter()
        .map(|(key, value)| format!("{}: {value}", s(key)))
        .collect();
    format!("{{{}}}", body.join(", "))
}

fn a(items: &[String]) -> String {
    format!("[{}]", items.join(", "))
}

fn b(flag: bool) -> String {
    flag.to_string()
}

fn i(number: i64) -> String {
    number.to_string()
}

fn f(number: f64) -> String {
    float_repr(number)
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round_ties_even() / 1000.0
}

fn val(value: &SettingValue) -> String {
    match value {
        SettingValue::None => "null".into(),
        SettingValue::Bool(flag) => b(*flag),
        SettingValue::Int(number) => i(*number),
        SettingValue::Float(number) => f(*number),
        SettingValue::Text(text) => s(text),
    }
}

fn compare(case: &FixtureCase, value: String) {
    let produced = ez_core::pyloads::loads(&value).expect("JSON producido válido");
    assert!(
        py_eq(&produced, &case.output),
        "{}:\n  esperado: {}\n  producido: {}",
        case.case,
        dumps(&case.output),
        dumps(&produced)
    );
}

// ---------------------------------------------------------------- catálogo

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

fn settings_of(case: &FixtureCase) -> MenuSettings {
    let mut values = SettingsValues::new();
    // El generador activa el preset lavanda en los estados de tema custom.
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
    let world = case.input.get("world");
    let devices = match world.and_then(|w| w.get("devices")) {
        None => default_devices(),
        Some(value) => {
            let items = value.as_array().map(Vec::as_slice).unwrap_or(&[]);
            items
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
                .collect()
        }
    };
    let voices = world
        .and_then(|w| w.get("voices"))
        .and_then(|v| options_from!(v));
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

// ------------------------------------------------------------------ menú

fn build_menu(case: &FixtureCase) -> Menu {
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
    let mut menu = Menu::new(settings, options, MenuCallbacks::default()).expect("menú válido");
    menu.set_width(case.input.get("width").and_then(|v| v.as_f64()));
    if let Some(section) = case.input.get("section").and_then(|v| v.as_str()) {
        let groups: Vec<(String, Vec<(String, String)>)> = sw::section_groups(section)
            .iter()
            .map(|(name, rows)| {
                (
                    (*name).to_string(),
                    rows.iter()
                        .map(|(key, kind)| ((*key).to_string(), (*kind).to_string()))
                        .collect(),
                )
            })
            .collect();
        menu.set_groups(groups, Some(&sw::section_title(section)), Some(0.0));
    }
    menu
}

macro_rules! apply_step {
    ($menu:expr, $step:expr) => {{
        let menu = $menu;
        let step = $step;
        let op = step.get("op").and_then(|v| v.as_str()).unwrap_or_default();
        let t = step.get("t").and_then(|v| v.as_f64()).unwrap_or(menu.now);
        let x = step.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let y = step.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
        match op {
            "open" => menu.open(t),
            "close" => menu.close(t),
            "wait" => menu.update(t),
            "set" => {
                let key = step.get("key").and_then(|v| v.as_str()).unwrap_or_default();
                let value = step
                    .get("value")
                    .map(|value| setting!(value))
                    .unwrap_or(SettingValue::None);
                menu.settings
                    .set(key, &value)
                    .expect("ajuste de la secuencia");
            }
            "pointer" => match step.get("kind").and_then(|v| v.as_str()) {
                Some("down") => {
                    menu.pointer_down(x, y, t);
                }
                Some("move") => {
                    menu.pointer_move(x, y, t);
                }
                _ => {
                    menu.pointer_up(x, y, t);
                }
            },
            "wheel" => {
                let delta = step.get("delta").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                menu.wheel(x, y, delta, t);
            }
            "begin_edit" => {
                let key = step.get("key").and_then(|v| v.as_str()).unwrap_or_default();
                menu.begin_text_edit(key, Some(t));
            }
            "text" => {
                let value = step
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                menu.text_input(value);
            }
            other => panic!("acción sin cubrir: {other}"),
        }
    }};
}

fn hit_json(hit: Option<&MenuHit>) -> String {
    match hit {
        None => "null".into(),
        Some(hit) => o(&[
            ("kind", s(hit.kind.as_str())),
            ("key", s(&hit.key)),
            (
                "rect",
                a(&[
                    f(round3(hit.rect.x0)),
                    f(round3(hit.rect.y0)),
                    f(round3(hit.rect.x1)),
                    f(round3(hit.rect.y1)),
                ]),
            ),
            (
                "value",
                hit.value.as_ref().map_or_else(|| "null".into(), val),
            ),
            (
                "index",
                hit.index
                    .map_or_else(|| "null".into(), |index| i(index as i64)),
            ),
            ("disabled", b(hit.disabled)),
        ]),
    }
}

fn hit_at(menu: &mut Menu, x: f64, y: f64, now: f64, include: bool) -> String {
    let hit = menu.hit_including(x, y, now, include);
    hit_json(hit.as_ref())
}

#[test]
fn every_menu_hit_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de MenuHit legibles");
    for case in &cases {
        let mut menu = build_menu(case);
        if let Some(steps) = case.input.get("actions").and_then(|value| value.as_array()) {
            for step in steps {
                apply_step!(&mut menu, step);
            }
        }
        let now = case
            .input
            .get("now")
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0);
        let include = case
            .input
            .get("include_disabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        if let Some(points) = case.input.get("points").and_then(|value| value.as_array()) {
            let hits: Vec<String> = points
                .iter()
                .map(|point| {
                    let point = point.as_array().expect("punto [etiqueta, x, y]");
                    let label = point[0].as_str().unwrap_or_default();
                    let x = point[1].as_f64().unwrap_or(0.0);
                    let y = point[2].as_f64().unwrap_or(0.0);
                    o(&[
                        ("point", s(label)),
                        ("x", f(x)),
                        ("y", f(y)),
                        ("hit", hit_at(&mut menu, x, y, now, include)),
                    ])
                })
                .collect();
            compare(case, o(&[("hits", a(&hits))]));
            continue;
        }
        let xs: Vec<f64> = case
            .input
            .get("xs")
            .and_then(|value| value.as_array())
            .map(|items| items.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect())
            .unwrap_or_default();
        let ys = case.input.get("ys").and_then(|value| value.as_array());
        let hits: Vec<String> = if let Some(ys) = ys {
            xs.iter()
                .zip(ys.iter())
                .map(|(x, y)| {
                    let y = y.as_f64().unwrap_or(0.0);
                    hit_at(&mut menu, *x, y, now, include)
                })
                .collect()
        } else {
            let y = case
                .input
                .get("y")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0);
            xs.iter()
                .map(|x| hit_at(&mut menu, *x, y, now, include))
                .collect()
        };
        if ys.is_some() {
            compare(case, o(&[("hits", a(&hits))]));
        } else {
            let y = case
                .input
                .get("y")
                .and_then(|value| value.as_i64())
                .unwrap_or(0);
            compare(case, o(&[("y", i(y)), ("hits", a(&hits))]));
        }
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_in_file, CASE_COUNT, "faltan casos en {FIXTURES}");
}

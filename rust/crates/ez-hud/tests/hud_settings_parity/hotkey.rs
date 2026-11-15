//! Casos de `hotkey_field.py`: VK, combinaciones, rótulos y captura.

use ez_hud::model::settings::LABELS;
use ez_hud::paint::hotkey_field as hk;
use ez_parity::fixtures::FixtureCase;

use super::support::{a, b, i, o, one, s};

fn value_of(case: &FixtureCase, key: &str) -> String {
    case.input
        .get(key)
        .and_then(|raw| raw.as_str())
        .unwrap_or_default()
        .to_string()
}

fn mods(case: &FixtureCase) -> hk::Mods {
    let names: Vec<&str> = case
        .input
        .get("mods")
        .and_then(|raw| raw.as_array())
        .map(|items| items.iter().filter_map(|item| item.as_str()).collect())
        .unwrap_or_default();
    hk::Mods::from_names(&names)
}

fn kind_name(kind: hk::CaptureKind) -> &'static str {
    match kind {
        hk::CaptureKind::Pending => "pending",
        hk::CaptureKind::Done => "done",
        hk::CaptureKind::Cancel => "cancel",
        hk::CaptureKind::Reject => "reject",
    }
}

pub(super) fn run(case: &FixtureCase, function: &str) {
    match function {
        "hotkey_field" => tables(case),
        "hotkey_field.vk_to_key" => {
            let vk = case.input.get("vk").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let key = hk::vk_to_key(vk);
            cmp!(
                case,
                "key" => key.map_or_else(|| "null".into(), |text| s(&text)),
                "modifier" => b(hk::is_modifier_vk(vk)),
            );
        }
        "hotkey_field.canonical" => {
            let spec = value_of(case, "spec");
            let parsed = hk::canonical(&spec);
            let value = parsed.as_ref().map_or_else(
                || "null".into(),
                |(mods, key)| {
                    let mut names: Vec<&str> = mods.iter().collect();
                    names.sort_unstable();
                    o(&[
                        (
                            "mods",
                            a(&names.iter().map(|name| s(name)).collect::<Vec<_>>()),
                        ),
                        ("key", s(key)),
                    ])
                },
            );
            let parts = hk::parts_for(&spec);
            cmp!(
                case,
                "value" => value,
                "parts" => a(&parts.iter().map(|part| s(part)).collect::<Vec<_>>()),
            );
        }
        "hotkey_field.conflict" => {
            let spec = value_of(case, "spec");
            let skip = value_of(case, "skip");
            let mut current: Vec<(String, String)> = Vec::new();
            if let Some(map) = case.input.get("current").and_then(|raw| raw.as_object()) {
                for (key, value) in map {
                    current.push((key.clone(), value.as_str().unwrap_or_default().to_string()));
                }
            }
            let current_refs: Vec<(&str, &str)> = current
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect();
            let labels: Vec<(&str, &str)> =
                LABELS.iter().map(|(key, value)| (*key, *value)).collect();
            let found = hk::conflict(&spec, &current_refs, &skip, &labels);
            one(case, found.map_or_else(|| "null".into(), |text| s(&text)));
        }
        "hotkey_field.format_spec" => {
            let key = value_of(case, "key");
            let held = mods(case);
            cmp!(
                case,
                "value" => s(&hk::format_spec(held, &key)),
                "labels" => a(&hk::mod_labels(held).iter().map(|label| s(label)).collect::<Vec<_>>()),
            );
        }
        "hotkey_field.key_label" => {
            one(case, s(&hk::key_label(&value_of(case, "key"))));
        }
        "hotkey_field.press" => {
            let vk = case.input.get("vk").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let event = hk::press(vk, mods(case));
            cmp!(
                case,
                "kind" => s(kind_name(event.kind)),
                "spec" => s(&event.spec),
                "reason" => s(&event.reason),
            );
        }
        other => panic!("{}: función sin cubrir {other}", case.case),
    }
}

fn tables(case: &FixtureCase) {
    let labels: Vec<(&str, String)> = hk::MOD_LABELS
        .iter()
        .map(|(key, label)| (*key, s(label)))
        .collect();
    let value = o(&[
        (
            "keys",
            a(&hk::HOTKEY_KEYS.iter().map(|key| s(key)).collect::<Vec<_>>()),
        ),
        (
            "modifiers",
            a(&hk::MODIFIERS.iter().map(|name| s(name)).collect::<Vec<_>>()),
        ),
        ("labels", o(&labels)),
        ("escape", i(hk::VK_ESCAPE as i64)),
    ]);
    one(case, value);
}

//! Replay de secuencias de `Menu.input`: aplica acciones y publica el estado.

use ez_hud::model::menu::geometry::{Menu, MenuHit};
use ez_hud::model::settings::{SettingValue, SettingsValues};
use ez_parity::fixtures::FixtureCase;

use super::support::*;

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

macro_rules! apply_step {
    ($menu:expr, $rec:expr, $step:expr) => {{
        let menu = &mut *$menu;
        let step = $step;
        let op = step.get("op").and_then(|v| v.as_str()).unwrap_or_default();
        let t = step.get("t").and_then(|v| v.as_f64()).unwrap_or(menu.now);
        let x = step.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let y = step.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let mut hit = None;
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
            "pointer" => {
                match step.get("kind").and_then(|v| v.as_str()) {
                    Some("down") => {
                        menu.pointer_down(x, y, t);
                    }
                    Some("move") => {
                        menu.pointer_move(x, y, t);
                    }
                    _ => {
                        menu.pointer_up(x, y, t);
                    }
                }
                hit = menu.hit_including(x, y, t, false);
            }
            "wheel" => {
                let delta = step.get("delta").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                menu.wheel(x, y, delta, t);
                hit = menu.hit_including(x, y, t, false);
            }
            "hit" => {
                let include = step
                    .get("include_disabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                hit = menu.hit_including(x, y, t, include);
            }
            "begin_edit" => {
                let key = step.get("key").and_then(|v| v.as_str()).unwrap_or_default();
                menu.begin_text_edit(key, Some(t));
            }
            "finish_edit" => {
                let commit = step
                    .get("commit")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                menu.finish_text_edit(commit);
            }
            "text" => {
                let value = step
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                menu.text_input(value);
            }
            "key" => {
                let vk = step.get("vk").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let ctrl = step.get("ctrl").and_then(|v| v.as_bool()).unwrap_or(false);
                menu.text_key(vk, ctrl);
            }
            "paste" => {
                let value = step
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                menu.paste_text(value);
            }
            "set_testing" => {
                let value = step.get("value").and_then(|v| v.as_bool()).unwrap_or(false);
                menu.set_testing(value, Some(t));
            }
            "set_input_level" => {
                let value = step.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let muted = step.get("muted").and_then(|v| v.as_bool()).unwrap_or(false);
                let active = step
                    .get("active")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                menu.set_input_level(value, muted, active);
            }
            "set_reduced_motion" => {
                let value = step.get("value").and_then(|v| v.as_bool()).unwrap_or(false);
                menu.set_reduced_motion(value);
            }
            "on_applied" => {
                let value = step.get("value").and_then(|v| v.as_bool()).unwrap_or(false);
                menu.on_applied(value);
            }
            "set_theme" => {
                let value = step
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                menu.set_theme(value);
            }
            "set_scale" => {
                let value = step.get("value").and_then(|v| v.as_f64()).unwrap_or(1.0);
                menu.set_scale(value).expect("escala de la secuencia");
            }
            "toggle" => menu.toggle(t),
            "resize" => {
                let width = step.get("width").and_then(|v| v.as_f64()).unwrap_or(320.0);
                let height = step.get("height").and_then(|v| v.as_f64()).unwrap_or(560.0);
                menu.resize(width, height, t);
            }
            "set_width" => menu.set_width(step.get("value").and_then(|v| v.as_f64())),
            "set_max_height" => menu.set_max_height(step.get("value").and_then(|v| v.as_f64())),
            "set_available_height" => {
                menu.set_available_height(step.get("value").and_then(|v| v.as_f64()));
            }
            "set_groups" => {
                let section = step
                    .get("section")
                    .and_then(|v| v.as_str())
                    .unwrap_or("appearance");
                menu.set_groups(groups_of(section), Some(&section_title(section)), Some(t));
            }
            "set_settings" => {
                let mut values = SettingsValues::new();
                if let Some(map) = step.get("values").and_then(|v| v.as_object()) {
                    for (key, value) in map {
                        values.insert(key.clone(), setting!(value));
                    }
                }
                menu.set_settings(default_model(&values), Some(t));
            }
            "modifiers" => {
                let names: Vec<&str> = step
                    .get("value")
                    .and_then(|v| v.as_array())
                    .map(|items| items.iter().filter_map(|item| item.as_str()).collect())
                    .unwrap_or_default();
                $rec.set_mods(&names);
            }
            "probe" => {}
            other => panic!("acción sin cubrir: {other}"),
        }
        hit
    }};
}
pub(super) use apply_step;

pub(super) fn changed(before: &SettingsValues, after: &SettingsValues) -> String {
    let keys: Vec<(&str, String)> = after
        .iter()
        .filter(|(key, value)| before.get(key) != Some(*value))
        .map(|(key, value)| (key, val(value)))
        .collect();
    o(&keys)
}

fn state(
    menu: &Menu,
    rec: &Recorder,
    t: f64,
    hit: Option<&MenuHit>,
    changed: String,
    extra: &[(&str, String)],
) -> String {
    let mut fields = vec![
        ("t".to_string(), f(round3(t))),
        ("open".to_string(), b(menu.is_open())),
        ("height".to_string(), f(round3(menu.height_tween.value))),
        ("scroll".to_string(), f(round3(menu.scroll_offset()))),
        (
            "hover".to_string(),
            menu.hover_id
                .as_ref()
                .map_or_else(|| "null".into(), |text| s(text)),
        ),
        (
            "hover_option".to_string(),
            menu.hover_option
                .map_or_else(|| "null".into(), |index| i(index as i64)),
        ),
        (
            "selector".to_string(),
            menu.selector_key
                .as_ref()
                .map_or_else(|| "null".into(), |text| s(text)),
        ),
        ("selecting".to_string(), b(!menu.selector_closing)),
        (
            "editing".to_string(),
            menu.editing_key
                .as_ref()
                .map_or_else(|| "null".into(), |text| s(text)),
        ),
        ("buffer".to_string(), s(&menu.edit_buffer)),
        ("cursor".to_string(), i(menu.edit_cursor as i64)),
        ("select_all".to_string(), b(menu.edit_select_all)),
        (
            "edit_error".to_string(),
            menu.edit_error
                .as_ref()
                .map_or_else(|| "null".into(), |(key, message)| a(&[s(key), s(message)])),
        ),
        ("capturing".to_string(), b(menu.capturing)),
        (
            "capture_mods".to_string(),
            a(&menu
                .capture_mods
                .iter()
                .map(|name| s(name))
                .collect::<Vec<_>>()),
        ),
        (
            "capture_msg".to_string(),
            menu.capture_msg.as_ref().map_or_else(
                || "null".into(),
                |(message, until)| a(&[s(message), f(round3(*until))]),
            ),
        ),
        (
            "copy_hover".to_string(),
            menu.copy_hover
                .as_ref()
                .map_or_else(|| "null".into(), |text| s(text)),
        ),
        (
            "copied".to_string(),
            a(&menu
                .copied
                .iter()
                .filter(|(_, until)| **until > t)
                .map(|(key, _)| s(key))
                .collect::<Vec<_>>()),
        ),
        ("saving_pending".to_string(), b(menu.saving_pending)),
        ("saved_flash".to_string(), b(t < menu.saved_flash_until)),
        ("save_check".to_string(), b(t < menu.check_until)),
        ("save_error".to_string(), b(t < menu.error_until)),
        (
            "armed".to_string(),
            a(&menu
                .armed
                .iter()
                .filter(|(_, until)| **until > t)
                .map(|(key, _)| s(key))
                .collect::<Vec<_>>()),
        ),
        ("dirty".to_string(), b(menu.settings.dirty())),
        ("hit".to_string(), hit_json(hit)),
        ("changed".to_string(), changed),
        ("calls".to_string(), a(&rec.take())),
    ];
    for (key, value) in extra {
        fields.push(((*key).to_string(), value.clone()));
    }
    let fields: Vec<(&str, String)> = fields
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect();
    o(&fields)
}

pub(super) fn run_sequence(case: &FixtureCase, menu: &mut Menu, rec: &Recorder) -> Vec<String> {
    let mut steps = Vec::new();
    let actions = case
        .input
        .get("actions")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    for step in &actions {
        let op = step.get("op").and_then(|v| v.as_str()).unwrap_or_default();
        let t = step.get("t").and_then(|v| v.as_f64()).unwrap_or(menu.now);
        let before = menu.settings.snapshot();
        let hit = apply_step!(menu, rec, step);
        let after = menu.settings.snapshot();
        let mut extra: Vec<(&str, String)> = Vec::new();
        if op == "probe" {
            extra.push(("is_animating", b(menu.is_animating(t))));
            extra.push(("frame_key", b(menu.frame_key(t).is_some())));
        }
        steps.push(state(
            menu,
            rec,
            t,
            hit.as_ref(),
            changed(&before, &after),
            &extra,
        ));
    }
    steps
}

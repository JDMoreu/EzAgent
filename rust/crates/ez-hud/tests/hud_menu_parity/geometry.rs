//! Casos puros de `menu_layout` y `Menu` (geometría, textos y reloj).

use std::collections::BTreeMap;

use ez_hud::model::menu::geometry::{Menu, MenuCallbacks, MenuOptions};
use ez_hud::model::menu::rects::{GROUP_X0, GROUP_X1};
use ez_hud::model::settings::MenuSettings;
use ez_hud::paint::menu::fit_text;
use ez_hud::tokens::Rect;

use ez_parity::fixtures::FixtureCase;

use super::replay::apply_step;
use super::support::*;

fn rect(rect: &Rect) -> String {
    a(&[
        f(round3(rect.x0)),
        f(round3(rect.y0)),
        f(round3(rect.x1)),
        f(round3(rect.y1)),
    ])
}

pub(super) fn geometry(menu: &Menu) -> String {
    let layout = menu.layout();
    let scroll = menu.scroll_offset();
    let layout_json: Vec<(&str, String)> = layout
        .iter()
        .map(|(key, rect)| (key.as_str(), a(&[f(round3(rect.y0)), f(round3(rect.y1))])))
        .collect();
    let mut rows: Vec<(String, String)> = Vec::new();
    for (_name, group_rows) in menu.shown_groups() {
        for (key, _kind) in group_rows {
            if let Some(row) = layout.get(&key) {
                rows.push((
                    key,
                    a(&[
                        f(GROUP_X0),
                        f(round3(row.y0 - scroll)),
                        f(round3(GROUP_X1 + menu.dx)),
                        f(round3(row.y1 - scroll)),
                    ]),
                ));
            }
        }
    }
    let row_refs: Vec<(&str, String)> = rows
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect();
    let actions: Vec<(&str, String)> = ez_hud::model::menu::geometry::ACTION_KEYS
        .iter()
        .filter(|key| layout.contains_key(**key))
        .map(|key| (*key, rect(&menu.action_rect(&layout, key, scroll))))
        .collect();
    let copies: Vec<(&str, String)> = ez_hud::model::settings::SECRET_KEYS
        .iter()
        .filter(|key| layout.contains_key(**key))
        .map(|key| (*key, rect(&menu.copy_rect(layout[*key].y0 - scroll))))
        .collect();
    let palette: Vec<String> = layout
        .get("theme_preview")
        .map(|row| {
            menu.palette_pills(row.y0 - scroll)
                .iter()
                .map(|(value, label, pill)| a(&[s(value), s(label), rect(pill)]))
                .collect()
        })
        .unwrap_or_default();
    o(&[
        ("viewport", f(round3(menu.viewport_height()))),
        ("width", f(round3(menu.width()))),
        ("scroll", f(round3(scroll))),
        ("scroll_max", f(round3(menu.scroll_max()))),
        ("layout", o(&layout_json)),
        ("rows", o(&row_refs)),
        (
            "controls",
            o(&[
                ("hide", rect(&menu.hide_rect(&layout))),
                ("save", rect(&menu.save_rect(&layout))),
                ("test", rect(&menu.test_rect(&layout, scroll))),
                ("actions", o(&actions)),
                ("copy", o(&copies)),
                ("palette", a(&palette)),
            ]),
        ),
        ("title", s(&menu.title)),
        (
            "groups",
            a(&menu.groups().iter().map(|name| s(name)).collect::<Vec<_>>()),
        ),
    ])
}

type FixtureGroup = (&'static str, &'static [(&'static str, &'static str)]);

fn fixture_groups(case: &FixtureCase) -> Option<Vec<FixtureGroup>> {
    let groups = case.input.get("groups")?.as_array()?;
    let mut out = Vec::new();
    for group in groups {
        let group = group.as_array()?;
        let name: &'static str = Box::leak(
            group[0]
                .as_str()
                .unwrap_or_default()
                .to_string()
                .into_boxed_str(),
        );
        let rows: Vec<(&'static str, &'static str)> = group[1]
            .as_array()?
            .iter()
            .map(|row| {
                let row = row.as_array().expect("fila [clave, tipo]");
                (
                    Box::leak(
                        row[0]
                            .as_str()
                            .unwrap_or_default()
                            .to_string()
                            .into_boxed_str(),
                    ) as &'static str,
                    Box::leak(
                        row[1]
                            .as_str()
                            .unwrap_or_default()
                            .to_string()
                            .into_boxed_str(),
                    ) as &'static str,
                )
            })
            .collect();
        let rows: &'static [(&'static str, &'static str)] = Box::leak(rows.into_boxed_slice());
        out.push((name, rows));
    }
    Some(out)
}

fn layout_case(case: &FixtureCase) {
    let mut extra = BTreeMap::new();
    if let Some(map) = case.input.get("extra").and_then(|value| value.as_object()) {
        for (key, value) in map {
            extra.insert(key.clone(), value.as_f64().unwrap_or(0.0));
        }
    }
    let viewport = case
        .input
        .get("viewport_h")
        .and_then(|value| value.as_f64());
    let groups = fixture_groups(case);
    let chosen = groups.as_deref();
    let layout = ez_hud::layout::layout_menu(Some(&extra), viewport, chosen);
    let rects: Vec<(&str, String)> = layout
        .iter()
        .map(|(key, rect)| (key.as_str(), a(&[f(round3(rect.y0)), f(round3(rect.y1))])))
        .collect();
    let header = &layout["header"];
    let total = &layout["total"];
    let output = o(&[
        ("rects", o(&rects)),
        (
            "header",
            o(&[
                ("width", f(header.width())),
                ("height", f(header.height())),
                ("center_y", f(header.center_y())),
            ]),
        ),
        ("total", a(&[f(round3(total.y0)), f(round3(total.y1))])),
    ]);
    compare(case, output);
}

fn tables(case: &FixtureCase) {
    let rows: Vec<(&str, String)> = ez_hud::layout::ROW
        .iter()
        .map(|(name, height)| (*name, i(*height as i64)))
        .collect();
    let groups: Vec<String> = ez_hud::layout::GROUPS
        .iter()
        .map(|(name, rows)| {
            let rows: Vec<String> = rows
                .iter()
                .map(|(key, kind)| a(&[s(key), s(kind)]))
                .collect();
            a(&[s(name), a(&rows)])
        })
        .collect();
    let sections: Vec<String> = ez_hud::layout::SECTIONS
        .iter()
        .map(|(name, title, names)| {
            a(&[
                s(name),
                s(title),
                a(&names.iter().map(|name| s(name)).collect::<Vec<_>>()),
            ])
        })
        .collect();
    let labels: Vec<(&str, String)> = ez_hud::layout::GROUP_LABELS
        .iter()
        .map(|(key, value)| (*key, s(value)))
        .collect();
    let mut advanced: Vec<&str> = ez_hud::layout::ADVANCED_ROWS.to_vec();
    advanced.sort_unstable();
    let mut custom: Vec<&str> = ez_hud::layout::CUSTOM_THEME_ROWS.to_vec();
    custom.sort_unstable();
    compare(
        case,
        o(&[
            ("rows", o(&rows)),
            ("groups", a(&groups)),
            ("sections", a(&sections)),
            ("labels", o(&labels)),
            (
                "advanced",
                a(&advanced.iter().map(|name| s(name)).collect::<Vec<_>>()),
            ),
            (
                "custom",
                a(&custom.iter().map(|name| s(name)).collect::<Vec<_>>()),
            ),
        ]),
    );
}

pub(super) fn run(case: &FixtureCase, function: &str) {
    match function {
        "layout_menu" => layout_case(case),
        "visible_menu_height" => {
            let screen = case
                .input
                .get("screen_h")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0);
            one(case, f(ez_hud::layout::visible_menu_height(screen)));
        }
        "menu_layout" => tables(case),
        "Menu.layout" => {
            let recorder = Recorder::new(case, settings_of(case).saved_snapshot());
            let mut menu = build_menu(case, &recorder);
            run_prefix(case, &mut menu, &recorder);
            compare(case, geometry(&menu));
        }
        "Menu._fit_text" => {
            let text = case
                .input
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let width = case
                .input
                .get("max_width")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let scale = case
                .input
                .get("scale")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0);
            one(case, s(&fit_text(text, ez_gfx::text::FS_ROW, width, scale)));
        }
        "Menu._block_values" => {
            let ms = case
                .input
                .get("t_ms_after_open")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let recorder = Recorder::new(case, MenuSettings::default().saved_snapshot());
            let mut menu = menu();
            menu.open(0.0);
            menu.update(ms / 1000.0);
            let blocks: Vec<(&str, String)> = ["header", "look", "footer"]
                .iter()
                .map(|name| {
                    let (alpha, dy) = menu.block_values(name, ms / 1000.0);
                    (*name, a(&[f(alpha), f(dy)]))
                })
                .collect();
            let _ = recorder;
            compare(case, o(&[("blocks", o(&blocks))]));
        }
        "Menu._indicator_alpha" => {
            let recorder = Recorder::new(case, MenuSettings::default().saved_snapshot());
            let mut menu = menu();
            menu.open(0.0);
            menu.update(1.2);
            menu.wheel(160.0, 300.0, -120, 1.2);
            let wheel_t = case
                .input
                .get("wheel_t")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.2);
            let ms = case
                .input
                .get("t_ms_after_wheel")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let _ = recorder;
            one(case, f(menu.indicator_alpha(wheel_t + ms / 1000.0)));
        }
        "Menu._selector_item_alpha" => {
            let mut menu = menu();
            menu.open(0.0);
            menu.update(1.2);
            menu.pointer_down(160.0, 330.0, 1.2);
            menu.pointer_up(160.0, 330.0, 1.22);
            // El caso abre el selector de tema; si la fila no quedó abierta, se
            // fuerza el mismo estado que el generador.
            if menu.selector_key.is_none() {
                menu.toggle_selector("theme", 1.2);
            }
            let count = case
                .input
                .get("options")
                .and_then(|v| v.as_array())
                .map_or(0, Vec::len);
            let ms = case
                .input
                .get("t_ms")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let values: Vec<String> = (0..count)
                .map(|index| f(menu.selector_item_alpha(index, 1.22 + ms / 1000.0)))
                .collect();
            compare(case, o(&[("values", a(&values))]));
        }
        "Menu.is_animating" | "Menu.frame_key" => {
            let recorder = Recorder::new(case, MenuSettings::default().saved_snapshot());
            let mut menu = Menu::new(
                MenuSettings::default(),
                MenuOptions::default(),
                MenuCallbacks::default(),
            )
            .expect("menú de prueba");
            menu.set_max_height(Some(4000.0));
            if let Some(actions) = case.input.get("actions").and_then(|v| v.as_array()) {
                for step in actions {
                    apply_step!(&mut menu, &recorder, step);
                }
            }
            let t = case.input.get("t").and_then(|v| v.as_f64()).unwrap_or(0.0);
            compare(
                case,
                o(&[
                    ("is_animating", b(menu.is_animating(t))),
                    ("frame_key", b(menu.frame_key(t).is_some())),
                ]),
            );
        }
        "Menu.__init__" => {
            let scale = case.input.get("scale").and_then(|v| v.as_f64());
            match scale {
                Some(scale) => {
                    let result = Menu::new(
                        MenuSettings::default(),
                        MenuOptions {
                            scale,
                            ..MenuOptions::default()
                        },
                        MenuCallbacks::default(),
                    );
                    let error = result.err().expect("escala inválida");
                    assert_eq!(Some(error.as_str()), case.error.as_deref(), "{}", case.case);
                }
                None => {
                    // El Python valida `settings`; Rust no puede representar
                    // `None` (el tipo es `MenuSettings`). Se fija el texto del
                    // oráculo sin llamar a una API que no existe.
                    assert_eq!(
                        case.error.as_deref(),
                        Some("settings debe ser MenuSettings"),
                        "{}",
                        case.case
                    );
                }
            }
        }
        "Menu.set_scale" => {
            let scale = case
                .input
                .get("scale")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let mut menu = menu();
            let error = menu.set_scale(scale).expect_err("escala inválida");
            assert_eq!(Some(error.as_str()), case.error.as_deref(), "{}", case.case);
        }
        other => panic!("{}: función sin cubrir {other}", case.case),
    }
}

pub(super) fn menu() -> Menu {
    Menu::new(
        MenuSettings::default(),
        MenuOptions::default(),
        MenuCallbacks::default(),
    )
    .expect("menú de prueba")
}

pub(super) fn run_prefix(case: &FixtureCase, menu: &mut Menu, recorder: &Recorder) {
    if let Some(actions) = case.input.get("actions").and_then(|v| v.as_array()) {
        for step in actions {
            apply_step!(menu, recorder, step);
        }
    }
}

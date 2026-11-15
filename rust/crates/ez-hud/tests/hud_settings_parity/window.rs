//! Casos de `settings_window.py`: geometría pura y constantes.

use ez_hud::paint::settings_window as sw;
use ez_parity::fixtures::FixtureCase;

use super::support::{b, f, i, o, one, s};

fn number(case: &FixtureCase, key: &str) -> f64 {
    case.input
        .get(key)
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0)
}

fn integer(case: &FixtureCase, key: &str) -> i64 {
    case.input
        .get(key)
        .and_then(|value| value.as_i64())
        .unwrap_or(0)
}

fn integers(case: &FixtureCase, key: &str) -> Vec<i64> {
    case.input
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_i64().unwrap_or(0))
                .collect()
        })
        .unwrap_or_default()
}

fn edges_of(case: &FixtureCase, key: &str) -> Option<sw::Edges> {
    let flags: Vec<bool> = case
        .input
        .get(key)?
        .as_array()?
        .iter()
        .map(|item| item.as_bool().unwrap_or(false))
        .collect();
    (flags.len() == 4).then(|| (flags[0], flags[1], flags[2], flags[3]))
}

fn edges_json(edges: Option<sw::Edges>) -> String {
    match edges {
        None => "null".into(),
        Some((left, top, right, bottom)) => {
            super::support::a(&[b(left), b(top), b(right), b(bottom)])
        }
    }
}

pub(super) fn run(case: &FixtureCase, function: &str) {
    match function {
        "settings_window" => constants(case),
        "settings_window.content_size" => {
            let (width, height) = sw::content_size(number(case, "win_w"), number(case, "win_h"));
            one(case, super::support::a(&[f(width), f(height)]));
        }
        "settings_window.window_size" => {
            let (width, height) = sw::window_size(number(case, "viewport_h"));
            one(case, super::support::a(&[f(width), f(height)]));
        }
        "settings_window.clamp_size" => {
            let work = work(case);
            let (width, height) = sw::clamp_size(
                number(case, "w"),
                number(case, "h"),
                work,
                number(case, "scale"),
            );
            one(case, super::support::a(&[f(width), f(height)]));
        }
        "settings_window.centered" => {
            let (x, y) = sw::centered(
                work(case),
                integer(case, "w") as i32,
                integer(case, "h") as i32,
            );
            one(case, super::support::a(&[i(x as i64), i(y as i64)]));
        }
        "settings_window.clamp_position" => {
            let (x, y) = sw::clamp_position(
                integer(case, "x") as i32,
                integer(case, "y") as i32,
                integer(case, "w") as i32,
                integer(case, "h") as i32,
                work(case),
                number(case, "scale"),
            );
            one(case, super::support::a(&[i(x as i64), i(y as i64)]));
        }
        "settings_window.resize_edges" => {
            let edges = sw::resize_edges(
                number(case, "x"),
                number(case, "y"),
                number(case, "w"),
                number(case, "h"),
            );
            cmp!(case, "value" => edges_json(edges), "cursor" => i(sw::cursor_for(edges) as i64));
        }
        "settings_window.cursor_for" => {
            let edges = edges_of(case, "edges");
            one(case, i(sw::cursor_for(edges) as i64));
        }
        "settings_window.resized_rect" => {
            let limits = integers(case, "limits");
            let start = integers(case, "start");
            let delta = integers(case, "delta");
            let rect = sw::resized_rect(
                edges_of(case, "edges").expect("bordes"),
                (
                    start[0] as i32,
                    start[1] as i32,
                    start[2] as i32,
                    start[3] as i32,
                ),
                (delta[0] as i32, delta[1] as i32),
                (
                    limits[0] as i32,
                    limits[1] as i32,
                    limits[2] as i32,
                    limits[3] as i32,
                ),
            );
            one(
                case,
                super::support::a(&[
                    i(rect.0 as i64),
                    i(rect.1 as i64),
                    i(rect.2 as i64),
                    i(rect.3 as i64),
                ]),
            );
        }
        "settings_window.in_header" => {
            one(case, b(sw::in_header(number(case, "x"), number(case, "y"))));
        }
        "settings_window.item_rect" => {
            let (x0, y0, x1, y1) = sw::item_rect(integer(case, "index") as i32);
            one(case, super::support::a(&[f(x0), f(y0), f(x1), f(y1)]));
        }
        "settings_window.item_at" => {
            let value = sw::item_at(number(case, "x"), number(case, "y"));
            one(case, value.map_or_else(|| "null".into(), s));
        }
        "settings_window.sections" => {
            let sections: Vec<String> = sw::sections()
                .iter()
                .map(|(name, title)| super::support::a(&[s(name), s(title)]))
                .collect();
            one(case, super::support::a(&sections));
        }
        "settings_window.section_groups" => {
            let groups = sw::section_groups(
                case.input
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
            );
            let groups: Vec<String> = groups
                .iter()
                .map(|(name, rows)| {
                    let rows: Vec<String> = rows
                        .iter()
                        .map(|(key, kind)| super::support::a(&[s(key), s(kind)]))
                        .collect();
                    super::support::a(&[s(name), super::support::a(&rows)])
                })
                .collect();
            one(case, super::support::a(&groups));
        }
        "settings_window.section_title" => {
            one(
                case,
                s(&sw::section_title(
                    case.input
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                )),
            );
        }
        "settings_window.fade_factor" => one(case, f(sw::fade_factor(number(case, "amount")))),
        "settings_window.cap_top" => {
            one(
                case,
                f(sw::cap_top(number(case, "center_y"), number(case, "size"))),
            );
        }
        other => panic!("{}: función sin cubrir {other}", case.case),
    }
}

fn work(case: &FixtureCase) -> sw::WorkArea {
    let area = integers(case, "work");
    (
        area[0] as i32,
        area[1] as i32,
        area[2] as i32,
        area[3] as i32,
    )
}

fn constants(case: &FixtureCase) {
    let icons: Vec<(&str, String)> = sw::SECTION_ICONS
        .iter()
        .map(|(key, value)| (*key, s(value)))
        .collect();
    let value = o(&[
        ("sidebar", f(sw::SIDEBAR_W)),
        (
            "default",
            super::support::a(&[f(sw::DEFAULT_W), f(sw::DEFAULT_H)]),
        ),
        ("min", super::support::a(&[f(sw::MIN_W), f(sw::MIN_H)])),
        ("extra_h", f(sw::WINDOW_EXTRA_H)),
        ("body_inset", f(sw::BODY_INSET)),
        ("edge", f(sw::EDGE)),
        ("corner", f(sw::CORNER)),
        ("header_h", f(sw::HEADER_H)),
        ("keep_visible", f(sw::KEEP_VISIBLE)),
        ("items_top", f(sw::ITEMS_TOP)),
        ("item_h", f(sw::ITEM_H)),
        ("item_gap", f(sw::ITEM_GAP)),
        (
            "item_x",
            super::support::a(&[f(sw::ITEM_X0), f(sw::ITEM_X1)]),
        ),
        ("item_radius", f(sw::ITEM_RADIUS)),
        ("icon", f(sw::ICON_SIZE)),
        ("mark_d", f(sw::MARK_D)),
        ("mark_cx", f(sw::MARK_CX)),
        ("mark_from_bottom", f(sw::MARK_FROM_BOTTOM)),
        ("foot_h", f(sw::FOOT_H)),
        ("label_cy", f(sw::LABEL_CY)),
        ("cap_ratio", f(sw::CAP_RATIO)),
        ("icons", o(&icons)),
    ]);
    one(case, value);
}

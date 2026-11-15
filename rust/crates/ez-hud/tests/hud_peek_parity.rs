//! Paridad de la vista rápida contra `parity/fixtures/hud_peek.jsonl` (P-HUD-B).

use std::path::Path;

use ez_core::pyjson::float_repr;
use ez_hud::paint::peek::{
    age, baselines, card_height, dismiss_at, fit, meta_line, paint_marks, paint_rows, row_at,
    PeekBuffer, PeekItem, AGENT_NAMES, CLOSE_SECS, DISMISS_W, FS_META, FS_TASK, GAP, HOVER_DELAY,
    LEAVE_GRACE, LINE_GAP, MARGIN, MARK_COL, MARK_ICON, MAX_ROWS, MORE_H, OPEN_SECS, PAD_X, PAD_Y,
    PEEK_W, RADIUS, REVEAL_SECS, RISE, ROW_H, ROW_INSET, SPIN_STEPS, TOAST_SECS,
};
use ez_hud::win::peek::compose;
use ez_hud::win::peek::place_card;
use ez_parity::fixtures::{self, FixtureCase};
use ez_parity::image as parity_image;

const FIXTURES: &str = "../../../parity/fixtures/hud_peek.jsonl";
const CASE_COUNT: usize = 154;

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

fn normalize_numbers(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
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

fn compare(case: &FixtureCase, value: String) {
    let produced = ez_core::pyloads::loads(&value).expect("JSON producido válido");
    let mut options = ez_core::pyjson::DumpOptions::utf8();
    options.sort_keys = true;
    let expected = normalize_numbers(&ez_core::pyjson::dumps_with(&case.output, &options));
    let actual = normalize_numbers(&ez_core::pyjson::dumps_with(&produced, &options));
    assert!(
        expected == actual,
        "{}:\n  esperado: {expected}\n  producido: {actual}",
        case.case
    );
}

macro_rules! item_from {
    ($value:expr) => {{
        let value = $value;
        PeekItem {
            key: value
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into(),
            title: value
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into(),
            agent: value
                .get("agent")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into(),
            state: value
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into(),
            since: value.get("since").and_then(|v| v.as_f64()).unwrap_or(0.0),
            unseen: value
                .get("unseen")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            live: value.get("live").and_then(|v| v.as_bool()).unwrap_or(false),
        }
    }};
}

macro_rules! items_from {
    ($case:expr, $key:expr) => {
        $case
            .input
            .get($key)
            .and_then(|v| v.as_array())
            .map(|rows| {
                rows.iter()
                    .map(|row| item_from!(row))
                    .collect::<Vec<PeekItem>>()
            })
            .unwrap_or_default()
    };
}

fn num(case: &FixtureCase, key: &str) -> f64 {
    case.input.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

struct Icons;

impl ez_hud::paint::peek::IconPainter for Icons {
    fn draw_icon(
        &self,
        buf: &mut PeekBuffer,
        name: &str,
        cx: f64,
        cy: f64,
        size: f64,
        color: ez_hud::tokens::Color,
        scale: f64,
    ) {
        let color = match color {
            ez_hud::tokens::Color::Rgb(rgb) => ez_gfx::icons::Color::Rgb(rgb[0], rgb[1], rgb[2]),
            ez_hud::tokens::Color::Rgba(rgba) => {
                ez_gfx::icons::Color::Rgba(rgba[0], rgba[1], rgba[2], rgba[3])
            }
        };
        let _ = ez_gfx::icons::draw_icon(
            &mut buf.data,
            buf.width,
            buf.height,
            name,
            cx,
            cy,
            size,
            color,
            scale,
            1.0,
        );
    }
}

fn straight_bgra(buffer: &PeekBuffer) -> image::RgbaImage {
    let mut data = Vec::with_capacity(buffer.data.len());
    for pixel in buffer.data.as_chunks::<4>().0 {
        let mut quantized = [0_u8; 4];
        for channel in 0..4 {
            quantized[channel] =
                (f64::from(pixel[channel].clamp(0.0, 1.0)) * 255.0).round_ties_even() as u8;
        }
        let alpha = f64::from(quantized[3]) / 255.0;
        for value in &quantized[..3] {
            let value = if alpha > 0.0 {
                (f64::from(*value) / 255.0 / alpha).clamp(0.0, 1.0)
            } else {
                0.0
            };
            data.push((value * 255.0).round_ties_even() as u8);
        }
        data.push(quantized[3]);
    }
    image::RgbaImage::from_raw(buffer.width as u32, buffer.height as u32, data).expect("búfer RGBA")
}

fn check_png(case: &FixtureCase) {
    let items = items_from!(case, "items");
    let more = num(case, "more") as usize;
    let width = num(case, "width") as usize;
    let height = num(case, "height") as usize;
    let scale = num(case, "scale");
    let theme = case
        .input
        .get("theme")
        .and_then(|v| v.as_str())
        .unwrap_or("dark");
    let hover = case
        .input
        .get("hover")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);
    let now = num(case, "now");
    let marks_now = num(case, "marks_now");
    let reduced = case
        .input
        .get("reduced")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let progress = num(case, "progress");
    let above = case
        .input
        .get("above")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut buffer = paint_rows(
        &items, more, width, height, scale, theme, hover, now, &Icons,
    );
    paint_marks(
        &mut buffer,
        &items,
        scale,
        theme,
        marks_now,
        reduced,
        &Icons,
    );
    let frame = compose(&buffer, progress, above, scale, reduced);
    let produced = straight_bgra(&frame);
    let png = case
        .output
        .get("png")
        .and_then(|v| v.as_str())
        .expect("PNG");
    let expected =
        parity_image::decode(&Path::new("../../../parity/golden").join(png)).expect("PNG dorado");
    let delta = parity_image::compare(&produced, &expected);
    let thresholds = parity_image::Thresholds::load(Path::new("../../../parity/thresholds.toml"))
        .expect("umbrales")
        .pixels
        .text;
    assert!(
        parity_image::within(&delta, &thresholds),
        "{} ({png}): max={} p995={} p999={} ssim={}",
        case.case,
        delta.max,
        delta.p995,
        delta.p999,
        delta.ssim
    );
}

fn style(name: &str) -> ez_hud::tokens::FontStyle {
    if name == "FS_META" {
        FS_META
    } else {
        FS_TASK
    }
}

fn constants(case: &FixtureCase) {
    let toast: Vec<(&str, String)> = TOAST_SECS
        .iter()
        .map(|(key, value)| (*key, f(*value)))
        .collect();
    let agents: Vec<(&str, String)> = AGENT_NAMES
        .iter()
        .map(|(key, value)| (*key, s(value)))
        .collect();
    let style_json = |style: ez_hud::tokens::FontStyle| {
        o(&[
            ("token", s(style.token)),
            ("family", s(style.family)),
            ("size", f(style.size)),
            ("weight", i(i64::from(style.weight))),
        ])
    };
    compare(
        case,
        o(&[
            ("PEEK_W", f(PEEK_W)),
            ("ROW_H", f(ROW_H)),
            ("PAD_X", f(PAD_X)),
            ("PAD_Y", f(PAD_Y)),
            ("MARK_COL", f(MARK_COL)),
            ("RADIUS", f(RADIUS)),
            ("ROW_INSET", f(ROW_INSET)),
            ("LINE_GAP", f(LINE_GAP)),
            ("MARK_ICON", f(MARK_ICON)),
            ("MARGIN", f(MARGIN)),
            ("GAP", f(GAP)),
            ("MAX_ROWS", i(MAX_ROWS as i64)),
            ("DISMISS_W", f(DISMISS_W)),
            ("MORE_H", f(MORE_H)),
            ("HOVER_DELAY", f(HOVER_DELAY)),
            ("LEAVE_GRACE", f(LEAVE_GRACE)),
            ("OPEN_SECS", f(OPEN_SECS)),
            ("CLOSE_SECS", f(CLOSE_SECS)),
            ("RISE", f(RISE)),
            ("TOAST_SECS", o(&toast)),
            ("REVEAL_SECS", f(REVEAL_SECS)),
            ("SPIN_STEPS", i(SPIN_STEPS as i64)),
            ("AGENT_NAMES", o(&agents)),
            ("FS_TASK", style_json(FS_TASK)),
            ("FS_META", style_json(FS_META)),
        ]),
    );
}

#[test]
fn every_peek_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de peek");
    for case in &cases {
        match case.function.as_str() {
            "peek.compose" => check_png(case),
            "peek.age" => compare(case, o(&[("text", s(&age(num(case, "seconds"))))])),
            "peek.meta_line" => {
                let item = item_from!(case.input.get("item").expect("item"));
                compare(case, o(&[("text", s(&meta_line(&item, num(case, "now"))))]));
            }
            "peek.fit" => {
                let name = case
                    .input
                    .get("style")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                compare(
                    case,
                    o(&[(
                        "text",
                        s(&fit(
                            case.input
                                .get("text")
                                .and_then(|v| v.as_str())
                                .unwrap_or(""),
                            style(name),
                            num(case, "max_w"),
                            num(case, "scale"),
                        )),
                    )]),
                );
            }
            "peek.card_height" => compare(
                case,
                o(&[(
                    "height",
                    f(card_height(
                        num(case, "rows") as usize,
                        case.input
                            .get("more")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false),
                    )),
                )]),
            ),
            "peek.baselines" => {
                let (first, second) = baselines(num(case, "scale"));
                compare(case, o(&[("first", f(first)), ("second", f(second))]));
            }
            "peek.constants" => constants(case),
            "peek.place_card" => {
                let values = |key: &str| {
                    case.input
                        .get(key)
                        .and_then(|v| v.as_array())
                        .map(|items| {
                            items
                                .iter()
                                .map(|v| v.as_f64().unwrap_or(0.0))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                };
                let hub = values("hub");
                let size = values("size");
                let work = values("work");
                let place = place_card(
                    (hub[0], hub[1], hub[2], hub[3]),
                    (size[0] as i32, size[1] as i32),
                    (
                        work[0] as i32,
                        work[1] as i32,
                        work[2] as i32,
                        work[3] as i32,
                    ),
                    num(case, "scale"),
                );
                compare(
                    case,
                    o(&[(
                        "place",
                        a(&[i(place.0 as i64), i(place.1 as i64), b(place.2)]),
                    )]),
                );
            }
            "peek.row_at" => {
                let rows = items_from!(case, "rows");
                let found = row_at(
                    rows.len(),
                    num(case, "x"),
                    num(case, "y"),
                    num(case, "scale"),
                );
                compare(
                    case,
                    o(&[(
                        "row",
                        found.map_or_else(|| "null".into(), |index| i(index as i64)),
                    )]),
                );
            }
            "peek.dismiss_at" => {
                let rows = items_from!(case, "rows");
                let found = dismiss_at(&rows, num(case, "x"), num(case, "y"), num(case, "scale"));
                compare(
                    case,
                    o(&[("key", found.map_or_else(|| "null".into(), |key| s(&key)))]),
                );
            }
            other => panic!("{}: función sin cubrir {other}", case.case),
        }
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_in_file, CASE_COUNT, "faltan casos en {FIXTURES}");
}

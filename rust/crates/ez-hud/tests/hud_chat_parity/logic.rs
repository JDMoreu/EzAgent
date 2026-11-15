//! Casos puros de `hud_chat`: constantes, medidas, formas y estilo.

use ez_gfx::Rgba;
use ez_hud::model::chat::footer::{
    break_line, input_box_height, AUDIO_W, BTN, HINT_H, INPUT_LINE, INPUT_MAX_LINES, INPUT_MIN, PAD,
};
use ez_hud::model::chat::header::{DELETE_SECS, HEADER_H, NEW_ROW_H, ORB_D};
use ez_hud::model::chat::view::{
    COPY_ROW, COPY_SLOT, COPY_W, GAP, KEYED, SCROLL_STEP, STREAM_REFRESH,
};
use ez_hud::paint::chat::labels::{stamp_height, LabelMessage, FS_STAMP, STAMP_H, STAMP_INSET};
use ez_hud::paint::chat::render::{CODE_HEAD, CODE_LINE, CODE_PAD, LIST_MARKER, LIST_STEP};
use ez_hud::paint::chat::render_rich::{
    clip, date_of, family_color, CARD_H, CELL_MIN, CELL_PAD_X, CELL_PAD_Y, IMAGE_MAX_H, PREVIEW_MAX,
};
use ez_hud::paint::chat::{wrap_spans, Ctx, LINE_K, PARA_GAP};
use ez_hud::win::chat::compose::{fluid_array, row_reveal, shape_progress};
use ez_hud::win::chat::{
    duration, elapsed_for, hotkey_label, step, Anim, AnimState, ACTIVE_POSES, CLOSE_SECS,
    FADE_SECS, MARGIN, MIN_H, MIN_W, MORPH_SECS, PANEL_H, PANEL_RADIUS, PANEL_W, STATUS_TEXT,
};
use ez_parity::fixtures::FixtureCase;

use super::markdown::parse_inline;
use super::support::{a, b, f, flag, i, int, line, num, o, s};

fn palette(theme: &str) -> ez_hud::tokens::ColorMap {
    ez_hud::tokens::palette(theme).expect("la paleta existe")
}

fn rgba(value: Rgba) -> String {
    a(&[
        i(i64::from(value.r)),
        i(i64::from(value.g)),
        i(i64::from(value.b)),
        i(i64::from(value.a)),
    ])
}

fn ctx<'a>(scale: f64, pal: &'a ez_hud::tokens::ColorMap) -> Ctx<'a> {
    Ctx::new(scale, pal, 1000.0, &super::support::EmptyHost)
}

pub(super) fn run(case: &FixtureCase, function: &str) {
    match function {
        "chat_footer.constants" => compare(
            case,
            o(&[
                ("PAD", f(PAD)),
                ("BTN", f(BTN)),
                ("INPUT_MIN", f(INPUT_MIN)),
                ("INPUT_LINE", f(INPUT_LINE)),
                ("INPUT_MAX_LINES", i(INPUT_MAX_LINES as i64)),
                ("HINT_H", f(HINT_H)),
                ("AUDIO_W", f(AUDIO_W)),
            ]),
        ),
        "chat_header.constants" => compare(
            case,
            o(&[
                ("HEADER_H", f(HEADER_H)),
                ("DELETE_SECS", f(DELETE_SECS)),
                ("ORB_D", f(ORB_D)),
                ("NEW_ROW_H", f(NEW_ROW_H)),
            ]),
        ),
        "chat_ui.constants" => {
            let status: Vec<(&str, String)> = STATUS_TEXT
                .iter()
                .map(|(key, value)| (*key, s(value)))
                .collect();
            let mut poses: Vec<&str> = ACTIVE_POSES.to_vec();
            poses.sort_unstable();
            compare(
                case,
                o(&[
                    ("CLOSE_SECS", f(CLOSE_SECS)),
                    ("STATUS_TEXT", o(&status)),
                    (
                        "ACTIVE_POSES",
                        a(&poses.iter().map(|pose| s(pose)).collect::<Vec<_>>()),
                    ),
                ]),
            );
        }
        "chat_view.constants" => compare(
            case,
            o(&[
                ("GAP", f(GAP)),
                (
                    "KEYED",
                    a(&KEYED.iter().map(|key| s(key)).collect::<Vec<_>>()),
                ),
                ("COPY_W", f(COPY_W)),
                ("COPY_SLOT", f(COPY_SLOT)),
                ("COPY_ROW", f(COPY_ROW)),
                ("SCROLL_STEP", f(SCROLL_STEP)),
                ("STREAM_REFRESH", f(STREAM_REFRESH)),
            ]),
        ),
        "chat.constants" => compare(case, constants()),
        "render_base.constants" => {
            compare(case, o(&[("LINE_K", f(LINE_K)), ("PARA_GAP", f(PARA_GAP))]))
        }
        "render_rich.constants" => compare(case, rich_constants()),
        "chat_render.constants" => compare(
            case,
            o(&[
                ("CODE_LINE", f(CODE_LINE)),
                ("CODE_HEAD", f(CODE_HEAD)),
                ("CODE_PAD", f(CODE_PAD)),
                ("LIST_STEP", f(LIST_STEP)),
                ("LIST_MARKER", f(LIST_MARKER)),
            ]),
        ),
        "render_base.styles" => {
            let key = line(case, "style");
            let style = ez_gfx::text::chat::style(&key).expect("estilo del chat");
            compare(
                case,
                o(&[
                    ("token", s(style.token)),
                    ("family", s(style.family)),
                    ("size", f(style.size)),
                    ("weight", i(i64::from(style.weight))),
                    ("tracking_em", f(style.tracking_em)),
                    ("uppercase", b(style.uppercase)),
                ]),
            );
        }
        "render_base.Ctx.px" => {
            let scale = num(case, "scale");
            let pal = palette("light");
            compare(
                case,
                o(&[("px", i(i64::from(ctx(scale, &pal).px(num(case, "value")))))]),
            );
        }
        "render_base.Ctx.color" => {
            let pal = palette("light");
            let alpha = case.input.get("alpha").and_then(|v| v.as_f64());
            let color = ctx(1.0, &pal).color(&line(case, "name"), alpha);
            compare(case, o(&[("rgba", rgba(color))]));
        }
        "render_base.Ctx.length" => {
            let pal = palette("light");
            let length = ctx(1.0, &pal).length(&line(case, "text"), &line(case, "style"));
            compare(case, o(&[("length", f(length))]));
        }
        "render_base.wrap" => compare(case, wrap(case)),
        "chat_footer._break" => {
            let ranges = break_line(&line(case, "text"), num(case, "width"), num(case, "scale"));
            let rows: Vec<String> = ranges
                .iter()
                .map(|(start, end)| a(&[i(*start as i64), i(*end as i64)]))
                .collect();
            compare(case, o(&[("ranges", a(&rows))]));
        }
        "chat_footer.input_box_height" => {
            let rows = int(case, "rows").max(0) as usize;
            compare(case, o(&[("height", f(input_box_height(rows)))]));
        }
        "chat_labels.stamp_height" => {
            struct Msg(String);
            impl LabelMessage for Msg {
                fn role(&self) -> &str {
                    &self.0
                }
                fn streaming(&self) -> bool {
                    false
                }
                fn timestamp(&self) -> f64 {
                    0.0
                }
            }
            compare(
                case,
                o(&[("height", f(stamp_height(&Msg(line(case, "role")))))]),
            );
        }
        "chat_labels.constants" => compare(
            case,
            o(&[
                (
                    "FS_STAMP",
                    o(&[
                        ("token", s(FS_STAMP.token)),
                        ("family", s(FS_STAMP.family)),
                        ("size", f(FS_STAMP.size)),
                        ("weight", i(i64::from(FS_STAMP.weight))),
                    ]),
                ),
                ("STAMP_H", f(STAMP_H)),
                ("STAMP_INSET", f(STAMP_INSET)),
            ]),
        ),
        "chat_paint.shape_progress" => compare(
            case,
            o(&[(
                "value",
                f(shape_progress(num(case, "p"), flag(case, "opening"))),
            )]),
        ),
        "chat_paint.row_reveal" => {
            let values = row_reveal(
                int(case, "rows").max(0) as usize,
                num(case, "p"),
                flag(case, "opening"),
            );
            let json = a(&values
                .iter()
                .map(|value| f(f64::from(*value)))
                .collect::<Vec<_>>());
            compare(case, o(&[("values", json)]));
        }
        "chat_ui.fluid_array" => compare(case, fluid(case)),
        "chat_ui._hotkey_label" => {
            compare(case, o(&[("label", s(&hotkey_label(&line(case, "spec"))))]))
        }
        "render_rich._clip" => {
            let pal = palette("light");
            let clipped = clip(
                &ctx(1.0, &pal),
                &line(case, "text"),
                num(case, "width"),
                &line(case, "style"),
            );
            compare(case, o(&[("text", s(&clipped))]));
        }
        "render_rich._date" => {
            let wall: Vec<i64> = case
                .input
                .get("mtime_wall")
                .and_then(|v| v.as_array())
                .map(|items| items.iter().filter_map(|v| v.as_i64()).collect())
                .unwrap_or_default();
            let epoch = super::support::epoch_from_wall(&wall);
            compare(case, o(&[("date", s(&date_of(epoch)))]));
        }
        "chat_ui.ChatPanel._duration" => compare(
            case,
            o(&[
                ("seconds", f(duration(flag(case, "reduced"), false))),
                ("closing", f(duration(flag(case, "reduced"), true))),
            ]),
        ),
        "chat_ui.ChatPanel._elapsed_for" => compare(
            case,
            o(&[(
                "seconds",
                f(elapsed_for(false, num(case, "p"), num(case, "duration"))),
            )]),
        ),
        "chat_ui.ChatPanel._step" => step_case(case),
        other => panic!("{}: función sin cubrir {other}", case.case),
    }
}

fn compare(case: &FixtureCase, value: String) {
    super::support::compare(case, value);
}

/// `chat.constants`: the constants of the whole chat domain.
fn constants() -> String {
    o(&[
        ("GAP", f(GAP)),
        ("COPY_W", f(COPY_W)),
        ("COPY_SLOT", f(COPY_SLOT)),
        ("COPY_ROW", f(COPY_ROW)),
        ("SCROLL_STEP", f(SCROLL_STEP)),
        ("STREAM_REFRESH", f(STREAM_REFRESH)),
        ("HEADER_H", f(HEADER_H)),
        ("DELETE_SECS", f(DELETE_SECS)),
        ("INPUT_MIN", f(INPUT_MIN)),
        ("INPUT_LINE", f(INPUT_LINE)),
        ("INPUT_MAX_LINES", i(INPUT_MAX_LINES as i64)),
        ("PAD", f(PAD)),
        ("BTN", f(BTN)),
        ("HINT_H", f(HINT_H)),
        ("AUDIO_W", f(AUDIO_W)),
        ("PANEL_W", f(PANEL_W)),
        ("PANEL_H", f(PANEL_H)),
        ("MIN_W", f(MIN_W)),
        ("MIN_H", f(MIN_H)),
        ("MARGIN", f(MARGIN)),
        ("PANEL_RADIUS", f(PANEL_RADIUS)),
        ("MORPH_SECS", f(MORPH_SECS)),
        ("FADE_SECS", f(FADE_SECS)),
    ])
}

fn rich_constants() -> String {
    const MONTHS: [&str; 12] = [
        "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
    ];
    const EXTS: [&str; 11] = [
        ".pdf", ".doc", ".docx", ".odt", ".xls", ".xlsx", ".ods", ".csv", ".ppt", ".pptx", ".odp",
    ];
    let family: Vec<(&str, String)> = EXTS
        .iter()
        .map(|ext| {
            let rgb = family_color(ext);
            (
                *ext,
                a(&[
                    i(i64::from(rgb[0])),
                    i(i64::from(rgb[1])),
                    i(i64::from(rgb[2])),
                ]),
            )
        })
        .collect();
    o(&[
        ("CELL_PAD_X", f(CELL_PAD_X)),
        ("CELL_PAD_Y", f(CELL_PAD_Y)),
        ("CELL_MIN", f(CELL_MIN)),
        ("CARD_H", f(CARD_H)),
        ("PREVIEW_MAX", f(PREVIEW_MAX)),
        ("IMAGE_MAX_H", f(IMAGE_MAX_H)),
        (
            "MONTHS",
            a(&MONTHS.iter().map(|month| s(month)).collect::<Vec<_>>()),
        ),
        ("FAMILY", o(&family)),
    ])
}

fn wrap(case: &FixtureCase) -> String {
    let spans = parse_inline(&line(case, "text"));
    let lines = wrap_spans(&spans, &line(case, "base"), num(case, "width"), 1.0);
    let mut out = Vec::new();
    for runs in &lines {
        let mut row = Vec::new();
        for run in runs {
            row.push(o(&[
                ("x", f(run.x)),
                ("text", s(&run.text)),
                ("width", f(run.width)),
                ("key", s(run.piece.key)),
                ("italic", b(run.piece.italic)),
                ("code", b(run.piece.code)),
                ("link", b(run.piece.link)),
                ("space", b(run.piece.space)),
            ]));
        }
        out.push(a(&row));
    }
    o(&[("lines", a(&out))])
}

fn fluid(case: &FixtureCase) -> String {
    let rgb = case.input.get("rgb").filter(|value| !value.is_null());
    let dims = rgb.and_then(|value| value.as_array()).map(|rows| {
        (
            rows.len(),
            rows.first()
                .and_then(|row| row.as_array())
                .map_or(0, Vec::len),
        )
    });
    let values = match (rgb, dims) {
        (Some(raw), Some((rows, columns))) => {
            let mut data = Vec::with_capacity(rows * columns * 3);
            for row in raw.as_array().expect("filas") {
                for cell in row.as_array().expect("celdas") {
                    for channel in cell.as_array().expect("canales") {
                        data.push(channel.as_u64().expect("u8") as u8);
                    }
                }
            }
            let array = ndarray::Array3::from_shape_vec((rows, columns, 3), data).expect("rgb");
            fluid_array(
                Some(&array),
                int(case, "w") as usize,
                int(case, "h") as usize,
            )
        }
        _ => fluid_array(None, int(case, "w") as usize, int(case, "h") as usize),
    };
    match values {
        Some(array) => {
            let (rows, columns, _) = array.dim();
            let mut out = Vec::new();
            for row in 0..rows {
                let mut cells = Vec::new();
                for column in 0..columns {
                    cells.push(a(&[
                        f(f64::from(array[[row, column, 0]])),
                        f(f64::from(array[[row, column, 1]])),
                        f(f64::from(array[[row, column, 2]])),
                    ]));
                }
                out.push(a(&cells));
            }
            o(&[("values", a(&out))])
        }
        None => o(&[("values", "null".into())]),
    }
}

fn step_case(case: &FixtureCase) {
    let state = match line(case, "state").as_str() {
        "closing" => AnimState::Closing,
        _ => AnimState::Opening,
    };
    let mut anim = Anim {
        state,
        t0: 0.0,
        p: 0.0,
    };
    let mut finished = false;
    let mut values = Vec::new();
    for now in [0.0, 0.05, 0.1, 0.16, 0.2, 0.28, 0.3] {
        let p = step(&mut anim, now, false, &mut finished);
        values.push(o(&[
            ("now", f(now)),
            ("p", f(p)),
            ("state", s(state_name(anim.state))),
            ("finished", b(finished)),
        ]));
    }
    compare(case, o(&[("values", a(&values))]));
}

fn state_name(state: AnimState) -> &'static str {
    match state {
        AnimState::Closed => "closed",
        AnimState::Opening => "opening",
        AnimState::Open => "open",
        AnimState::Closing => "closing",
    }
}

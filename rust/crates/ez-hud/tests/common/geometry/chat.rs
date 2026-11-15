//! Casos de `chat_geometry` de `hud_geometry`.

use ez_hud::{geometry, pacing};

use crate::common::json::{
    arr, edges, edges_arg, f64s, field, num, obj, rect4, saved, work_area, Json,
};
use crate::common::FixtureCase;

fn coverage_case(input: &Json, expected: &Json) -> Json {
    let (w, h) = (
        field(input, "w").as_i64().expect("w") as i32,
        field(input, "h").as_i64().expect("h") as i32,
    );
    let radius = field(input, "radius").as_f64().expect("radius");
    let inset = field(input, "inset").as_f64().unwrap_or(0.0);
    let array = geometry::rounded_coverage(w, h, radius, inset);
    let (rows, columns) = array.dim();
    if expected.get("values").is_some() {
        let mut values = Vec::with_capacity(rows);
        for row in 0..rows {
            let mut line = Vec::with_capacity(columns);
            for column in 0..columns {
                line.push(num(f64::from(array[[row, column]])));
            }
            values.push(arr(line));
        }
        return obj(vec![
            ("shape", arr(vec![num(rows as f64), num(columns as f64)])),
            ("values", arr(values)),
        ]);
    }
    let mut samples = Vec::new();
    for item in field(expected, "samples").as_array().expect("muestras") {
        let xy = f64s(item);
        let (x, y) = (xy[0] as usize, xy[1] as usize);
        samples.push(arr(vec![
            num(x as f64),
            num(y as f64),
            num(f64::from(array[[y, x]])),
        ]));
    }
    obj(vec![
        ("shape", arr(vec![num(rows as f64), num(columns as f64)])),
        ("samples", arr(samples)),
        ("center", num(f64::from(array[[rows / 2, columns / 2]]))),
    ])
}

fn shadow_case(input: &Json) -> Json {
    let (w, h) = (
        field(input, "w").as_i64().expect("w") as i32,
        field(input, "h").as_i64().expect("h") as i32,
    );
    let radius = field(input, "radius").as_f64().expect("radius");
    let reach = field(input, "reach").as_i64().expect("reach") as i32;
    let array = geometry::rounded_shadow(w, h, radius, reach);
    let (rows, columns) = array.dim();
    let samples = [
        (0, 0),
        (columns - 1, 0),
        (0, rows - 1),
        (columns - 1, rows - 1),
        (columns / 2, rows / 2),
        (columns / 2, 0),
    ];
    let items = samples
        .iter()
        .map(|(x, y)| {
            arr(vec![
                num(*x as f64),
                num(*y as f64),
                num(f64::from(array[[*y, *x]])),
            ])
        })
        .collect();
    obj(vec![
        ("shape", arr(vec![num(rows as f64), num(columns as f64)])),
        ("samples", arr(items)),
    ])
}

pub fn run(case: &FixtureCase) -> Result<Json, String> {
    let input = &case.input;
    Ok(match case.function.as_str() {
        "chat_geometry.constants" => obj(vec![
            ("PANEL_W", num(geometry::PANEL_W)),
            ("PANEL_H", num(geometry::PANEL_H)),
            ("MIN_W", num(geometry::MIN_W)),
            ("MIN_H", num(geometry::MIN_H)),
            ("MARGIN", num(geometry::MARGIN)),
            ("SCREEN_PAD", num(geometry::SCREEN_PAD)),
            ("PANEL_RADIUS", num(geometry::PANEL_RADIUS)),
            ("GRIP", num(geometry::GRIP)),
            ("MORPH_SECS", num(geometry::MORPH_SECS)),
            ("FADE_SECS", num(geometry::FADE_SECS)),
            ("IDC_SIZEWE", num(geometry::IDC_SIZEWE.into())),
            ("IDC_SIZENS", num(geometry::IDC_SIZENS.into())),
            ("IDC_SIZENWSE", num(geometry::IDC_SIZENWSE.into())),
            ("IDC_SIZENESW", num(geometry::IDC_SIZENESW.into())),
        ]),
        "chat_geometry.smoothstep" => obj(vec![(
            "y",
            num(geometry::smoothstep(field(input, "x").as_f64().expect("x"))),
        )]),
        "chat_geometry.ease" => obj(vec![(
            "y",
            num(geometry::ease(field(input, "x").as_f64().expect("x"))),
        )]),
        "chat_geometry.ease_out_back" => obj(vec![(
            "y",
            num(geometry::ease_out_back_with(
                field(input, "x").as_f64().expect("x"),
                field(input, "overshoot").as_f64().expect("overshoot"),
            )),
        )]),
        "chat_geometry.lerp" => obj(vec![(
            "value",
            num(geometry::lerp(
                field(input, "a").as_f64().expect("a"),
                field(input, "b").as_f64().expect("b"),
                field(input, "k").as_f64().expect("k"),
            )),
        )]),
        "chat_geometry.lerp_rect" => {
            let rect = geometry::lerp_rect(
                rect4(&f64s(field(input, "a"))),
                rect4(&f64s(field(input, "b"))),
                field(input, "k").as_f64().expect("k"),
            );
            obj(vec![(
                "rect",
                arr(vec![num(rect.0), num(rect.1), num(rect.2), num(rect.3)]),
            )])
        }
        "chat_geometry.panel_limits" => {
            let size = geometry::panel_limits(
                work_area(field(input, "work")),
                field(input, "scale").as_f64().expect("scale"),
            );
            obj(vec![("size", arr(vec![num(size.0), num(size.1)]))])
        }
        "chat_geometry.panel_size" => {
            let size = geometry::panel_size(
                work_area(field(input, "work")),
                field(input, "scale").as_f64().expect("scale"),
                saved(input),
            );
            obj(vec![("size", arr(vec![num(size.0), num(size.1)]))])
        }
        "chat_geometry.panel_rect" => {
            let rect = geometry::panel_rect(
                rect4(&f64s(field(input, "form"))),
                work_area(field(input, "work")),
                field(input, "scale").as_f64().expect("scale"),
                saved(input),
            );
            obj(vec![(
                "rect",
                arr(vec![num(rect.0), num(rect.1), num(rect.2), num(rect.3)]),
            )])
        }
        "chat_geometry.rounded_coverage" => coverage_case(input, &case.output),
        "chat_geometry.rounded_shadow" => shadow_case(input),
        "chat_geometry.edges_at" => obj(vec![(
            "edges",
            edges(geometry::edges_at_grip(
                field(input, "x").as_f64().expect("x"),
                field(input, "y").as_f64().expect("y"),
                field(input, "w").as_f64().expect("w"),
                field(input, "h").as_f64().expect("h"),
                field(input, "grip").as_f64().expect("grip"),
            )),
        )]),
        "chat_geometry.edge_cursor" => obj(vec![(
            "cursor",
            num(geometry::edge_cursor(edges_arg(field(input, "edges"))).into()),
        )]),
        "chat_geometry.resize_rect" => {
            let rect = geometry::resize_rect(
                edges_arg(field(input, "edges")),
                rect4(&f64s(field(input, "rect"))),
                field(input, "dx").as_f64().expect("dx"),
                field(input, "dy").as_f64().expect("dy"),
                work_area(field(input, "work")),
                field(input, "scale").as_f64().expect("scale"),
            );
            obj(vec![(
                "rect",
                arr(vec![num(rect.0), num(rect.1), num(rect.2), num(rect.3)]),
            )])
        }
        "layered.constants" => obj(vec![("FRAME_SLACK", num(pacing::FRAME_SLACK))]),
        "layered.frame_due" => obj(vec![(
            "due",
            Json::Bool(pacing::frame_due(
                field(input, "now").as_f64().expect("now"),
                field(input, "last_paint").as_f64().expect("last"),
                field(input, "interval").as_f64().expect("interval"),
            )),
        )]),
        "layered.paint_due" => obj(vec![(
            "due",
            Json::Bool(pacing::paint_due(
                field(input, "visible").as_bool().expect("visible"),
                field(input, "dragging").as_bool().expect("dragging"),
                field(input, "dirty").as_bool().expect("dirty"),
                field(input, "now").as_f64().expect("now"),
                field(input, "last_paint").as_f64().expect("last"),
                field(input, "interval").as_f64().expect("interval"),
            )),
        )]),
        "layered.wait_delay" => obj(vec![(
            "delay",
            num(pacing::wait_delay(
                field(input, "visible").as_bool().expect("visible"),
                field(input, "last_paint").as_f64().expect("last"),
                field(input, "interval").as_f64().expect("interval"),
                field(input, "now").as_f64().expect("now"),
            )),
        )]),
        "layered.paint_mark" => obj(vec![(
            "mark",
            num(pacing::paint_mark(
                field(input, "now").as_f64().expect("now"),
                field(input, "last_paint").as_f64().expect("last"),
                field(input, "interval").as_f64().expect("interval"),
            )),
        )]),
        other => return Err(format!("función sin portar: {other}")),
    })
}

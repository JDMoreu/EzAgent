//! Casos de `hud_drop_model`: geometría de la barra y `DropModel.update`.

use ez_hud::model::drop::{
    choose_dock, clean_dock, emerge_order, form_center, layout_size, row_center_y, slots,
    DropModel, Update, BRIGHT, BTN, BUD_SS, BUTTONS, CIRCLE_D, DEFAULT_DOCK, DOCKS, DOUBLE_CLICK,
    FPS_BUSY, FPS_CALM, GAP, GRID_CELL, HOVER_HOLD, HOVER_MARGIN, HYSTERESIS, LAYOUTS,
    MOTION_MODES, NB, ORB_GAP, PAD, PILL_H, PILL_W, POSE_OF, SHADOW,
};

use crate::common::json::{arr, f64s, field, num, obj, pair, s, work_area, Json};
use crate::common::FixtureCase;

fn optional<'a>(value: &'a Json, name: &str) -> Option<&'a str> {
    field(value, name).as_str()
}

fn constants() -> Json {
    let default = obj(DEFAULT_DOCK
        .iter()
        .map(|(layout, dock)| (*layout, s(dock)))
        .collect());
    let docks = obj(DOCKS
        .iter()
        .map(|(layout, entries)| (*layout, arr(entries.iter().map(|dock| s(dock)).collect())))
        .collect());
    let poses = obj(POSE_OF.iter().map(|(pose, bar)| (*pose, s(bar))).collect());
    let bright = obj(BRIGHT
        .iter()
        .map(|(pose, value)| (*pose, num(*value)))
        .collect());
    obj(vec![
        ("BTN", num(BTN)),
        ("GAP", num(GAP)),
        ("PAD", num(PAD)),
        ("PILL_W", num(PILL_W)),
        ("PILL_H", num(PILL_H)),
        ("CIRCLE_D", num(CIRCLE_D)),
        ("ORB_GAP", num(ORB_GAP)),
        ("HOVER_MARGIN", num(HOVER_MARGIN)),
        ("DOUBLE_CLICK", num(DOUBLE_CLICK)),
        ("HOVER_HOLD", num(HOVER_HOLD)),
        ("BUD_SS", num(BUD_SS.into())),
        ("GRID_CELL", num(GRID_CELL)),
        ("SHADOW", num(SHADOW)),
        ("FPS_BUSY", num(FPS_BUSY)),
        ("FPS_CALM", num(FPS_CALM)),
        ("HYSTERESIS", num(HYSTERESIS)),
        ("BUTTONS", arr(BUTTONS.iter().map(|name| s(name)).collect())),
        ("NB", num(NB as f64)),
        ("LAYOUTS", arr(LAYOUTS.iter().map(|name| s(name)).collect())),
        (
            "MOTION_MODES",
            arr(MOTION_MODES.iter().map(|name| s(name)).collect()),
        ),
        ("DEFAULT_DOCK", default),
        ("DOCKS", docks),
        ("POSE_OF", poses),
        ("BRIGHT", bright),
    ])
}

fn python_key_error(layout: &str, dock: Option<&str>) -> String {
    format!(
        "('{layout}', '{}')",
        clean_dock(layout, Some(dock.unwrap_or("")))
    )
}

fn slots_case(input: &Json) -> Json {
    let layout = field(input, "layout").as_str().expect("layout");
    let mic = field(input, "mic").as_f64().expect("mic");
    let dock = optional(input, "dock");
    let centers = slots(layout, mic, dock);
    obj(centers
        .iter()
        .map(|(name, (x, y))| (*name, arr(vec![num(*x), num(*y)])))
        .collect())
}

fn model_case(input: &Json) -> Json {
    let layout = field(input, "layout").as_str().expect("layout");
    let mut model = DropModel::new(layout, optional(input, "dock"));
    model.form_hidden = field(input, "form_hidden").as_bool().unwrap_or(false);
    let probes: Vec<(f64, f64)> = field(input, "probes")
        .as_array()
        .expect("probes")
        .iter()
        .map(|probe| pair(&f64s(probe)))
        .collect();
    let mut snapshots = Vec::new();
    for step in field(input, "steps").as_array().expect("steps") {
        let now = field(step, "now").as_f64().expect("now");
        model.update(&Update {
            now,
            dt: field(step, "dt").as_f64().expect("dt"),
            pose: field(step, "pose").as_str().expect("pose"),
            rms: field(step, "rms").as_f64().expect("rms"),
            theme: field(step, "theme").as_str().expect("theme"),
            hover: field(step, "hover").as_bool().expect("hover"),
            asking: field(step, "asking").as_bool().expect("asking"),
            menu_open: field(step, "menu_open").as_bool().expect("menu_open"),
            reduced: field(step, "reduced").as_bool().expect("reduced"),
            buttons: field(step, "buttons").as_str().expect("buttons"),
            muted: field(step, "muted").as_bool().expect("muted"),
            hover_item: optional(step, "hover_item"),
            pressed: optional(step, "pressed"),
            in_call: field(step, "in_call").as_bool().expect("in_call"),
            custom: None,
        });
        let positions = obj(model
            .positions()
            .iter()
            .map(|(name, (cx, cy, scale, alpha))| {
                (
                    *name,
                    arr(vec![num(*cx), num(*cy), num(*scale), num(*alpha)]),
                )
            })
            .collect());
        let item_at = arr(probes
            .iter()
            .map(|(x, y)| match model.item_at(*x, *y) {
                Some(name) => s(name),
                None => Json::Null,
            })
            .collect());
        let near = arr(probes
            .iter()
            .map(|(x, y)| Json::Bool(model.near(*x, *y)))
            .collect());
        let (x0, y0, x1, y1) = model.bar_rect();
        let (hx0, hy0, hx1, hy1) = model.hub_rect(1.0);
        snapshots.push(obj(vec![
            ("now", num(now)),
            ("positions", positions),
            ("item_at", item_at),
            ("bar_rect", arr(vec![num(x0), num(y0), num(x1), num(y1)])),
            (
                "hub_rect",
                arr(vec![num(hx0), num(hy0), num(hx1), num(hy1)]),
            ),
            ("near", near),
            ("animating", Json::Bool(model.animating(now))),
            ("flow", num(model.flow())),
            ("bright", num(model.bright)),
            ("lvl", num(model.lvl)),
            ("peak", num(model.peak)),
        ]));
    }
    obj(vec![("snapshots", arr(snapshots))])
}

pub fn run(case: &FixtureCase) -> Result<Json, String> {
    let input = &case.input;
    Ok(match case.function.as_str() {
        "drop.constants" => constants(),
        "drop.clean_dock" => obj(vec![(
            "dock",
            s(clean_dock(
                field(input, "layout").as_str().expect("layout"),
                optional(input, "dock"),
            )),
        )]),
        "drop.layout_size" => {
            let (width, height) = layout_size(
                field(input, "layout").as_str().expect("layout"),
                optional(input, "dock"),
            );
            obj(vec![("size", arr(vec![num(width), num(height)]))])
        }
        "drop.emerge_order" => {
            let layout = field(input, "layout").as_str().expect("layout");
            if !LAYOUTS.contains(&layout) {
                return Err(python_key_error(layout, optional(input, "dock")));
            }
            let order = emerge_order(layout, optional(input, "dock"));
            obj(vec![(
                "order",
                arr(order
                    .iter()
                    .map(|group| arr(group.iter().map(|name| s(name)).collect()))
                    .collect()),
            )])
        }
        "drop.form_center" => {
            let layout = field(input, "layout").as_str().expect("layout");
            if !LAYOUTS.contains(&layout) {
                return Err(python_key_error(layout, optional(input, "dock")));
            }
            let (x, y) = form_center(layout, optional(input, "dock"));
            obj(vec![("center", arr(vec![num(x), num(y)]))])
        }
        "drop.row_center_y" => {
            let layout = field(input, "layout").as_str().expect("layout");
            if !LAYOUTS.contains(&layout) {
                return Err(python_key_error(layout, optional(input, "dock")));
            }
            obj(vec![(
                "y",
                num(row_center_y(layout, optional(input, "dock"))),
            )])
        }
        "drop.slots" => obj(vec![("slots", slots_case(input))]),
        "drop.choose_dock" => obj(vec![(
            "dock",
            s(choose_dock(
                field(input, "layout").as_str().expect("layout"),
                optional(input, "current"),
                pair(&f64s(field(input, "anchor"))),
                work_area(field(input, "work")),
                field(input, "bs").as_f64().expect("bs"),
            )),
        )]),
        "DropModel.update" => model_case(input),
        other => return Err(format!("función sin portar: {other}")),
    })
}

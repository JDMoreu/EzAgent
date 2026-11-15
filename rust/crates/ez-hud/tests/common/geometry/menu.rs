//! Casos del layout del menú de `hud_geometry`.

use ez_hud::layout;

use crate::common::json::{arr, field, num, obj, s, Json};
use crate::common::FixtureCase;

pub fn menu_constants() -> Json {
    let rows = obj(layout::ROW
        .iter()
        .map(|(kind, height)| (*kind, num((*height).into())))
        .collect());
    let groups = arr(layout::GROUPS
        .iter()
        .map(|(name, items)| {
            arr(vec![
                s(name),
                arr(items
                    .iter()
                    .map(|(key, kind)| arr(vec![s(key), s(kind)]))
                    .collect()),
            ])
        })
        .collect());
    let sections = arr(layout::SECTIONS
        .iter()
        .map(|(id, title, groups)| {
            arr(vec![
                s(id),
                s(title),
                arr(groups.iter().map(|g| s(g)).collect()),
            ])
        })
        .collect());
    let titles = obj(layout::SECTIONS
        .iter()
        .map(|(id, title, _)| (*id, s(title)))
        .collect());
    let group_map = obj(layout::SECTIONS
        .iter()
        .map(|(id, _, groups)| (*id, arr(groups.iter().map(|g| s(g)).collect())))
        .collect());
    let labels = obj(layout::GROUP_LABELS
        .iter()
        .map(|(key, label)| (*key, s(label)))
        .collect());
    let mut custom: Vec<&str> = layout::CUSTOM_THEME_ROWS.to_vec();
    custom.sort_unstable();
    let mut advanced: Vec<&str> = layout::ADVANCED_ROWS.to_vec();
    advanced.sort_unstable();
    obj(vec![
        ("ISLAND_H", num(layout::ISLAND_H.into())),
        ("PAD_TOP", num(layout::PAD_TOP.into())),
        ("HEADER", num(layout::HEADER.into())),
        ("GAP_HEADER", num(layout::GAP_HEADER.into())),
        ("LABEL", num(layout::LABEL.into())),
        ("GROUP_GAP", num(layout::GROUP_GAP.into())),
        ("FOOTER", num(layout::FOOTER.into())),
        ("PAD_BOTTOM", num(layout::PAD_BOTTOM.into())),
        ("ROW", rows),
        ("GROUPS", groups),
        (
            "CUSTOM_THEME_ROWS",
            arr(custom.iter().map(|v| s(v)).collect()),
        ),
        (
            "ADVANCED_ROWS",
            arr(advanced.iter().map(|v| s(v)).collect()),
        ),
        ("SECTIONS", sections),
        (
            "SIDEBAR_ORDER",
            arr(layout::SIDEBAR_ORDER.iter().map(|v| s(v)).collect()),
        ),
        ("SECTION_TITLES", titles),
        ("SECTION_GROUPS", group_map),
        ("GROUP_LABELS", labels),
        ("MENU_HEIGHT", num(layout::MENU_HEIGHT.into())),
    ])
}

fn layout_case(input: &Json) -> Json {
    let extra: std::collections::BTreeMap<String, f64> = field(input, "extra")
        .as_object()
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| Some((key.clone(), value.as_f64()?)))
                .collect()
        })
        .unwrap_or_default();
    let groups: Option<Vec<layout::Group>> = field(input, "groups").as_array().map(|items| {
        items
            .iter()
            .map(|item| {
                let pair = item.as_array().expect("grupo");
                let rows: Vec<layout::Row> = pair[1]
                    .as_array()
                    .expect("filas")
                    .iter()
                    .map(|row| {
                        let row = row.as_array().expect("fila");
                        let key: &'static str =
                            Box::leak(row[0].as_str().expect("clave").to_string().into_boxed_str());
                        let kind: &'static str =
                            Box::leak(row[1].as_str().expect("tipo").to_string().into_boxed_str());
                        (key, kind)
                    })
                    .collect();
                let rows: &'static [layout::Row] = Box::leak(rows.into_boxed_slice());
                let name: &'static str = Box::leak(
                    pair[0]
                        .as_str()
                        .expect("nombre")
                        .to_string()
                        .into_boxed_str(),
                );
                (name, rows)
            })
            .collect()
    });
    let out = layout::layout_menu(
        Some(&extra),
        field(input, "viewport_h").as_f64(),
        groups.as_deref(),
    );
    obj(out
        .iter()
        .map(|(key, rect)| {
            (
                key.as_str(),
                arr(vec![num(rect.x0), num(rect.y0), num(rect.x1), num(rect.y1)]),
            )
        })
        .collect())
}

pub fn run(case: &FixtureCase) -> Result<Json, String> {
    let input = &case.input;
    Ok(match case.function.as_str() {
        "menu_layout.constants" => menu_constants(),
        "menu_layout.layout_menu" => layout_case(input),
        "menu_layout.visible_menu_height" => obj(vec![(
            "height",
            num(layout::visible_menu_height(
                field(input, "screen_h").as_f64().expect("screen_h"),
            )),
        )]),
        other => return Err(format!("función sin portar: {other}")),
    })
}

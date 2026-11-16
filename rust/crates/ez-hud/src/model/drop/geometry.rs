//! Geometría de la barra: tamaños, huecos de los botones y elección del dock.
//!
//! Port de `src/ezagent/voice/hud/drop.py 62-249`. Todo es lógica pura: sin
//! Win32 ni dibujo, con las medidas en píxeles lógicos a 96 DPI.

use std::collections::BTreeMap;

/// Diámetro de los botones redondos.
pub const BTN: f64 = 36.0;
/// Separación entre elementos de la fila.
pub const GAP: f64 = 8.0;
/// Margen para sombras dentro de la ventana.
pub const PAD: f64 = 12.0;
/// La píldora es compacta: ni más ancha ni más alta.
pub const PILL_W: f64 = 108.0;
pub const PILL_H: f64 = 38.0;
pub const CIRCLE_D: f64 = 58.0;
/// Separación entre el círculo y la fila de botones.
pub const ORB_GAP: f64 = 10.0;
/// El puntero «cerca» de la barra hace aparecer los botones.
pub const HOVER_MARGIN: f64 = 22.0;
/// Segundos entre dos clics sobre la forma para cambiar pill ↔ circle.
pub const DOUBLE_CLICK: f64 = 0.4;
/// Segundos que los botones siguen tras retirar el puntero.
pub const HOVER_HOLD: f64 = 0.7;
/// Supersampling de los sprites de los botones.
pub const BUD_SS: i32 = 3;
/// Píxeles lógicos por celda del fluido (se escala suavemente).
pub const GRID_CELL: f64 = 2.0;
/// Margen de sombra de cada sprite.
pub const SHADOW: f64 = 10.0;
/// Mientras algo se mueve: botones, voz, crecimiento, iconos.
pub const FPS_BUSY: f64 = 60.0;
/// En reposo; el fluido sigue fluyendo despacio.
pub const FPS_CALM: f64 = 30.0;
/// Píxeles de holgura extra para volver al dock por defecto (evita parpadeo).
pub const HYSTERESIS: f64 = 24.0;

// DECISION: tres botones (☰ menú, 🎤 micrófono, ✕ colgar). «Repetir» pasa al
// menú y a la voz/atajo; «nueva conversación» vive en el menú con confirmación.
// Sin llamada el micrófono se pliega y ✕ se convierte en «llamar» (mismo botón).
pub const BUTTONS: [&str; 4] = ["gear", "chat", "mic", "hang"];
/// `len(BUTTONS)`.
pub const NB: usize = BUTTONS.len();
pub const LAYOUTS: [&str; 2] = ["pill", "circle"];
// DECISION: los botones solo aparecen al acercar el puntero; «always» queda para
// las pruebas y no se ofrece en el menú.
pub const MOTION_MODES: [&str; 3] = ["system", "full", "reduced"];

/// Dock por defecto de cada disposición.
pub const DEFAULT_DOCK: [(&str, &str); 2] = [("pill", "sides"), ("circle", "bottom")];
/// Docks válidos de cada disposición.
pub const DOCKS: [(&str, &[&str]); 2] = [
    ("pill", &["sides", "right", "left"]),
    ("circle", &["bottom", "top", "right", "left"]),
];

/// Punto lógico `(x, y)`.
pub type Point = (f64, f64);
/// Rectángulo lógico `(x0, y0, x1, y1)`.
pub type Rect = (f64, f64, f64, f64);
/// Área de trabajo de Windows: `(left, top, right, bottom)`.
pub type WorkArea = (i32, i32, i32, i32);
/// Centro de cada elemento: `name -> (cx, cy)`.
pub type Slots = BTreeMap<&'static str, Point>;

/// Botones antes y después de la forma, del más cercano al más lejano.
type SidePair = (&'static [&'static str], &'static [&'static str]);

/// `_SIDES` del Python: leídos de izquierda a derecha son siempre ☰ 🎤 ✕.
///
/// DECISION: una disposición desconocida cae a la píldora con botones a los
/// lados, donde el Python lanzaría `KeyError`; la API pública deja de abortar.
fn sides(layout: &str, dock: &str) -> SidePair {
    match (layout, dock) {
        ("pill", "sides") => (&["chat", "gear"], &["mic", "hang"]),
        ("pill", "right") | ("circle", "right") => (&[], &["gear", "chat", "mic", "hang"]),
        ("pill", "left") | ("circle", "left") => (&["hang", "mic", "chat", "gear"], &[]),
        _ => (&["chat", "gear"], &["mic", "hang"]),
    }
}

/// `DEFAULT_DOCK[layout]`.
pub fn default_dock(layout: &str) -> &'static str {
    if layout == "circle" {
        "bottom"
    } else {
        "sides"
    }
}

/// `DOCKS[layout]` (una disposición desconocida usa los de la píldora).
pub fn docks(layout: &str) -> &'static [&'static str] {
    if layout == "circle" {
        DOCKS[1].1
    } else {
        DOCKS[0].1
    }
}

/// `dock` si vale para la disposición; si no, el de por defecto.
pub fn clean_dock(layout: &str, dock: Option<&str>) -> &'static str {
    match (layout, dock) {
        ("pill", Some("sides")) => "sides",
        ("pill", Some("right")) => "right",
        ("pill", Some("left")) => "left",
        ("circle", Some("bottom")) => "bottom",
        ("circle", Some("top")) => "top",
        ("circle", Some("right")) => "right",
        ("circle", Some("left")) => "left",
        _ => default_dock(layout),
    }
}

/// Grupos de botones por salida: del hueco más cercano a la forma al más lejano.
pub fn emerge_order(layout: &str, dock: Option<&str>) -> Vec<Vec<&'static str>> {
    let dock = clean_dock(layout, dock);
    if layout == "circle" && (dock == "bottom" || dock == "top") {
        return vec![vec!["chat", "mic"], vec!["gear", "hang"]];
    }
    let (before, after) = sides(layout, dock);
    (0..before.len().max(after.len()))
        .map(|index| {
            let mut group = Vec::new();
            if index < before.len() {
                group.push(before[index]);
            }
            if index < after.len() {
                group.push(after[index]);
            }
            group
        })
        .collect()
}

/// Tamaño lógico de la ventana según la disposición y el dock.
pub fn layout_size(layout: &str, dock: Option<&str>) -> (f64, f64) {
    let dock = clean_dock(layout, dock);
    if layout == "circle" {
        if dock == "bottom" || dock == "top" {
            return (
                NB as f64 * BTN + (NB - 1) as f64 * GAP + 2.0 * PAD,
                PAD + CIRCLE_D + ORB_GAP + BTN + PAD,
            );
        }
        return (
            2.0 * PAD + CIRCLE_D + ORB_GAP + NB as f64 * BTN + (NB - 1) as f64 * GAP,
            2.0 * PAD + CIRCLE_D,
        );
    }
    (
        NB as f64 * BTN + PILL_W + NB as f64 * GAP + 2.0 * PAD,
        2.0 * PAD + BTN.max(PILL_H),
    )
}

/// Separación normal entre dos vecinos de la fila.
fn neighbor_gap(a: &str, b: &str, circle: bool) -> f64 {
    if circle && (a == "pill" || b == "pill") {
        ORB_GAP
    } else {
        GAP
    }
}

/// Centro x de cada elemento de una fila; `mic` es el hueco del micrófono.
fn row_x(order: &[&'static str], mic: f64, circle: bool) -> BTreeMap<&'static str, f64> {
    let form_w = if circle { CIRCLE_D } else { PILL_W };
    let mut out = BTreeMap::new();
    let mut x = PAD;
    for (index, name) in order.iter().enumerate() {
        let width = match *name {
            "pill" => form_w,
            "mic" => BTN * mic,
            _ => BTN,
        };
        out.insert(*name, x + width / 2.0);
        if index + 1 < order.len() {
            let next = order[index + 1];
            let mut gap = neighbor_gap(name, next, circle);
            if *name == "mic" && index > 0 {
                // Plegado, el micro no deja hueco: sus dos separaciones se funden
                // en la normal entre sus vecinos, igual que entre los demás.
                let previous = order[index - 1];
                let folded =
                    neighbor_gap(previous, next, circle) - neighbor_gap(previous, name, circle);
                gap = gap * mic + folded * (1.0 - mic);
            }
            x += width + gap;
        }
    }
    out
}

/// Centro de cada elemento. `mic` (0..1) es el hueco del micrófono: sin llamada
/// se pliega y ✕/llamar se desliza a su sitio; la forma no se mueve.
pub fn slots(layout: &str, mic: f64, dock: Option<&str>) -> Slots {
    let dock = clean_dock(layout, dock);
    let (width, height) = layout_size(layout, Some(dock));
    let mic = mic.clamp(0.0, 1.0);
    let circle = layout == "circle";
    if circle && (dock == "bottom" || dock == "top") {
        let widths = [BTN, BTN, BTN * mic, BTN];
        let gaps = [GAP, GAP, GAP * mic, 0.0]; // plegado, el micro no deja hueco extra
        let total: f64 = widths.iter().sum::<f64>() + gaps.iter().sum::<f64>();
        let mut x = width / 2.0 - total / 2.0;
        let bottom = dock == "bottom";
        let row_y = if bottom {
            PAD + CIRCLE_D + ORB_GAP + BTN / 2.0
        } else {
            PAD + BTN / 2.0
        };
        let form_y = if bottom {
            PAD + CIRCLE_D / 2.0
        } else {
            PAD + BTN + ORB_GAP + CIRCLE_D / 2.0
        };
        let mut out = BTreeMap::new();
        for ((name, width), gap) in BUTTONS.iter().zip(widths).zip(gaps) {
            out.insert(*name, (x + width / 2.0, row_y));
            x += width + gap;
        }
        out.insert("pill", (width / 2.0, form_y));
        return out;
    }
    let (before, after) = sides(layout, dock);
    let mut order: Vec<&'static str> = before.iter().rev().copied().collect();
    order.push("pill");
    order.extend(after.iter().copied());
    let full = row_x(&order, 1.0, circle);
    let current = row_x(&order, mic, circle);
    // La forma no se mueve cuando el micro se pliega.
    let shift = full["pill"] - current["pill"];
    order
        .iter()
        .map(|name| (*name, (current[*name] + shift, height / 2.0)))
        .collect()
}

/// Centro de la forma de color (cápsula o círculo).
pub fn form_center(layout: &str, dock: Option<&str>) -> Point {
    slots(layout, 1.0, dock)["pill"]
}

/// Altura de la fila de botones.
pub fn row_center_y(layout: &str, dock: Option<&str>) -> f64 {
    slots(layout, 1.0, dock)["gear"].1
}

/// Cuánto se sale de la zona útil (px) la ventana de `dock` con la forma en
/// `anchor`.
///
/// Se permite el mismo margen de sombras que `DropUI.clamp_position`; `extra`
/// lo reduce (holgura exigida para volver al dock por defecto).
pub fn window_overflow(
    layout: &str,
    dock: &str,
    anchor: Point,
    work: WorkArea,
    bs: f64,
    extra: f64,
) -> f64 {
    let (width, height) = layout_size(layout, Some(dock));
    let (fx, fy) = form_center(layout, Some(dock));
    let margin = (PAD * bs - extra).max(0.0);
    let x0 = anchor.0 - fx * bs;
    let y0 = anchor.1 - fy * bs;
    (work.0 as f64 - margin - x0).max(0.0)
        + (x0 + width * bs - (work.2 as f64 + margin)).max(0.0)
        + (work.1 as f64 - margin - y0).max(0.0)
        + (y0 + height * bs - (work.3 as f64 + margin)).max(0.0)
}

/// Lado de los botones que cabe en pantalla con la forma de color en `anchor`.
///
/// Prefiere el dock por defecto; si la barra está pegada a un borde o esquina,
/// pasa al primero que sí cabe. Para volver al de por defecto pide algo de
/// holgura (`HYSTERESIS`), así no parpadea al arrastrar justo en el límite.
pub fn choose_dock(
    layout: &str,
    current: Option<&str>,
    anchor: Point,
    work: WorkArea,
    bs: f64,
) -> &'static str {
    let current = clean_dock(layout, current);
    let default = default_dock(layout);
    let slack = if current == default {
        0.0
    } else {
        HYSTERESIS * bs
    };
    if window_overflow(layout, default, anchor, work, bs, slack) <= 0.0 {
        return default;
    }
    if window_overflow(layout, current, anchor, work, bs, 0.0) <= 0.0 {
        return current;
    }
    let mut best: Option<(&'static str, f64)> = None;
    for &dock in docks(layout) {
        let overflow = window_overflow(layout, dock, anchor, work, bs, 0.0);
        if overflow <= 0.0 {
            return dock;
        }
        if best.is_none_or(|(_, least)| overflow < least) {
            best = Some((dock, overflow));
        }
    }
    best.expect("DOCKS siempre tiene al menos un lado").0
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: WorkArea = (0, 0, 1920, 1040);

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn sizes_are_the_python_anchors() {
        assert_eq!(layout_size("pill", None), (308.0, 62.0));
        assert_eq!(layout_size("pill", Some("bottom")), (308.0, 62.0));
        assert_eq!(layout_size("circle", Some("bottom")), (192.0, 128.0));
        assert_eq!(layout_size("circle", Some("top")), (192.0, 128.0));
        assert_eq!(layout_size("circle", Some("right")), (260.0, 82.0));
        assert_eq!(layout_size("circle", Some("left")), (260.0, 82.0));
        assert_eq!(layout_size("circle", Some("raro")), (192.0, 128.0));
        assert_eq!(PILL_W, 108.0);
        assert_eq!(PILL_H, 38.0);
        assert_eq!(NB, 4);
    }

    #[test]
    fn slots_are_the_python_anchors() {
        let pill = slots("pill", 1.0, None);
        assert_eq!(pill["gear"], (30.0, 31.0));
        assert_eq!(pill["chat"], (74.0, 31.0));
        assert_eq!(pill["pill"], (154.0, 31.0));
        assert_eq!(pill["mic"], (234.0, 31.0));
        assert_eq!(pill["hang"], (278.0, 31.0));

        let folded = slots("pill", 0.0, None);
        assert_eq!(folded["pill"], (154.0, 31.0));
        assert_eq!(folded["mic"], (216.0, 31.0));
        assert_eq!(folded["hang"], (234.0, 31.0));

        let right = slots("pill", 1.0, Some("right"));
        assert_eq!(right["pill"], (66.0, 31.0));
        assert_eq!(right["gear"], (146.0, 31.0));
        assert_eq!(right["hang"], (278.0, 31.0));

        let bottom = slots("circle", 1.0, Some("bottom"));
        assert_eq!(bottom["gear"], (30.0, 98.0));
        assert_eq!(bottom["hang"], (162.0, 98.0));
        assert_eq!(bottom["pill"], (96.0, 41.0));

        let top = slots("circle", 1.0, Some("top"));
        assert_eq!(top["gear"], (30.0, 30.0));
        assert_eq!(top["pill"], (96.0, 87.0));

        let circle_right = slots("circle", 1.0, Some("right"));
        assert_eq!(circle_right["pill"], (41.0, 41.0));
        assert_eq!(circle_right["gear"], (98.0, 41.0));
        assert_eq!(circle_right["hang"], (230.0, 41.0));
    }

    #[test]
    fn every_element_fits_inside_the_window_and_the_form_never_moves() {
        for (layout, docks) in DOCKS {
            for &dock in docks {
                let (width, height) = layout_size(layout, Some(dock));
                for mic in [0.0, 0.5, 1.0] {
                    let sl = slots(layout, mic, Some(dock));
                    assert_eq!(sl.len(), NB + 1);
                    for (name, (x, y)) in &sl {
                        assert!(*x >= 0.0 && *x <= width, "{layout}/{dock}/{name}/{mic}");
                        assert!(*y >= 0.0 && *y <= height, "{layout}/{dock}/{name}/{mic}");
                    }
                }
                let folded = slots(layout, 0.0, Some(dock));
                let open = slots(layout, 1.0, Some(dock));
                assert_eq!(folded["pill"], open["pill"]);
                assert_eq!(form_center(layout, Some(dock)), open["pill"]);
                assert_eq!(row_center_y(layout, Some(dock)), open["gear"].1);
            }
        }
    }

    #[test]
    fn emerge_order_lists_every_button_once_nearest_first() {
        for (layout, docks) in DOCKS {
            for &dock in docks {
                let flat: Vec<&str> = emerge_order(layout, Some(dock)).concat();
                assert_eq!(flat.len(), NB);
                let sl = slots(layout, 1.0, Some(dock));
                let (fx, fy) = sl["pill"];
                let mut previous = 0.0;
                for name in flat {
                    let distance = ((sl[name].0 - fx).powi(2) + (sl[name].1 - fy).powi(2)).sqrt();
                    assert!(distance >= previous - 1e-9, "{layout}/{dock}/{name}");
                    previous = distance;
                }
            }
        }
        assert_eq!(
            emerge_order("pill", None),
            vec![vec!["chat", "mic"], vec!["gear", "hang"]]
        );
        assert_eq!(
            emerge_order("circle", None),
            vec![vec!["chat", "mic"], vec!["gear", "hang"]]
        );
        assert_eq!(
            emerge_order("pill", Some("right")),
            vec![vec!["gear"], vec!["chat"], vec!["mic"], vec!["hang"]]
        );
        assert_eq!(
            emerge_order("pill", Some("left")),
            vec![vec!["hang"], vec!["mic"], vec!["chat"], vec!["gear"]]
        );
    }

    #[test]
    fn unknown_docks_fall_back_to_the_default() {
        assert_eq!(clean_dock("circle", Some("diagonal")), "bottom");
        assert_eq!(clean_dock("pill", Some("bottom")), "sides");
        assert_eq!(clean_dock("pill", None), "sides");
        assert_eq!(default_dock("pill"), "sides");
        assert_eq!(default_dock("circle"), "bottom");
        assert_eq!(docks("circle").len(), 4);
        assert_eq!(docks("pill").len(), 3);
    }

    #[test]
    fn the_default_dock_is_kept_in_the_middle_of_the_screen() {
        assert_eq!(
            choose_dock("circle", None, (960.0, 500.0), WORK, 1.0),
            "bottom"
        );
        assert_eq!(
            choose_dock("pill", None, (960.0, 500.0), WORK, 1.0),
            "sides"
        );
        for bs in [0.6, 1.0, 1.5, 2.0] {
            assert_eq!(
                choose_dock("circle", None, (960.0, 500.0), WORK, bs),
                "bottom"
            );
            assert_eq!(choose_dock("pill", None, (960.0, 500.0), WORK, bs), "sides");
        }
    }

    #[test]
    fn near_an_edge_the_buttons_go_to_the_side_with_room() {
        assert_eq!(
            choose_dock("circle", Some("bottom"), (960.0, 1011.0), WORK, 1.0),
            "top"
        );
        assert_eq!(
            choose_dock("circle", Some("bottom"), (45.0, 500.0), WORK, 1.0),
            "right"
        );
        assert_eq!(
            choose_dock("circle", Some("bottom"), (1875.0, 500.0), WORK, 1.0),
            "left"
        );
        assert_eq!(
            choose_dock("pill", Some("sides"), (60.0, 500.0), WORK, 1.0),
            "right"
        );
        assert_eq!(
            choose_dock("pill", Some("sides"), (1860.0, 500.0), WORK, 1.0),
            "left"
        );
    }

    #[test]
    fn the_form_can_reach_the_edge_thanks_to_the_dock() {
        for (layout, anchor) in [("circle", (45.0, 500.0)), ("pill", (60.0, 500.0))] {
            let dock = choose_dock(layout, None, anchor, WORK, 1.0);
            assert_eq!(window_overflow(layout, dock, anchor, WORK, 1.0, 0.0), 0.0);
            assert!(window_overflow(layout, default_dock(layout), anchor, WORK, 1.0, 0.0) > 0.0);
        }
    }

    #[test]
    fn returning_to_the_default_dock_needs_some_slack() {
        let anchor = (960.0, 960.0);
        assert_eq!(choose_dock("circle", Some("top"), anchor, WORK, 1.0), "top");
        assert_eq!(
            choose_dock("circle", Some("bottom"), anchor, WORK, 1.0),
            "bottom"
        );
        assert_eq!(
            choose_dock("circle", Some("top"), (960.0, 700.0), WORK, 1.0),
            "bottom"
        );
    }

    #[test]
    fn a_valid_non_default_dock_is_not_changed_for_no_reason() {
        let anchor = (45.0, 1011.0);
        let first = choose_dock("circle", Some("bottom"), anchor, WORK, 1.0);
        assert!(docks("circle").contains(&first) && first != "bottom");
        assert_eq!(choose_dock("circle", Some(first), anchor, WORK, 1.0), first);
    }

    #[test]
    fn buttons_do_not_overlap_each_other_or_the_form() {
        for (layout, docks) in DOCKS {
            let circle = layout == "circle";
            for &dock in docks {
                let sl = slots(layout, 1.0, Some(dock));
                let mut names: Vec<&str> = BUTTONS.to_vec();
                names.push("pill");
                let form_r = if circle { CIRCLE_D } else { PILL_H } / 2.0;
                for (index, a) in names.iter().enumerate() {
                    for b in &names[index + 1..] {
                        let (ax, ay) = sl[a];
                        let (bx, by) = sl[b];
                        let ra = if *a == "pill" { form_r } else { BTN / 2.0 };
                        let rb = if *b == "pill" { form_r } else { BTN / 2.0 };
                        if (*a == "pill" || *b == "pill") && !circle {
                            let dx = ((ax - bx).abs() - PILL_W / 2.0 - BTN / 2.0).max(0.0);
                            assert!(
                                dx > 0.0 || (ay - by).abs() > PILL_H / 2.0 + BTN / 2.0 - 1.0,
                                "{layout}/{dock}: {a} y {b}"
                            );
                        } else {
                            let distance = ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt();
                            assert!(distance >= ra + rb, "{layout}/{dock}: {a} y {b}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_call_button_keeps_the_same_spacing_as_the_others() {
        for (layout, docks) in DOCKS {
            let circle = layout == "circle";
            for &dock in docks {
                for mic in [1.0, 0.0] {
                    let sl = slots(layout, mic, Some(dock));
                    let row_y = sl["hang"].1;
                    let mut names: Vec<&str> = sl
                        .iter()
                        .filter(|(name, point)| {
                            (point.1 - row_y).abs() < 0.01 && !(**name == "mic" && mic == 0.0)
                        })
                        .map(|(name, _)| *name)
                        .collect();
                    names.sort_by(|a, b| sl[a].0.total_cmp(&sl[b].0));
                    let half = |name: &str| {
                        if name == "pill" {
                            (if circle { CIRCLE_D } else { PILL_W }) / 2.0
                        } else {
                            BTN * if name == "mic" { mic } else { 1.0 } / 2.0
                        }
                    };
                    for pair in names.windows(2) {
                        let (a, b) = (pair[0], pair[1]);
                        let gap = sl[b].0 - half(b) - (sl[a].0 + half(a));
                        let expected = neighbor_gap(a, b, circle);
                        assert!(
                            (gap - expected).abs() < 1e-6,
                            "{layout}/{dock}/{mic}: {a}-{b} {gap} != {expected}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn when_nothing_fits_the_least_bad_dock_is_returned() {
        let tiny = (0, 0, 120, 90);
        let dock = choose_dock("circle", None, (60.0, 45.0), tiny, 1.0);
        assert!(docks("circle").contains(&dock));
        let best = docks("circle")
            .iter()
            .map(|dock| window_overflow("circle", dock, (60.0, 45.0), tiny, 1.0, 0.0))
            .fold(f64::INFINITY, f64::min);
        close(
            window_overflow("circle", dock, (60.0, 45.0), tiny, 1.0, 0.0),
            best,
        );
    }
}

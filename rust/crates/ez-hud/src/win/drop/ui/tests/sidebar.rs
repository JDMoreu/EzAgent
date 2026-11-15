//! Barra lateral de la tarjeta de ajustes (P-FIX-SIDEBAR): al cambiar de
//! sección solo queda marcada la nueva; ningún hover se queda encendido sin el
//! puntero encima.

use std::sync::Arc;

use crate::layout::SIDEBAR_ORDER;
use crate::paint::settings_window as sw;
use crate::win::drop::ui::{DropUI, PointerPhase};

use super::support::Harness;

/// Resaltado visible de una fila de la barra lateral.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Normal,
    Hover,
    Selected,
}

/// Fondo (BGRA) de una fila de la barra lateral en un fotograma.
fn row_pixel(ui: &Arc<DropUI>, row: usize) -> [u8; 4] {
    let frame = ui.render_card();
    let (_, y0, _, y1) = sw::item_rect(row as i32);
    let at = (((y0 + y1) / 2.0) as usize * frame.width as usize + 160) * 4;
    frame.data[at..at + 4].try_into().expect("4 canales")
}

/// Colores del fondo de cada fila: sin marca, con hover y con la sección activa.
fn row_colors(ui: &Arc<DropUI>) -> Vec<([u8; 4], [u8; 4], [u8; 4])> {
    let rows = SIDEBAR_ORDER.len();
    let capture = |section: &str, hover: Option<&str>, row: usize| {
        *ui.section.lock() = section.to_string();
        *ui.sidebar_hover.lock() = hover.map(str::to_string);
        row_pixel(ui, row)
    };
    let colors = (0..rows)
        .map(|row| {
            let next = SIDEBAR_ORDER[(row + 1) % rows];
            let other = SIDEBAR_ORDER[(row + 2) % rows];
            (
                capture(next, Some(other), row),
                capture(next, Some(SIDEBAR_ORDER[row]), row),
                capture(SIDEBAR_ORDER[row], None, row),
            )
        })
        .collect();
    *ui.section.lock() = String::from(SIDEBAR_ORDER[0]);
    *ui.sidebar_hover.lock() = None;
    colors
}

/// Cómo se ve cada fila de la barra lateral ahora mismo.
fn marks(ui: &Arc<DropUI>, colors: &[([u8; 4], [u8; 4], [u8; 4])]) -> Vec<Mark> {
    colors
        .iter()
        .enumerate()
        .map(|(row, (normal, hover, selected))| {
            let pixel = row_pixel(ui, row);
            if pixel == *selected {
                Mark::Selected
            } else if pixel == *hover {
                Mark::Hover
            } else if pixel == *normal {
                Mark::Normal
            } else {
                panic!("fila {row} con un color inesperado: {pixel:?}");
            }
        })
        .collect()
}

/// Centro y fila de cada sección en la barra lateral.
fn center(name: &str) -> (i32, i32, usize) {
    let index = SIDEBAR_ORDER
        .iter()
        .position(|candidate| *candidate == name)
        .expect("sección");
    let (x0, y0, x1, y1) = sw::item_rect(index as i32);
    (((x0 + x1) / 2.0) as i32, ((y0 + y1) / 2.0) as i32, index)
}

#[test]
fn changing_section_leaves_only_the_new_one_marked() {
    let harness = Harness::new();
    harness.open_card();
    // Sin fundido: el alto del menú iguala al del contenido.
    harness.menus.menu().lock().state.lock().height = 600.0;
    let ui = &harness.ui;
    // Movimiento sin botón: lo único que entrega la ventana (card_hand/cursor).
    let plain_move = |x: i32, y: i32| {
        let _ = ui.card_hand(x, y);
        let _ = ui.card_cursor(x, y);
    };
    let colors = row_colors(ui);
    let only_selected = |name: &str| {
        let (_, _, index) = center(name);
        let shown = marks(ui, &colors);
        assert_eq!(shown[index], Mark::Selected, "{name}: {shown:?}");
        assert!(
            shown
                .iter()
                .enumerate()
                .all(|(other, mark)| other == index || *mark == Mark::Normal),
            "{name}: {shown:?}"
        );
    };

    let (ax, ay, _) = center("appearance");
    let (bx, by, _) = center("listening");
    let (cx, cy, _) = center("voice");

    // A: el puntero llega, pulsa y suelta.
    plain_move(ax, ay);
    assert_eq!(ui.sidebar_hover.lock().as_deref(), Some("appearance"));
    assert!(ui.card_pointer(PointerPhase::Down, ax, ay));
    assert!(ui.card_pointer(PointerPhase::Up, ax, ay));
    only_selected("appearance");

    // B: el puntero se mueve sin pulsar (queda el hover de B) y pulsa.
    plain_move(bx, by);
    assert_eq!(ui.sidebar_hover.lock().as_deref(), Some("listening"));
    assert!(ui.card_pointer(PointerPhase::Down, bx, by));
    // La sección anterior ya no conserva ningún resaltado.
    only_selected("listening");
    assert!(ui.card_pointer(PointerPhase::Up, bx, by));
    only_selected("listening");

    // C: ídem, y al salir de la lista no queda ningún hover encendido.
    plain_move(cx, cy);
    assert!(ui.card_pointer(PointerPhase::Down, cx, cy));
    only_selected("voice");
    assert!(ui.card_pointer(PointerPhase::Up, cx, cy));
    plain_move(300, cy);
    assert_eq!(*ui.sidebar_hover.lock(), None);
    only_selected("voice");
}

#[test]
fn the_card_opens_without_a_stale_sidebar_hover() {
    let harness = Harness::new();
    harness.open_card();
    let _ = harness.ui.card_hand(100, 120);
    assert_eq!(
        harness.ui.sidebar_hover.lock().as_deref(),
        Some("listening")
    );
    harness.menus.menu().lock().state.lock().close_immediately = true;
    harness.ui.close_menu();
    harness.open_card();
    assert_eq!(*harness.ui.sidebar_hover.lock(), None);
}

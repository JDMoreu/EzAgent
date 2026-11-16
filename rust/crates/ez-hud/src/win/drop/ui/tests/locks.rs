//! `P-FIX-LOCKAUDIT`: la barra no retiene sus locks mientras mueve o para una
//! ventana de otro hilo. Los dobles comprueban, como el hilo de la ventana real
//! (que pide esos locks desde un gancho o desde `render_card`), que el lock
//! estaba libre al llegar la llamada.

use std::sync::atomic::Ordering;

use super::support::*;

/// Con el lock de la tarjeta tomado, `resize`/`set_pos`/`show` dejan al hilo
/// que pinta la tarjeta esperando: era el mismo abrazo mortal que la vista
/// rápida (`Peek::place`). `place_card` los aplica ahora fuera del lock.
#[test]
fn placing_the_card_does_not_hold_its_lock() {
    let harness = Harness::new();
    harness.open_card();
    let card = harness.windows.card();
    card.watch_card.store(true, Ordering::SeqCst);

    // Un DPI nuevo recoloca la tarjeta con la ventana ya creada.
    harness.ui.set_scale(1.5);

    assert!(
        !card.card_taken.load(Ordering::SeqCst),
        "`place_card` movió la tarjeta con su lock tomado"
    );
}

/// El menú avisa de que terminó de plegarse desde dentro de `update` (o del
/// cierre), con su lock tomado; el aviso paraba la tarjeta en ese mismo hilo.
/// El hilo de la ventana, en un gancho del puntero, pide ese lock: se cuelga la
/// interfaz. `menu_closed` la para en su propio hilo.
#[test]
fn closing_the_menu_stops_the_card_without_the_menu_lock() {
    let harness = Harness::new();
    harness.open_card();
    let card = harness.windows.card();
    harness.menus.menu().lock().state.lock().close_immediately = true;

    harness.ui.close_menu();
    wait_for(&card.stopped, "el stop de la tarjeta");

    assert!(
        !card.menu_taken.load(Ordering::SeqCst),
        "la tarjeta se paró con el lock del menú tomado"
    );
}

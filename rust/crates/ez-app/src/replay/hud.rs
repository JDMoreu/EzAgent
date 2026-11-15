//! HUD sin ventana (`RecordingHud` y `FakeRoot` de `run_py.py`).
//!
//! La barra de `ez-hud` se construye con su modelo real, pero su fábrica de
//! ventanas devuelve un doble que nunca abre nada: `start` dice que no hay
//! sesión gráfica y el resto son no-ops. Así la app cree tener barra y el
//! arnés puede correr sin escritorio.

use std::sync::{Arc, Weak};

use ez_hud::win::drop::{DropUI, HudWindow, WindowFactory};

/// Fábrica que nunca abre una ventana nativa.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoWindows;

impl WindowFactory for NoWindows {
    fn bar(
        &self,
        _ui: Weak<DropUI>,
        _x: i32,
        _y: i32,
        _width: i32,
        _height: i32,
    ) -> Arc<dyn HudWindow> {
        Arc::new(NoWindow)
    }

    fn card(
        &self,
        _ui: Weak<DropUI>,
        _x: i32,
        _y: i32,
        _width: i32,
        _height: i32,
    ) -> Arc<dyn HudWindow> {
        Arc::new(NoWindow)
    }
}

/// Ventana nula: acepta todo y no dibuja nada.
pub struct NoWindow;

impl HudWindow for NoWindow {
    fn start(&self) -> bool {
        // `false` = no hay sesión gráfica; `DropUI.show` se queda en el modelo.
        false
    }

    fn show(&self) {}

    fn hide(&self) {}

    fn stop(&self) {}

    fn set_pos(&self, _x: i32, _y: i32) {}

    fn resize(&self, _width: i32, _height: i32) {}

    fn position(&self) -> (i32, i32) {
        (0, 0)
    }

    fn size(&self) -> (i32, i32) {
        (0, 0)
    }

    fn is_visible(&self) -> bool {
        false
    }
}

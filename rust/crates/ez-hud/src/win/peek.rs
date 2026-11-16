//! Port of `src/ezagent/voice/hud/peek.py` (F5-06).
//!
//! Vista rápida de la actividad: una tarjeta compacta que nace de la forma. Al
//! reposar el puntero sobre la forma, o al llegar un desenlace nuevo, una
//! tarjeta pequeña sube desde la barra compartiendo su material. Las filas
//! siguen la prioridad (needs you → failed → working → waiting → stale → done);
//! cada una lleva su marca de estado, la tarea y «estado · agente · tiempo». No
//! hay cabecera, chincheta ni botón de cerrar: la tarjeta se va cuando se va el
//! puntero (tras una gracia corta para poder alcanzarla), y un clic en una fila
//! trae al frente la terminal de ese agente.
//!
//! Hilos: `tick` corre en el hilo de pintura de la barra, `refresh`/`toast` en
//! el hilo de Tk y `render` en el hilo de pintura de la tarjeta. El estado
//! compartido vive tras un `Mutex`; la capa estática se cachea y solo las marcas
//! se mueven por cuadro.

mod controller;
mod frame;
mod surface;
mod ui;
mod window;

pub use controller::Peek;
pub use frame::{compose, place_card};
pub use ui::PeekUi;
pub use window::PeekWindow;

pub use crate::paint::peek::{
    age, card_height, ease_out, fit, meta_line, paint_marks, paint_rows, CLOSE_SECS, DISMISS_W,
    FS_META, FS_TASK, GAP, HOVER_DELAY, LEAVE_GRACE, MARGIN, MAX_ROWS, OPEN_SECS, PAD_X, PAD_Y,
    PEEK_W, REVEAL_SECS, RISE, ROW_H, TOAST_SECS,
};

#[cfg(test)]
mod tests;

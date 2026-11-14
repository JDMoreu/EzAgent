//! Port of `src/ezagent/voice/hud/drop.py 250-362, 553-844` (F5-04).
//!
//! Pintura de la barra: la forma fija con la Marea dentro, los botones
//! redondos, el tinte de la pausa y el cuerpo de la tarjeta de ajustes.
//! Devuelve buffers (sin Win32): la ventana que los presenta es F5-05.
//!
//! La geometría y la animación viven en [`crate::model::drop`] (F5-03); aquí
//! solo se dibuja el fotograma que ese modelo describe.

mod canvas;
mod card;
mod colors;
mod fluid;
mod render;
mod sprite;

pub use card::render_card_body;
pub use colors::{Ui, SIGNAL, UI};
pub use fluid::{form_mask, render_fluid};
pub use render::{render_model, RenderOptions};
pub use sprite::{button_sprite, SpriteOptions};

/// Lado lógico del icono dentro de un botón de 36 px.
pub const BAR_ICON: f64 = 16.0;
/// Cuánto se encoge la forma al pausar el micrófono (0..1).
pub const MUTE_SHRINK: f64 = 0.14;
/// Color que conserva el interior en la pausa (0 = gris puro).
pub const MUTE_KEEP: f64 = 0.10;
/// Brillo que conserva el interior en la pausa.
pub const MUTE_DIM: f64 = 0.92;

#[cfg(test)]
mod tests;

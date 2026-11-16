//! Port of `src/ezagent/voice/hud/peek_paint.py` (F5-06).
//!
//! Dibujo de la vista rápida: medidas, textos y capas (sin Win32). Una tarjeta
//! compacta que nace de la forma; las filas siguen la prioridad de la actividad
//! y cada una lleva su marca de estado, la tarea y «estado · agente · tiempo».
//!
//! Los iconos del HUD viven en `ez-gfx::icons` (F2-11, aún esqueleto): aquí se
//! reciben por el trait [`IconPainter`], como pide el brief.

use crate::paint::marks;
use crate::tokens::{Color, FontStyle};

mod body;
mod draw;
mod spin;
mod text;

pub use draw::{paint_marks, paint_rows};
pub use spin::{spinner, SPIN_STEPS};
pub use text::{age, baselines, card_height, dismiss_at, fit, meta_line, row_at};

// Medidas en unidades de la barra (se escalan con ella, ``DropUI.bs``): una fila mide lo
// que la píldora (38) y el radio es su mitad, así que un aviso de una fila es una cápsula
// hermana de la píldora; el ancho queda entre la píldora (108) y la fila completa (~284).
/// Ancho lógico de la tarjeta.
pub const PEEK_W: f64 = 232.0;
/// Alto de una fila (= `PILL_H`).
pub const ROW_H: f64 = 38.0;
pub const PAD_X: f64 = 10.0;
pub const PAD_Y: f64 = 0.0;
/// Columna del indicador de estado.
pub const MARK_COL: f64 = 24.0;
/// Radio de la cápsula (= `PILL_H / 2`).
pub const RADIUS: f64 = 19.0;
/// El resaltado de la fila bajo el puntero, dentro de la cápsula.
pub const ROW_INSET: f64 = 3.0;
/// De la línea base del título a la de la línea de estado.
pub const LINE_GAP: f64 = 13.0;
pub const MARK_ICON: f64 = 15.0;
/// Aire alrededor de la tarjeta para la sombra.
pub const MARGIN: f64 = 14.0;
/// Separación entre la forma y la tarjeta.
pub const GAP: f64 = 10.0;
pub const MAX_ROWS: usize = 4;
/// Zona de la × al final de la fila (px lógicos).
pub const DISMISS_W: f64 = 26.0;
pub const MORE_H: f64 = 22.0;
/// Reposo sobre la forma antes de abrir (s).
pub const HOVER_DELAY: f64 = 0.28;
/// Margen para alcanzar la tarjeta al salir de la forma (s).
pub const LEAVE_GRACE: f64 = 0.35;
pub const OPEN_SECS: f64 = 0.2;
pub const CLOSE_SECS: f64 = 0.13;
/// Cuánto se desplaza al aparecer (px lógicos).
pub const RISE: f64 = 6.0;
/// Segundos que un aviso permanece, por estado; el resto usa 3.5.
pub const TOAST_SECS: [(&str, f64); 4] = [
    (marks::DONE, 3.2),
    (marks::NEEDS_YOU, 6.0),
    (marks::FAILED, 6.0),
    (marks::STALE, 4.5),
];
/// «¿qué tareas tengo pendientes?»: la lista completa un rato (s).
pub const REVEAL_SECS: f64 = 8.0;

/// `FS_TASK`.
pub const FS_TASK: FontStyle = FontStyle::new("FS_TASK", "Geist", 12.5, 500);
/// `FS_META`.
pub const FS_META: FontStyle = FontStyle::new("FS_META", "Geist", 10.5, 400);

/// Nombres de agente que se muestran con mayúsculas fijas.
pub const AGENT_NAMES: [(&str, &str); 4] = [
    ("opencode", "OpenCode"),
    ("codex", "Codex"),
    ("claude", "Claude"),
    ("enjambre", "Enjambre"),
];

/// Palabras de estado de `activity.py` (`ActivityItem.label`).
pub const STATE_LABEL: [(&str, &str); 6] = [
    (marks::NEEDS_YOU, "Necesita tu respuesta"),
    (marks::FAILED, "Falló"),
    (marks::WORKING, "En curso"),
    (marks::WAITING, "En espera"),
    (marks::STALE, "Sin actualización reciente"),
    (marks::DONE, "Completada"),
];

/// Segundos de un aviso para `state` (`TOAST_SECS.get(state, 3.5)`).
pub(crate) fn toast_secs(state: &str) -> f64 {
    TOAST_SECS
        .iter()
        .find(|(name, _)| *name == state)
        .map_or(3.5, |(_, seconds)| *seconds)
}

/// `ActivityItem` de `activity.py`, con los campos que consume la vista rápida.
///
// DECISION: `ez-hud` no depende de `ez-herdr` (allí vive `ActivityItem`), así
// que el ítem de la vista se define aquí con los mismos campos y semántica.
#[derive(Clone, Debug, PartialEq)]
pub struct PeekItem {
    pub key: String,
    pub title: String,
    /// Agente de la fila; la fila `autonomous` lleva aquí su estado o motivo.
    pub agent: String,
    pub state: String,
    pub since: f64,
    pub unseen: bool,
    pub live: bool,
}

impl PeekItem {
    /// `ActivityItem.label`.
    pub fn label(&self) -> &'static str {
        STATE_LABEL
            .iter()
            .find(|(state, _)| *state == self.state)
            .map_or("", |(_, label)| *label)
    }

    /// `ActivityItem.marked`: lo vivo, y lo terminado que aún no se ha visto.
    pub fn marked(&self) -> bool {
        self.live || self.unseen
    }
}

/// `dismissible(item)`: todo menos un enjambre en marcha y la fila del modo
/// autónomo, que solo se quita deteniendo la tarea (10.19).
pub fn dismissible(item: &PeekItem) -> bool {
    !(item.live && item.key.starts_with("swarm:")) && item.state != marks::AUTONOMOUS
}

/// Búfer RGBA premultiplicado en `float32` (el `np.ndarray` del Python).
#[derive(Clone, Debug, PartialEq)]
pub struct PeekBuffer {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl PeekBuffer {
    /// Búfer transparente de `width`×`height`.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![0.0; width * height * 4],
        }
    }

    /// Un píxel premultiplicado, en orden RGBA.
    pub fn pixel(&self, x: usize, y: usize) -> [f32; 4] {
        let index = (y * self.width + x) * 4;
        [
            self.data[index],
            self.data[index + 1],
            self.data[index + 2],
            self.data[index + 3],
        ]
    }
}

/// Rasterizador de iconos del HUD (`ez-gfx::icons`, F2-11).
///
/// El brief permite recibir la máscara por parámetro mientras los iconos no
/// estén portados; el trait evita duplicar aquí la familia de glifos.
pub trait IconPainter: Send + Sync {
    /// Dibuja `name` centrado en `(cx, cy)` (px lógicos) sobre `buf`.
    #[allow(clippy::too_many_arguments)] // firma del Python (`draw_icon`)
    fn draw_icon(
        &self,
        buf: &mut PeekBuffer,
        name: &str,
        cx: f64,
        cy: f64,
        size: f64,
        color: Color,
        scale: f64,
    );
}

/// Proveedor de iconos que no dibuja nada (pruebas y arranque sin iconos).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoIcons;

impl IconPainter for NoIcons {
    fn draw_icon(
        &self,
        _buf: &mut PeekBuffer,
        _name: &str,
        _cx: f64,
        _cy: f64,
        _size: f64,
        _color: Color,
        _scale: f64,
    ) {
    }
}

/// Curva de salida de la animación.
pub fn ease_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[cfg(test)]
mod tests;

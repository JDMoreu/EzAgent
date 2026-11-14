//! Port of `src/ezagent/voice/hud/drop.py 62-552` (F5-03).
//!
//! Geometría de la barra y `DropModel`: rectángulos, centros de los botones,
//! recorte de clics por geometría y la animación del modelo. Lógica pura, sin
//! Win32 ni dibujo (el fluido se pinta en F5-04 y las ventanas son F5-05).

mod fluid;
mod geometry;
mod model;
mod palette;

pub use fluid::{FlowState, ATTACK, FLOW_SPEED, RELEASE, RISE, SPIN, WAVE_SPEED};
pub use geometry::{
    choose_dock, clean_dock, default_dock, docks, emerge_order, form_center, layout_size,
    row_center_y, slots, window_overflow, Point, Rect, Slots, WorkArea, BTN, BUD_SS, BUTTONS,
    CIRCLE_D, DEFAULT_DOCK, DOCKS, DOUBLE_CLICK, FPS_BUSY, FPS_CALM, GAP, GRID_CELL, HOVER_HOLD,
    HOVER_MARGIN, HYSTERESIS, LAYOUTS, MOTION_MODES, NB, ORB_GAP, PAD, PILL_H, PILL_W, SHADOW,
};
pub use model::{DropModel, Update};
pub use palette::{colors_for, Palette, CUSTOM_POSES};

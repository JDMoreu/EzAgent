//! Port of `src/ezagent/voice/hud/layered.py` and
//! `src/ezagent/voice/hud/frame_stats.py` (F5-01).
//!
//! Ventanas *layered* de 64 bits con transparencia real por píxel: creación,
//! bucle de mensajes, DPI por monitor, selección de monitor y entrada (puntero,
//! teclado, rueda y arrastre), más las estadísticas de cuadro. El dibujo es de
//! `paint/`: aquí solo se presenta un [`RgbaFrame`] y se reparten eventos.
//!
//! El código nativo solo aparece en este grupo de módulos (AGENTS.rust.md §5),
//! y cada bloque lleva su comentario `// SAFETY:`.

pub mod dpi;
pub mod drag;
pub mod frame_stats;
pub mod keyboard;
pub mod layered;
pub mod monitor;
pub mod pointer;
pub mod wheel;
pub mod wndproc;

pub use frame_stats::{FrameStats, BUDGET_MS, REPORT_SECS};
pub use layered::{LayeredWindow, RgbaFrame, WindowEvent, WindowHooks, WindowOptions, FPS_ACTIVE};
pub use monitor::{clamp_pos, monitor_rect, taskbar_rect, work_area, Rect};
pub use pointer::HitRegion;

use std::sync::LazyLock;
use std::time::Instant;

/// `available()`: `true` si se puede crear chrome flotante en este proceso.
///
/// DECISION: los prototipos de `ctypes` ya no existen (los declara el crate
/// `windows`), así que en Windows el resultado es siempre `true`.
pub fn available() -> bool {
    cfg!(windows)
}

/// `time.monotonic()`: segundos desde un origen arbitrario y monótono.
pub fn monotonic_seconds() -> f64 {
    static ORIGIN: LazyLock<Instant> = LazyLock::new(Instant::now);
    ORIGIN.elapsed().as_secs_f64()
}

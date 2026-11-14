//! Port of `src/ezagent/voice/hud/drop.py 845-2246` (F5-05).
//!
//! Las dos ventanas del HUD y su controlador:
//!
//! * [`bar::DropWindow`]: la barra (forma, botones, arrastre y rueda);
//! * [`card::CardWindow`]: la tarjeta de ajustes, con su barra lateral y el
//!   menú dentro;
//! * [`ui::DropUI`]: el contrato que `ez-app` usa (`show`, `hide`,
//!   `set_state`…), el autómata del puntero y la colocación de la tarjeta.
//!
//! La pintura y la animación ya existen en [`crate::paint::drop`] y
//! [`crate::model::drop`]; aquí solo se presenta un fotograma y se reparten
//! eventos. La vista rápida ([`crate::win::peek`]) y el chat
//! ([`ui::ChatPanel`]) se consumen por sus contratos.
//!
//! Hilos: cada ventana tiene su hilo de pintura (`ezagent-drop-paint`,
//! `ezagent-card-paint`) y `layered` el suyo de mensajes. `DropUI` no bloquea al
//! llamador: sus operaciones solo publican estado bajo locks cortos y el hilo
//! de pintura lo aplica en el cuadro siguiente (cola de un elemento). Esta es
//! la zona de `ez-hud` donde el contrato permite `unsafe`: aquí solo el registro
//! de Windows que lee el tema del sistema, con su `// SAFETY:`.

pub mod bar;
pub mod card;
pub mod ui;

pub use bar::DropWindow;
pub use card::CardWindow;
pub use ui::{
    clean_bar_scale, create_ui, ActivityItem, BarFrame, BoolCallback, CardFrame, CardKey, CardSlot,
    CardText, ChatPanel, ConfigValue, CopyCallback, DropConfig, DropHost, DropUI, DropUiOptions,
    FluidBuffer, HudWindow, LiveCallback, MenuCallbacks, MenuFactory, MenuPanel,
    MenuSettingsProvider, NativeWindowFactory, NoChat, PointerPhase, SaveCallback, TestCallback,
    TextCallback, VoidCallback, WindowFactory, ORBIT_LIMIT,
};

/// `time.monotonic()`: segundos desde un origen arbitrario y monótono.
pub use crate::win::monotonic_seconds as monotonic;

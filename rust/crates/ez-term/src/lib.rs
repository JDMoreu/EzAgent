//! Cliente JSON-RPC de herdr-core (`hc connect`). Diseño:
//! `docs/designs/espacio-elefante.md` §9.
//!
//! Un mensaje JSON por línea. Las peticiones van por una conexión con un hilo
//! lector que empareja cada respuesta por `id`; los eventos usan una conexión
//! aparte porque `hc` deja de responder peticiones mientras está suscrito.
//! Todo es síncrono: hilos y canales, sin runtime asíncrono.

mod client;
mod conn;
mod error;
mod events;
mod locate;
mod types;

pub use client::HcClient;
pub use error::TermError;
pub use events::{EventStream, TermEvent};
pub use locate::locate_hc;
pub use types::{Delivery, ReadText, RunSpec, SendOutcome, Status, Terminal};

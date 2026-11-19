//! Árbitro del espacio de trabajo: tareas, rondas, reglas, verificación y traza.
//!
//! Especificación: `docs/designs/espacio-elefante.md` §8 (reglas R1–R12) y
//! `rust-migration/briefs/ECO-2.md`.
//!
//! ```no_run
//! use std::path::PathBuf;
//! use std::sync::Arc;
//! use ez_arbiter::{Arbiter, Clock, FishLauncher, InstructionWriter, SystemClock};
//!
//! # struct Launcher;
//! # impl FishLauncher for Launcher {
//! #     fn launch(&self, _: &ez_arbiter::LaunchRequest) -> Result<ez_arbiter::Launched, String> {
//! #         Ok(ez_arbiter::Launched { terminal: "t7".into(), delivery: ez_arbiter::Delivery::Delivered })
//! #     }
//! #     fn send(&self, _: &str, _: &str) -> Result<(), String> { Ok(()) }
//! #     fn alive(&self, _: &str) -> bool { true }
//! #     fn revoke(&self, _: &str) {}
//! # }
//! # struct Writer;
//! # impl InstructionWriter for Writer {
//! #     fn render(&self, _: &ez_team::InstructionContext) -> Result<String, String> { Ok(String::new()) }
//! #     fn first_line(&self, _: ez_team::Role, _: &std::path::Path) -> String { String::new() }
//! # }
//! let arbiter = Arbiter::new(
//!     PathBuf::from("datos"),
//!     Arc::new(Launcher),
//!     Arc::new(Writer),
//!     Arc::new(SystemClock),
//! ).expect("abrir el árbitro");
//! ```

mod arbiter;
mod criteria;
mod events;
mod exec;
mod git;
mod paths;
mod state;
mod texts;
mod types;
mod verify;

pub use arbiter::Arbiter;
pub use events::{ArbiterEvent, Card, CardKind, ElephantEvent, EventKind};
pub use types::{
    Caller, Clock, Command, Delivery, FishKey, FishLauncher, InstructionWriter, LaunchRequest,
    Launched, Outcome, Rejection, Space, SystemClock, Verdict,
};

/// Resultado de una comprobación de un criterio (logueable y serializable).
pub use verify::CheckRecord;

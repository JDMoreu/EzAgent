//! Agentes, suscripciones, inventario, equipo y textos de los peces.
//! Diseño: `docs/designs/espacio-elefante.md` §6.

pub mod contract;
pub mod roles;

pub use contract::{InstructionContext, Seat};
pub use roles::Role;

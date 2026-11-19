//! Agentes, suscripciones, inventario, equipo y textos de los peces.
//! Diseño: `docs/designs/espacio-elefante.md` §6.

pub mod contract;
pub mod error;
pub mod family;
pub mod inventory;
pub mod launch;
pub mod profiles;
pub mod roles;
pub mod subscriptions;
pub mod team;

#[cfg(test)]
mod test_support;

pub use contract::{InstructionContext, Seat};
pub use error::TeamError;
pub use inventory::{AvailableAgent, Inventory, RealSystem, System};
pub use profiles::AgentProfile;
pub use roles::Role;
pub use subscriptions::Subscription;
pub use team::TeamCheck;

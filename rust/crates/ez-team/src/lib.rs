//! Agentes, suscripciones, inventario, equipo y textos de los subagentes.
//! Diseño: `docs/designs/espacio-elefante.md` §6 y §15.

pub mod contract;
pub mod error;
pub mod family;
pub mod inventory;
pub mod launch;
pub mod profiles;
pub mod role_text;
pub mod roles;
pub mod subscriptions;
pub mod team;

#[cfg(test)]
mod test_support;

pub use contract::{InstructionContext, Seat};
pub use error::TeamError;
pub use inventory::{AvailableAgent, Inventory, RealSystem, System};
pub use launch::LaunchCommand;
pub use profiles::AgentProfile;
pub use role_text::{builtin_roles, library_dir, load_roles, roles_json, roles_text};
pub use roles::{builtin, RoleDef, RoleId, RoleKind, RoleOrigin};
pub use subscriptions::Subscription;
pub use team::TeamCheck;

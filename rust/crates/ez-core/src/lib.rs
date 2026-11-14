//! Crate skeleton created by F0-04.

// DECISION: `casefold` vive en `ez-stt` (F2-06) y `ez-core` no puede depender de
// él sin tocar `Cargo.toml`; se compila el mismo archivo aquí para no duplicar
// la tabla. Si el orquestador lo mueve a `ez-core`, basta con borrar este `path`.
pub mod agent_config;
#[path = "../../ez-stt/src/casefold.rs"]
pub(crate) mod casefold;
pub mod config;
pub mod dirs;
pub mod downloads;
pub mod llm_providers;
pub mod logging;
pub mod metrics;
pub mod models;
pub mod paths;
pub mod permissions;
pub mod pyjson;
pub mod pyloads;
pub mod pytime;
pub mod pyvalue;
pub mod redact;
pub mod speech_text;

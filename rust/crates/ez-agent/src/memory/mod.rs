//! Memoria persistente: `USER.md`, `MEMORY.md` e historial SQLite con FTS5.
//!
//! Port de `src/ezagent/agent/memory.py` (F3-09a/b). Solo se guardan mensajes de
//! usuario y asistente; nunca resultados de herramientas. El historial conserva
//! 180 días.

pub mod ops;
pub mod recall;
pub mod schema;
pub mod store;

pub use recall::make_snippet;
pub use schema::{
    contains_web_content, find_sensitive_text, fold_text, normalize_entry, normalize_for_compare,
    normalize_kind, summarize_text, MemoryResult, DEFAULT_RECALL_LIMIT, MAX_ENTRY_CHARS,
    MEMORY_LIMIT, RECALL_FRAGMENT_CHARS, RETENTION_DAYS, USER_LIMIT,
};
pub use store::{Clock, MemoryOptions, MemoryStore, MemoryTimestamp};

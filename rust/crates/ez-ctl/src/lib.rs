//! Protocolo, servidor local y comando `ez`. Diseño: `docs/designs/espacio-elefante.md` §7.
//!
//! `ez-ctl` no depende de `ez-arbiter`: define el [`Handler`] y la integración
//! (ola 2) lo implementa con el árbitro.

mod cli;
mod client;
mod local;
mod protocol;
mod scan_cmds;
mod server;

pub use cli::{main_with, SKILL};
pub use local::{
    delegate, parse_delegate_options, summary, warnings, DelegateOptions, LocalDelegate,
    LocalError, MAX_BRIEF_BYTES,
};
pub use protocol::{new_token, Caller, Handler, Reply, Request, Tokens};
pub use server::{CtlServer, HOST, MAX_BODY_BYTES, MAX_WAITS, WORKERS};

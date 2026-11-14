//! Servicios falsos compartidos por la app Python y la de Rust (F1-11).
//!
//! - [`fake_llm`]: OpenAI Chat Completions y Anthropic Messages con SSE.
//! - [`fake_whisper`]: `whisper-server` (`/inference` multipart).
//! - [`fake_hc`]: `hc`/`herdr-app` con estado en un JSON.
//! - [`fake_agent`]: pantallas de agente guionizadas (trabaja, termina, bloqueo,
//!   límite de uso, contexto lleno).

pub mod fake_agent;
pub mod fake_hc;
pub mod fake_llm;
pub mod fake_whisper;

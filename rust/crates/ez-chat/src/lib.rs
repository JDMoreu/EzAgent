//! Modo chat de EzAgent: conversación escrita, historial y contenido enriquecido.
//!
//! Puerto de `src/ezagent/voice/chat/` (F3-14): historial persistente
//! ([`store`]), parser de markdown ([`markdown`]), acceso seguro a imágenes y
//! documentos ([`media`]) y el estado visible de la conversación ([`session`]).
//! La ventana que los dibuja vive en `ez-hud` (F5-13).

pub mod contexts;
pub mod markdown;
pub mod media;
pub mod session;
pub mod speech;
pub mod store;
pub mod text_input;
pub mod trace;

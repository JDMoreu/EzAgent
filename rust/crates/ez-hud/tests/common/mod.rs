//! Helpers compartidos de las pruebas de paridad del HUD (P-HUD-A).
//!
//! `ez-hud` no declara `serde_json` y el brief prohíbe tocar `Cargo.toml`; el
//! valor JSON se nombra con la proyección de tipo de
//! `ez_core::pyjson::OrderedObject` y se construye con `pyloads::loads`.

#![allow(dead_code)]

pub mod bar;
pub mod bar_cases;
pub mod drop;
pub mod geometry;
pub mod json;

pub use ez_parity::fixtures::{self, FixtureCase};

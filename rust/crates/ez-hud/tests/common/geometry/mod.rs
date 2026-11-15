//! Reparto de los casos de `hud_geometry` por submódulo.

pub mod basic;
pub mod chat;
pub mod menu;
pub mod tokens;

use crate::common::json::Json;
use crate::common::FixtureCase;

pub fn run(case: &FixtureCase) -> Result<Json, String> {
    if case.function.starts_with("chat_geometry.") || case.function.starts_with("layered.") {
        chat::run(case)
    } else if case.function.starts_with("menu_layout.") {
        menu::run(case)
    } else {
        basic::run(case)
    }
}

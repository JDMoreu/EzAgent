//! Pruebas del bucle de herramientas y de las utilidades compartidas del
//! `Messenger` (F4-07/F4-08). Los dobles viven en `support` y cada área en su
//! submódulo.

mod support;
pub(crate) use support::*;

mod budgets;
mod lifecycle;
mod text;
mod tools;
mod waits;

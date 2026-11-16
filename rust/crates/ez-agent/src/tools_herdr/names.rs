//! Resolución de nombres dictados en las herramientas de Herdr.
//!
//! Mejora deliberada sobre el Python (ver `rust-migration/python-quirks.md`):
//! el nombre que llega del reconocimiento de voz se compara con lo abierto en
//! Herdr antes de usarlo. El texto de la pregunta y de la nota los escribe este
//! módulo, nunca el modelo.

use std::cell::RefCell;

use ez_core::pyvalue::py_strip;
use ez_herdr::herdr::{candidate_labels, resolve_name, NameHit, NameKind, NameMatch};
use serde_json::Value;

use super::args::{ArgumentError, NameConfirm};
use super::port::HerdrPort;
use crate::registry::ToolResult;

/// Clave del resultado donde se cuenta el nombre usado («usé el espacio «TEST»»).
pub const NOTE_FIELD: &str = "name_note";

/// Resolvedor de una llamada: acumula las notas para el resultado.
pub struct Names<'a> {
    port: &'a dyn HerdrPort,
    confirm: Option<&'a NameConfirm>,
    notes: RefCell<Vec<String>>,
}

impl<'a> Names<'a> {
    pub fn new(port: &'a dyn HerdrPort, confirm: Option<&'a NameConfirm>) -> Self {
        Self {
            port,
            confirm,
            notes: RefCell::new(Vec::new()),
        }
    }

    /// Sustituye `spoken` por el nombre vivo, o falla con la lista de nombres.
    ///
    /// Sin candidatos que comparar se deja el texto tal cual: es lo que hacía
    /// el Python y lo que ven los dobles de prueba.
    pub fn pick(&self, kind: NameKind, spoken: &str) -> Result<String, ArgumentError> {
        let spoken = py_strip(spoken);
        let cached = self.port.name_candidates(kind, false);
        if let NameMatch::Exact(found) = resolve_name(spoken, &cached) {
            return Ok(found.value);
        }
        // Antes de preguntar o rendirse se mira la lista viva: la caché puede
        // no tener la terminal que se acaba de abrir.
        let live = self.port.name_candidates(kind, true);
        let candidates = if live.is_empty() { &cached } else { &live };
        let available = candidate_labels(candidates);
        match resolve_name(spoken, candidates) {
            NameMatch::Exact(found) | NameMatch::Confident(found) => Ok(self.use_it(kind, &found)),
            NameMatch::Doubtful(found) => self.ask(kind, spoken, &found, &available),
            NameMatch::Unknown { .. } if available.is_empty() => Ok(spoken.to_string()),
            NameMatch::Unknown { .. } => Err(ArgumentError::new(kind.unknown(spoken, &available))),
        }
    }

    /// Pregunta y, si acepta, sigue con el nombre resuelto.
    fn ask(
        &self,
        kind: NameKind,
        spoken: &str,
        found: &NameHit,
        available: &[String],
    ) -> Result<String, ArgumentError> {
        let question = kind.question(&found.label);
        let accepted = self.confirm.is_some_and(|confirm| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| confirm(&question)))
                .unwrap_or(false)
        });
        if accepted {
            self.notes.borrow_mut().push(kind.note(&found.label));
            return Ok(found.value.clone());
        }
        Err(ArgumentError::new(kind.unknown(spoken, available)))
    }

    /// Añade al resultado la nota del nombre que se usó sin coincidencia exacta.
    pub fn attach(&self, result: &mut ToolResult) {
        let notes = self.notes.borrow();
        if notes.is_empty() {
            return;
        }
        if let Value::Object(map) = &mut result.data {
            map.insert(NOTE_FIELD.to_string(), Value::String(notes.join("; ")));
        }
    }

    fn use_it(&self, kind: NameKind, found: &NameHit) -> String {
        self.notes.borrow_mut().push(kind.note(&found.label));
        found.value.clone()
    }
}

#[cfg(test)]
mod tests;

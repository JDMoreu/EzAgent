//! La pestaña del subagente en el espacio de su padre.
//!
//! `terminal.keep --place tab` abre la pestaña en el espacio que está a la
//! vista, no en el del padre. Herdr abre la pestaña donde dice la etiqueta
//! `herdr.space` del terminal: por eso el hijo recibe la del padre (sin
//! `herdr.tab`, que sería una pestaña fija) y la pestaña aparece en su
//! espacio sin mover el foco de la persona (verificado en vivo el 19-nov).

use std::time::Duration;

use serde_json::{Map, Value};

use super::super::{Herdr, HerdrError, Placement};
use super::{Placed, PLACE_RETRY_LIMIT_S, PLACE_RETRY_STEP_S};

/// Aviso que aparece cuando el padre no está en un espacio de Herdr.
const NO_SPACE_WARNING: &str = "El agente padre no está en un espacio de Herdr; \
                                 abrí el subagente en la pestaña del espacio a la vista.";

/// Aviso que aparece cuando Herdr no abre la pestaña en el espacio del padre.
const NO_TAB_WARNING: &str = "Herdr no abrió la pestaña en el espacio del padre; \
                               quedó en la del espacio a la vista.";

impl Herdr {
    /// Abre la pestaña del subagente en el espacio donde está su padre.
    ///
    /// Si el padre no tiene `herdr.space` (no se ve en Herdr), cae al
    /// `terminal.keep` de siempre con su aviso.
    pub(super) fn keep_child_in_parent_space(&self, terminal: &str, parent: &str) -> Placed {
        let Some(space) = self.parent_space(parent) else {
            let mut placed = self.keep_child(terminal, Placement::Tab);
            placed.warnings.insert(0, NO_SPACE_WARNING.to_string());
            return placed;
        };
        if let Err(error) = self.label_space(terminal, &space) {
            let mut placed = self.keep_child(terminal, Placement::Tab);
            placed.warnings.insert(
                0,
                format!(
                    "No pude abrir la pestaña en el espacio del padre ({}); \
                 quedó en la del espacio a la vista.",
                    error.message()
                ),
            );
            return placed;
        }
        match self.wait_for_tab(terminal, &space) {
            Some(tab) => {
                let mut extra = Map::new();
                extra.insert("tab".to_string(), Value::String(tab));
                Placed {
                    placement: Placement::Tab,
                    warnings: Vec::new(),
                    extra,
                }
            }
            None => {
                let mut placed = self.keep_child(terminal, Placement::Tab);
                placed.warnings.insert(0, NO_TAB_WARNING.to_string());
                placed
            }
        }
    }

    /// El espacio de Herdr donde está el padre (`herdr.space`).
    fn parent_space(&self, parent: &str) -> Option<String> {
        let data = self.hc_get(parent)?;
        let space = data.get("labels")?.get("herdr.space")?.as_str()?;
        let space = space.trim();
        if space.is_empty() {
            None
        } else {
            Some(space.to_string())
        }
    }

    /// `hc label <hijo> herdr.space=<espacio>`.
    fn label_space(&self, terminal: &str, space: &str) -> Result<(), HerdrError> {
        let args = vec![
            "label".to_string(),
            terminal.to_string(),
            format!("herdr.space={space}"),
        ];
        let completed = self.run(&args, 15.0)?;
        if completed.returncode != 0 {
            let stderr = completed.stderr.trim().to_string();
            let stdout = completed.stdout.trim().to_string();
            let message = if stderr.is_empty() { stdout } else { stderr };
            return Err(HerdrError::failed(if message.is_empty() {
                "hc label no devolvió nada".to_string()
            } else {
                message
            }));
        }
        Ok(())
    }

    /// Espera a que Herdr abra la pestaña del subagente en ese espacio.
    fn wait_for_tab(&self, terminal: &str, space: &str) -> Option<String> {
        let start = self.runtime.monotonic();
        loop {
            if let Some(tab) = self.tab_in_space(terminal, space) {
                return Some(tab);
            }
            if self.runtime.monotonic() - start >= PLACE_RETRY_LIMIT_S {
                return None;
            }
            std::thread::sleep(Duration::from_secs_f64(PLACE_RETRY_STEP_S));
        }
    }

    /// La pestaña del espacio donde está la terminal (`herdr-app ls`).
    fn tab_in_space(&self, terminal: &str, space: &str) -> Option<String> {
        let data = self.json_app(&["ls".to_string()], 20.0).ok()?;
        let spaces = data.get("spaces")?.as_array()?;
        for room in spaces {
            if room.get("id").and_then(Value::as_str) != Some(space) {
                continue;
            }
            let tabs = room.get("tabs").and_then(Value::as_array)?;
            for tab in tabs {
                let terminals = tab.get("terminals").and_then(Value::as_array);
                let inside = terminals.is_some_and(|items| {
                    items
                        .iter()
                        .any(|item| item.get("terminal").and_then(Value::as_str) == Some(terminal))
                });
                if inside {
                    return tab.get("id").and_then(Value::as_str).map(str::to_string);
                }
            }
            return None;
        }
        None
    }
}

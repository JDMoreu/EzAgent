//! Avisos de memoria baja del vigilante (`inventario-recursos.md` §4).
//!
//! El vigilante no mide memoria (no depende de `ez-scan`): quien lo arranca le
//! pasa un [`MemoryProbe`]. Cada vuelta se consulta; por debajo de los umbrales
//! se avisa al padre **una vez por episodio** y no se mata nada.

use std::sync::Arc;

use super::{Tick, Watcher};

/// La memoria medida cuando está por debajo de los umbrales.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryReading {
    pub free_ram_mb: u64,
    pub free_commit_mb: u64,
    /// El subagente vivo que más memoria usa, si hay alguno.
    pub top_name: Option<String>,
    pub top_mb: u64,
}

/// Quien mide la memoria por el vigilante.
pub trait MemoryProbe: Send + Sync {
    /// `Some` solo cuando la memoria está por debajo de los umbrales.
    fn low(&self) -> Option<MemoryReading>;
}

impl Watcher {
    /// Avisa de la memoria baja una vez por episodio; no mata nada.
    pub(super) fn check_memory(&mut self, tick: &mut Tick) {
        let Some(probe) = self.memory.clone() else {
            return;
        };
        let Some(reading) = probe.low() else {
            self.memory_low = false;
            return;
        };
        if self.memory_low {
            return;
        }
        if self.send(&memory_text(&reading)) {
            self.memory_low = true;
            tick.memory_warned = true;
        }
    }

    /// El medidor de memoria (lo instala `ez watch`).
    pub fn with_memory_probe(mut self, probe: Arc<dyn MemoryProbe>) -> Self {
        self.memory = Some(probe);
        self
    }
}

/// El texto exacto de §4.
fn memory_text(reading: &MemoryReading) -> String {
    let mut text = format!(
        "[EzAgent · aviso] Memoria baja: RAM libre {} MB, virtual libre {} MB.",
        reading.free_ram_mb, reading.free_commit_mb
    );
    if let Some(name) = reading.top_name.as_deref() {
        text.push_str(&format!(
            " El subagente que más usa es {name} ({} MB).",
            reading.top_mb
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_texto_lleva_las_cifras_y_el_que_mas_usa() {
        let reading = MemoryReading {
            free_ram_mb: 1_200,
            free_commit_mb: 3_000,
            top_name: Some("dev1-opencode".to_string()),
            top_mb: 2_048,
        };
        assert_eq!(
            memory_text(&reading),
            "[EzAgent · aviso] Memoria baja: RAM libre 1200 MB, virtual libre 3000 MB. \
             El subagente que más usa es dev1-opencode (2048 MB)."
        );
    }

    #[test]
    fn sin_subagentes_omite_la_ultima_frase() {
        let reading = MemoryReading {
            free_ram_mb: 900,
            free_commit_mb: 2_000,
            top_name: None,
            top_mb: 0,
        };
        assert_eq!(
            memory_text(&reading),
            "[EzAgent · aviso] Memoria baja: RAM libre 900 MB, virtual libre 2000 MB."
        );
    }
}

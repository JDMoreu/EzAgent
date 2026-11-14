//! Escenario de `fake-llm`: una respuesta guionizada por cada petición.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

fn default_model() -> String {
    "fake-model".to_string()
}

fn default_chunk_chars() -> usize {
    12
}

/// Texto final cuando el guion se agota (nunca bloquea al cliente).
pub const FALLBACK_TEXT: &str = "Fin del guion.";

/// Una llamada a herramienta que el escenario quiere que emita el modelo.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ToolCallSpec {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// JSON de argumentos tal cual (puede venir roto a propósito).
    #[serde(default)]
    pub arguments: String,
}

/// Una respuesta del guion: texto, herramientas, error HTTP o corte.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Turn {
    pub content: String,
    pub tool_calls: Vec<ToolCallSpec>,
    /// `stop`, `length` o `tool_calls`; si falta se deduce.
    pub finish_reason: Option<String>,
    /// 0 equivale a 200.
    pub status: u16,
    pub error: String,
    pub error_type: String,
    pub retry_after: Option<u64>,
    /// Emite solo los primeros N eventos SSE y cierra (corte a mitad).
    pub max_chunks: Option<usize>,
    pub delay_ms: u64,
    /// `usage` exacto que se quiere ver en el último chunk, si hace falta.
    pub usage: Option<Value>,
}

impl Turn {
    pub fn status_code(&self) -> u16 {
        if self.status == 0 {
            200
        } else {
            self.status
        }
    }

    pub fn is_error(&self) -> bool {
        self.status_code() >= 400
    }

    /// `finish_reason` explícito o el que corresponde al contenido.
    pub fn finish(&self) -> String {
        self.finish_reason.clone().unwrap_or_else(|| {
            if self.tool_calls.is_empty() {
                "stop".to_string()
            } else {
                "tool_calls".to_string()
            }
        })
    }

    /// Lo que ocupa la respuesta, para el `usage` inventado.
    pub fn output_tokens(&self) -> u64 {
        let chars = self.content.chars().count()
            + self
                .tool_calls
                .iter()
                .map(|call| call.arguments.chars().count() + call.name.chars().count())
                .sum::<usize>();
        (chars / 4).max(1) as u64
    }
}

/// Guion completo: modelo, tamaño de los trozos y respuestas por turno.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Scenario {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_chunk_chars")]
    pub chunk_chars: usize,
    pub turns: Vec<Turn>,
    /// Respuesta tras agotar `turns`; por defecto, `FALLBACK_TEXT`.
    pub fallback: Option<Turn>,
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            model: default_model(),
            chunk_chars: default_chunk_chars(),
            turns: Vec::new(),
            fallback: None,
        }
    }
}

impl Scenario {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("no pude leer el escenario {}", path.display()))?;
        Self::from_json(&text)
    }

    pub fn from_json(text: &str) -> Result<Self> {
        let mut scenario: Scenario =
            serde_json::from_str(text).context("el escenario de fake-llm no es JSON válido")?;
        if scenario.chunk_chars == 0 {
            scenario.chunk_chars = 1;
        }
        if scenario.model.trim().is_empty() {
            scenario.model = default_model();
        }
        Ok(scenario)
    }

    /// Turno `index` del guion; al agotarse devuelve el fallback.
    pub fn turn(&self, index: usize) -> Turn {
        if let Some(turn) = self.turns.get(index) {
            return turn.clone();
        }
        self.fallback.clone().unwrap_or_else(|| Turn {
            content: FALLBACK_TEXT.to_string(),
            ..Turn::default()
        })
    }

    /// ¿El índice ya cayó fuera del guion?
    pub fn is_fallback(&self, index: usize) -> bool {
        index >= self.turns.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let scenario = Scenario::from_json("{}").expect("escenario");
        assert_eq!(scenario.model, "fake-model");
        assert_eq!(scenario.chunk_chars, 12);
        let turn = scenario.turn(0);
        assert_eq!(turn.content, FALLBACK_TEXT);
        assert!(!turn.is_error());
        assert_eq!(turn.finish(), "stop");
    }

    #[test]
    fn infers_tool_finish_and_status() {
        let scenario = Scenario::from_json(
            r#"{"turns": [
                {"content": "hola", "tool_calls": [{"name": "web", "arguments": "{}"}]},
                {"status": 429, "retry_after": 2, "error": "límite"}
            ]}"#,
        )
        .expect("escenario");
        let tool = scenario.turn(0);
        assert_eq!(tool.finish(), "tool_calls");
        assert_eq!(tool.status_code(), 200);
        let error = scenario.turn(1);
        assert!(error.is_error());
        assert_eq!(error.status_code(), 429);
        assert_eq!(error.retry_after, Some(2));
    }

    #[test]
    fn zero_chunk_chars_becomes_one() {
        let scenario = Scenario::from_json(r#"{"chunk_chars": 0}"#).expect("escenario");
        assert_eq!(scenario.chunk_chars, 1);
    }
}

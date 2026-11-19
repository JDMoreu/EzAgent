//! Error de `ez-term`, con mensajes en español neutro latinoamericano.

use std::fmt;

/// Fallo al hablar con herdr-core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermError {
    /// No existe `hc` en la ruta configurada ni donde se busca.
    NotFound,
    /// No se pudo lanzar `hc` o uno de sus hilos.
    Spawn(String),
    /// La conexión con `hc` terminó (fin de flujo o error de lectura).
    Disconnected,
    /// `hc` no respondió a la petición dentro del tope.
    Timeout {
        /// Método JSON-RPC que quedó sin respuesta.
        method: String,
    },
    /// `hc` respondió con un error JSON-RPC.
    Rpc {
        /// Código JSON-RPC (`-32601`, `-32000`, ...).
        code: i64,
        /// `error.data.kind` cuando `hc` lo trae (`not_found`, `name_taken`, ...).
        kind: Option<String>,
        /// Texto del error tal como lo envió `hc`.
        message: String,
    },
    /// La respuesta no tuvo la forma esperada.
    Decode(String),
}

impl fmt::Display for TermError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => formatter
                .write_str("No se encontró hc: revisa la ruta configurada o instala herdr-core."),
            Self::Spawn(message) => write!(formatter, "No se pudo lanzar hc: {message}"),
            Self::Disconnected => formatter.write_str("La conexión con hc se cerró."),
            Self::Timeout { method } => {
                write!(
                    formatter,
                    "hc no respondió a {method} dentro del tiempo de espera."
                )
            }
            Self::Rpc {
                code,
                kind,
                message,
            } => {
                formatter.write_str("hc rechazó la petición: ")?;
                formatter.write_str(message)?;
                write!(formatter, " (código {code}")?;
                if let Some(kind) = kind {
                    write!(formatter, ", tipo {kind}")?;
                }
                formatter.write_str(")")
            }
            Self::Decode(message) => write!(formatter, "Respuesta de hc inválida: {message}"),
        }
    }
}

impl std::error::Error for TermError {}

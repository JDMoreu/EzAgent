//! Error type of the team crate. Messages are in Spanish because the arbiter and
//! the view show them to the person.

use std::fmt;
use std::path::{Path, PathBuf};

/// Failure while reading configuration, validating a team or rendering the fish texts.
#[derive(Debug)]
pub enum TeamError {
    /// A file could not be read or written.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// A TOML file is not valid or does not have the expected shape.
    Toml {
        path: PathBuf,
        source: toml::de::Error,
    },
    /// The data is well formed but breaks a rule of the design (§6).
    Invalid(String),
}

impl TeamError {
    pub(crate) fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub(crate) fn toml(path: impl AsRef<Path>, source: toml::de::Error) -> Self {
        Self::Toml {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }
}

impl fmt::Display for TeamError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "No pude acceder a «{}»: {source}.",
                    path.display()
                )
            }
            Self::Toml { path, source } => {
                write!(
                    formatter,
                    "«{}» no es TOML válido: {source}.",
                    path.display()
                )
            }
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for TeamError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Toml { source, .. } => Some(source),
            Self::Invalid(_) => None,
        }
    }
}

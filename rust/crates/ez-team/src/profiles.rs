//! Agent profiles (`agentes.toml`, design §6).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::TeamError;

/// Default `agentes.toml`, exactly as written in the design (§6). It is created when
/// the file does not exist; the comments tell the person what each field means.
const DEFAULT_PROFILES_TOML: &str = r#"[[agente]]
id = "opencode"
nombre = "OpenCode"
argv = ["opencode", "--model", "{modelo}"]
modelos_cmd = ["opencode", "models"]      # salida: un modelo por línea

[[agente]]
id = "claude"
nombre = "Claude Code"
argv = ["claude", "--model", "{modelo}"]
modelos = ["opus", "sonnet", "haiku"]
familia = "anthropic"

[[agente]]
id = "codex"
nombre = "Codex"
argv = ["codex", "--model", "{modelo}"]
esfuerzo_args = ["-c", "model_reasoning_effort={esfuerzo}"]
modelos = []                              # la persona los completa
familia = "openai"
"#;

/// One `[[agente]]` of `agentes.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Id used in `equipo.toml` and in the protocol (`"opencode"`).
    pub id: String,
    /// Name shown to the person (`"OpenCode"`).
    #[serde(rename = "nombre")]
    pub name: String,
    /// Command line template; `{modelo}` and `{esfuerzo}` are substituted.
    pub argv: Vec<String>,
    /// Command whose stdout lists the models, one per line.
    #[serde(rename = "modelos_cmd", default)]
    pub models_cmd: Option<Vec<String>>,
    /// Known models, used when there is no `modelos_cmd` or it fails.
    #[serde(rename = "modelos", default)]
    pub models: Vec<String>,
    /// Model family; when absent it is derived from the model name.
    #[serde(rename = "familia", default)]
    pub family: Option<String>,
    /// Arguments added only when the seat has an effort; `{esfuerzo}` is substituted.
    #[serde(rename = "esfuerzo_args", default)]
    pub effort_args: Vec<String>,
}

#[derive(Deserialize)]
struct ProfilesFile {
    #[serde(default, rename = "agente")]
    agents: Vec<AgentProfile>,
}

/// Reads `dir/agentes.toml`. If it does not exist, writes the design defaults first.
pub fn load_or_init(dir: &Path) -> Result<Vec<AgentProfile>, TeamError> {
    let path = dir.join("agentes.toml");
    if !path.exists() {
        fs::create_dir_all(dir).map_err(|error| TeamError::io(dir, error))?;
        fs::write(&path, DEFAULT_PROFILES_TOML).map_err(|error| TeamError::io(&path, error))?;
    }
    load(dir)
}

/// Reads `dir/agentes.toml`; a missing file is an error (use [`load_or_init`]).
pub fn load(dir: &Path) -> Result<Vec<AgentProfile>, TeamError> {
    let path = dir.join("agentes.toml");
    let text = fs::read_to_string(&path).map_err(|error| TeamError::io(&path, error))?;
    let file: ProfilesFile =
        toml::from_str(&text).map_err(|error| TeamError::toml(&path, error))?;
    Ok(file.agents)
}

#[cfg(test)]
mod tests {
    use super::{load_or_init, TeamError};
    use crate::test_support::TempDir;

    #[test]
    fn creates_the_default_file_and_reads_it_back() {
        let dir = TempDir::new("profiles-default");
        let first = load_or_init(dir.path()).expect("defaults");
        assert_eq!(first.len(), 3);
        assert_eq!(first[0].id, "opencode");
        assert_eq!(first[0].name, "OpenCode");
        assert_eq!(first[0].argv, ["opencode", "--model", "{modelo}"]);
        assert_eq!(
            first[0].models_cmd.as_deref(),
            Some(["opencode".to_string(), "models".to_string()].as_slice())
        );
        assert_eq!(first[1].family.as_deref(), Some("anthropic"));
        assert!(first[2].effort_args.contains(&"-c".to_string()));
        assert!(first[2].models.is_empty());

        let written = std::fs::read_to_string(dir.path().join("agentes.toml")).expect("file");
        assert!(written.contains("[[agente]]"));

        let second = load_or_init(dir.path()).expect("second read");
        assert_eq!(first, second);
    }

    #[test]
    fn invalid_toml_is_an_error() {
        let dir = TempDir::new("profiles-invalid");
        std::fs::write(dir.path().join("agentes.toml"), "[[agente]\nid = ").expect("write");
        let error = load_or_init(dir.path()).expect_err("must fail");
        assert!(matches!(error, TeamError::Toml { .. }), "{error}");
        assert!(error.to_string().contains("TOML"));
    }
}

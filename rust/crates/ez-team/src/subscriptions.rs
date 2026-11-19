//! Agent subscriptions (`suscripciones.toml`, design §6).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::TeamError;

/// One `[[suscripcion]]`: the person declares which agents they pay for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    /// Name shown to the person (`"OpenCode Go"`).
    #[serde(rename = "nombre")]
    pub name: String,
    /// Agent ids covered by this subscription.
    #[serde(rename = "agentes", default)]
    pub agents: Vec<String>,
}

#[derive(Deserialize)]
struct SubscriptionsFile {
    #[serde(default, rename = "suscripcion")]
    subscriptions: Vec<Subscription>,
}

/// Reads `dir/suscripciones.toml`; a missing file means no declared subscriptions.
pub fn load(dir: &Path) -> Result<Vec<Subscription>, TeamError> {
    let path = dir.join("suscripciones.toml");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path).map_err(|error| TeamError::io(&path, error))?;
    let file: SubscriptionsFile =
        toml::from_str(&text).map_err(|error| TeamError::toml(&path, error))?;
    Ok(file.subscriptions)
}

#[cfg(test)]
mod tests {
    use super::load;
    use crate::test_support::TempDir;

    #[test]
    fn missing_file_means_no_subscriptions() {
        let dir = TempDir::new("subs-missing");
        assert!(load(dir.path()).expect("empty").is_empty());
    }

    #[test]
    fn reads_name_and_agents() {
        let dir = TempDir::new("subs-read");
        std::fs::write(
            dir.path().join("suscripciones.toml"),
            "[[suscripcion]]\nnombre = \"OpenCode Go\"\nagentes = [\"opencode\"]\n",
        )
        .expect("write");
        let subscriptions = load(dir.path()).expect("read");
        assert_eq!(subscriptions.len(), 1);
        assert_eq!(subscriptions[0].name, "OpenCode Go");
        assert_eq!(subscriptions[0].agents, ["opencode"]);
    }

    #[test]
    fn invalid_file_is_an_error() {
        let dir = TempDir::new("subs-invalid");
        std::fs::write(dir.path().join("suscripciones.toml"), "nombre = [").expect("write");
        assert!(load(dir.path()).is_err());
    }
}

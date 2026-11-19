//! Model families (design §6): the `familia` field wins; otherwise it is derived from
//! the model name.
//!
//! Rule: take the part after the last `/`, up to the first `-` or `.`, and drop trailing
//! digits. A `#variant` suffix is ignored:
//!
//! ```
//! use ez_team::family::family;
//! use ez_team::profiles::AgentProfile;
//!
//! let profile = AgentProfile {
//!     id: "opencode".into(),
//!     name: "OpenCode".into(),
//!     argv: vec!["opencode".into()],
//!     models_cmd: None,
//!     models: Vec::new(),
//!     family: None,
//!     effort_args: Vec::new(),
//! };
//! assert_eq!(family(&profile, "opencode-go/deepseek-v4.1-flash"), "deepseek");
//! assert_eq!(family(&profile, "opencode-go/qwen4-max"), "qwen");
//! assert_eq!(family(&profile, "opencode-go/gpt-6-luna"), "gpt");
//! assert_eq!(family(&profile, "hy4-flash-free"), "hy");
//! ```

use crate::profiles::AgentProfile;

/// Family of `model` for `profile` (the profile's `family` field, or derived).
pub fn family(profile: &AgentProfile, model: &str) -> String {
    if let Some(family) = profile
        .family
        .as_deref()
        .map(str::trim)
        .filter(|family| !family.is_empty())
    {
        return family.to_string();
    }
    derive(model)
}

fn derive(model: &str) -> String {
    let base = model.split('#').next().unwrap_or_default();
    let tail = base.rsplit('/').next().unwrap_or_default();
    let stem: String = tail
        .chars()
        .take_while(|character| *character != '-' && *character != '.')
        .collect();
    stem.trim_end_matches(|character: char| character.is_ascii_digit())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::family;
    use crate::profiles::AgentProfile;

    fn profile(family: Option<&str>) -> AgentProfile {
        AgentProfile {
            id: "opencode".to_string(),
            name: "OpenCode".to_string(),
            argv: vec!["opencode".to_string()],
            models_cmd: None,
            models: Vec::new(),
            family: family.map(str::to_string),
            effort_args: Vec::new(),
        }
    }

    #[test]
    fn design_examples() {
        let opencode = profile(None);
        assert_eq!(
            family(&opencode, "opencode-go/deepseek-v4.1-flash"),
            "deepseek"
        );
        assert_eq!(family(&opencode, "opencode-go/qwen4-max"), "qwen");
        assert_eq!(family(&opencode, "opencode-go/gpt-6-luna"), "gpt");
        assert_eq!(family(&opencode, "hy4-flash-free"), "hy");
    }

    #[test]
    fn variant_suffix_is_ignored() {
        assert_eq!(family(&profile(None), "opencode-go/gpt-6#variant"), "gpt");
    }

    #[test]
    fn declared_family_wins() {
        assert_eq!(family(&profile(Some("anthropic")), "opus"), "anthropic");
        // A blank field is the same as no field.
        assert_eq!(family(&profile(Some("  ")), "opus"), "opus");
    }
}

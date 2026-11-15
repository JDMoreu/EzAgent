//! Reparte cada caso del fixture a su módulo de dominio.

use std::path::Path;

use ez_parity::fixtures::FixtureCase;
use serde_json::Value;

const PRELAUNCH: [&str; 9] = [
    "opencode_state_root",
    "prepare_opencode_state",
    "mcode_config_path",
    "discover_mcode_models",
    "set_mcode_effort",
    "codex_home",
    "discover_codex_models",
    "_strip_jsonc",
    "opencode_default_model",
];

const SESSION: [&str; 4] = ["valid_keys", "fill", "command_line", "_is_shell"];

pub(super) fn run_case(case: &FixtureCase, root: &Path) -> Result<Value, String> {
    let function = case.function.as_str();
    if PRELAUNCH.contains(&function) {
        return super::prelaunch::run(case, root);
    }
    if SESSION.contains(&function) || function.starts_with("AgentSession.") {
        return super::session::run(case);
    }
    if function.starts_with("AgentDriver.") {
        return super::driver::run(case, root);
    }
    if function.starts_with("Watcher.") {
        return super::watcher::run(case, root);
    }
    Err(format!("función sin portar: {function}"))
}

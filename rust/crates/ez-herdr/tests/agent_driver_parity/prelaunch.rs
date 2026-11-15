//! Casos de `agent_prelaunch.py`: rutas, estado de OpenCode, mcode y Codex.

use std::path::Path;

use ez_herdr::prelaunch;
use ez_parity::fixtures::FixtureCase;
use serde_json::{json, Value};

use super::common::{put, snapshot};
use super::fake::text;

fn seed(input: &Value, root: &Path, relative: &str, field: &str) {
    if let Some(content) = input.get(field).filter(|value| !value.is_null()) {
        put(root, relative, &text(Some(content)));
    }
}

pub(super) fn run(case: &FixtureCase, root: &Path) -> Result<Value, String> {
    let input = &case.input;
    let result = match case.function.as_str() {
        "opencode_state_root" => {
            json!({"path": prelaunch::opencode_state_root().display().to_string()})
        }
        "mcode_config_path" => {
            json!({"path": prelaunch::mcode_config_path().display().to_string()})
        }
        "codex_home" => json!({"path": prelaunch::codex_home().display().to_string()}),
        "prepare_opencode_state" => {
            for (relative, content) in input
                .get("files")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                put(root, relative, &text(Some(content)));
            }
            let env = prelaunch::prepare_opencode_state(
                &text(input.get("model_ref")),
                &text(input.get("effort")),
            );
            json!({"env": Value::Object(env), "files": snapshot(root)})
        }
        "discover_mcode_models" => {
            seed(input, root, "minimax/config.yaml", "config");
            json!({"models": prelaunch::discover_mcode_models()})
        }
        "set_mcode_effort" => {
            seed(input, root, "minimax/config.yaml", "config");
            for (relative, content) in input
                .get("files")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                put(root, relative, &text(Some(content)));
            }
            let ok = prelaunch::set_mcode_effort(&text(input.get("effort")));
            json!({"ok": ok, "files": snapshot(root)})
        }
        "discover_codex_models" => {
            seed(input, root, "codex/models_cache.json", "cache");
            json!({"models": Value::Object(prelaunch::discover_codex_models())})
        }
        "_strip_jsonc" => json!({"text": prelaunch::strip_jsonc(&text(input.get("text")))}),
        "opencode_default_model" => {
            for (relative, content) in input
                .get("files")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                put(root, relative, &text(Some(content)));
            }
            json!({"model": prelaunch::opencode_default_model()})
        }
        other => return Err(format!("función sin portar: {other}")),
    };
    Ok(result)
}

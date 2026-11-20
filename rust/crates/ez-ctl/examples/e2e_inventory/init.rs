//! Step 1: `ez init --json`, the atomic `entorno.json` and `planes.toml`.

use std::path::Path;

use serde_json::Value;

use super::checks::Report;
use super::run::{ez_json, flag, number, text};
use super::setup::{self, Sandbox};

/// The agent types the fake catalog declares.
const AGENTS: [&str; 6] = ["claude", "opencode", "mcode", "codex", "pi", "qwen"];

/// Runs the two `ez init` passes.
pub fn run(sandbox: &Sandbox, report: &mut Report) {
    let args = vec!["init".to_string(), "--json".to_string()];
    let first = match ez_json(sandbox, None, &args) {
        Ok(value) => value,
        Err(message) => {
            report.fail("`ez init --json`", message);
            return;
        }
    };
    report.check("`ez init --json` termina bien", flag(&first, "ok"), "ok");
    let path = text(&first, "path");
    let expected = setup::config_folder(sandbox).join("entorno.json");
    report.check(
        "entorno.json queda en %APPDATA%",
        Path::new(&path) == expected,
        format!("{path} (se esperaba {})", expected.display()),
    );
    if let Ok(saved) = std::fs::read_to_string(&expected) {
        match serde_json::from_str::<Value>(&saved) {
            Ok(entorno) => check_entorno(&entorno, report),
            Err(error) => report.fail("entorno.json es JSON", format!("{error}")),
        }
    } else {
        report.fail(
            "entorno.json existe",
            format!("no existe {}", expected.display()),
        );
    }
    check_no_temporary(&expected, report);
    let planes = first.get("planes").cloned().unwrap_or(Value::Null);
    check_planes(&planes, report);
    second_run(sandbox, &args, report);
}

/// The system, resource and agent data of `entorno.json`.
fn check_entorno(entorno: &Value, report: &mut Report) {
    let system = entorno.get("system").cloned().unwrap_or(Value::Null);
    report.check(
        "entorno.json trae el sistema",
        !text(&system, "os").is_empty() && !text(&system, "cpu_model").is_empty(),
        format!("{} · {}", text(&system, "os"), text(&system, "cpu_model")),
    );
    let resources = number(&system, "total_ram_mb") > 0.0
        && number(&system, "logical_cores") > 0.0
        && number(&system, "disk_free_mb") > 0.0;
    report.check(
        "entorno.json trae los recursos",
        resources,
        format!(
            "RAM {} MB · {} núcleos lógicos · disco {} MB",
            number(&system, "total_ram_mb"),
            number(&system, "logical_cores"),
            number(&system, "disk_free_mb")
        ),
    );
    let agents = entorno
        .get("agents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for key in AGENTS {
        let found = agents.iter().find(|agent| text(agent, "key") == key);
        let installed = found.is_some_and(|agent| flag(agent, "installed"));
        let path = found.map(|agent| text(agent, "path")).unwrap_or_default();
        let suffix = format!("{key}.exe");
        report.check(
            &format!("el agente falso {key} está en el inventario"),
            installed && path.to_lowercase().ends_with(&suffix),
            format!("instalado={installed} · {path}"),
        );
    }
    let missing = entorno
        .get("missing")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    report.check(
        "entorno.json dice qué falta declarar",
        missing
            .iter()
            .filter_map(Value::as_str)
            .any(|line| line.contains("planes.toml")),
        format!("{} avisos", missing.len()),
    );
}

/// `planes.toml` exists and was created (not overwritten) on the second pass.
fn check_planes(planes: &Value, report: &mut Report) {
    let path = text(planes, "path");
    let created = flag(planes, "created");
    report.check(
        "`ez init` crea planes.toml",
        created && Path::new(&path).is_file(),
        format!("{path} creado={created}"),
    );
    if let Ok(content) = std::fs::read_to_string(&path) {
        let has_agents = AGENTS
            .iter()
            .all(|key| content.contains(&format!("[{key}]")));
        report.check(
            "planes.toml propone cada agente detectado",
            has_agents,
            format!("{} agentes · {} bytes", AGENTS.len(), content.len()),
        );
    }
}

/// The second pass does not overwrite a `planes.toml` that the person edited.
fn second_run(sandbox: &Sandbox, args: &[String], report: &mut Report) {
    let path = setup::config_folder(sandbox).join("planes.toml");
    let marker = "# marca de la prueba IR-4: no me pises\n";
    let previous = std::fs::read_to_string(&path).unwrap_or_default();
    if let Err(error) = std::fs::write(&path, format!("{previous}{marker}")) {
        report.fail(
            "preparar la segunda corrida de init",
            format!("No pude escribir {}: {error}", path.display()),
        );
        return;
    }
    let second = match ez_json(sandbox, None, args) {
        Ok(value) => value,
        Err(message) => {
            report.fail("la segunda corrida de `ez init`", message);
            return;
        }
    };
    let planes = second.get("planes").cloned().unwrap_or(Value::Null);
    report.check(
        "la segunda corrida no crea planes.toml",
        !flag(&planes, "created"),
        format!("creado={}", flag(&planes, "created")),
    );
    let after = std::fs::read_to_string(&path).unwrap_or_default();
    report.check(
        "la segunda corrida no pisa planes.toml",
        after.contains(marker),
        format!("{} bytes", after.len()),
    );
}

/// The atomic write leaves no temporary file next to `entorno.json`.
fn check_no_temporary(path: &Path, report: &mut Report) {
    let folder = path.parent().unwrap_or(Path::new("."));
    let leftovers: Vec<String> = std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".tmp"))
                .collect()
        })
        .unwrap_or_default();
    report.check(
        "entorno.json se escribe sin dejar temporales",
        leftovers.is_empty(),
        format!("{leftovers:?}"),
    );
}

//! Step 4: `ez delegate developer` without `--agent` chooses with the
//! recommendation, says it and leaves the child under the fake parent.

use std::path::Path;

use serde_json::Value;

use super::checks::Report;
use super::run::{self, ez_json, flag, strings, text};
use super::setup::{self, Sandbox};

/// The child the recommendation must choose.
const EXPECTED_NAME: &str = "dev1-opencode";

/// Runs the delegation and checks the result, the registry and the child.
pub fn run(sandbox: &Sandbox, parent: &str, report: &mut Report) {
    let (brief, mark) = match setup::write_brief(sandbox, 1) {
        Ok(value) => value,
        Err(message) => {
            report.fail("el encargo de la delegación", message);
            return;
        }
    };
    let args = vec![
        "delegate".to_string(),
        "developer".to_string(),
        "--brief".to_string(),
        setup::path_text(&brief),
        "--cwd".to_string(),
        setup::path_text(&sandbox.work),
        "--json".to_string(),
    ];
    let row = match ez_json(sandbox, Some(parent), &args) {
        Ok(value) => value,
        Err(message) => {
            report.fail("`ez delegate developer` sin --agent", message);
            return;
        }
    };
    report.check(
        "`ez delegate developer` sin --agent termina bien",
        flag(&row, "ok"),
        "código 0",
    );
    check_row(&row, parent, report);
    check_registry(sandbox, parent, &row, report);
    check_child(sandbox, parent, &row, &mark, report);
}

/// The product's result: the chosen type, model, effort and placement.
fn check_row(row: &Value, parent: &str, report: &mut Report) {
    let chosen = text(row, "chosen");
    report.check(
        "la salida dice qué eligió y por qué",
        chosen.starts_with("Elegí OpenCode · opencode-go/deepseek-v4.1-flash · high porque"),
        format!("{chosen:.200}"),
    );
    report.check(
        "el hijo elegido es OpenCode · deepseek-v4.1-flash · high",
        text(row, "agent") == "opencode"
            && text(row, "model") == "opencode-go/deepseek-v4.1-flash"
            && text(row, "effort") == "high",
        format!(
            "{} · {} · {}",
            text(row, "agent"),
            text(row, "model"),
            text(row, "effort")
        ),
    );
    report.check(
        "el hijo se llama dev1-opencode y cuelga del padre",
        text(row, "name") == EXPECTED_NAME && text(row, "parent") == parent,
        format!("{} · padre {}", text(row, "name"), text(row, "parent")),
    );
    report.check(
        "el hijo queda en el árbol y el encargo se entrega",
        text(row, "placed") == "tree" && flag(row, "task_sent"),
        format!(
            "placed={} · task_sent={}",
            text(row, "placed"),
            flag(row, "task_sent")
        ),
    );
    report.check(
        "la delegación no deja avisos",
        strings(row, "warnings").is_empty(),
        format!("{:?}", strings(row, "warnings")),
    );
}

/// The record saved by the product under `EZ_SUBAGENTS_DIR`.
fn check_registry(sandbox: &Sandbox, parent: &str, row: &Value, report: &mut Report) {
    let terminal = text(row, "terminal");
    let path = sandbox
        .subagentes
        .join(parent)
        .join(format!("{EXPECTED_NAME}.json"));
    let record = match read_json(&path) {
        Ok(value) => value,
        Err(message) => {
            report.fail("el registro del hijo existe", message);
            return;
        }
    };
    report.check(
        "el registro del hijo está en_proceso bajo el padre",
        text(&record, "state") == "en_proceso"
            && text(&record, "parent") == parent
            && text(&record, "terminal") == terminal
            && text(&record, "role") == "developer",
        format!(
            "{} · {} · {}",
            text(&record, "state"),
            text(&record, "parent"),
            text(&record, "terminal")
        ),
    );
    report.check(
        "el registro del hijo guarda tipo, modelo y esfuerzo",
        text(&record, "agent") == "opencode"
            && text(&record, "model") == "opencode-go/deepseek-v4.1-flash"
            && text(&record, "effort") == "high",
        format!(
            "{} · {} · {}",
            text(&record, "agent"),
            text(&record, "model"),
            text(&record, "effort")
        ),
    );
}

/// `hc` and the fake agent see the same parent, name and message.
fn check_child(sandbox: &Sandbox, parent: &str, row: &Value, mark: &str, report: &mut Report) {
    let terminal = text(row, "terminal");
    match run::terminal(sandbox, &terminal) {
        Ok(data) => {
            let labels = data.get("labels").cloned().unwrap_or(Value::Null);
            report.check(
                "`hc get` confirma el padre y el nombre del hijo",
                text(&data, "parent") == parent && text(&data, "name") == EXPECTED_NAME,
                format!("{} · padre {}", text(&data, "name"), text(&data, "parent")),
            );
            report.check(
                "las etiquetas del hijo lo asocian al padre y al papel",
                text(&labels, "ez.parent") == parent
                    && text(&labels, "ez.role") == "developer"
                    && text(&labels, "ez.model") == "opencode-go/deepseek-v4.1-flash",
                format!("{labels}"),
            );
        }
        Err(message) => report.fail("`hc get` del hijo", format!("{terminal}: {message}")),
    }
    let path = sandbox.registros.join(format!("{terminal}.json"));
    let fake = match read_json(&path) {
        Ok(value) => value,
        Err(message) => {
            report.fail("el registro del agente falso existe", message);
            return;
        }
    };
    let lines = strings(&fake, "lines");
    let message = lines.join("\n");
    let role = "Eres un subagente de EzAgent con el papel «developer»";
    report.check(
        "el primer mensaje del hijo trae el papel",
        message.contains(role),
        format!("{} línea(s) recibidas", lines.len()),
    );
    report.check(
        "el primer mensaje del hijo nombra a su padre y el encargo",
        message.contains(parent) && message.contains(mark),
        format!("padre {parent} · marca {mark}"),
    );
    let argv = strings(&fake, "argv");
    report.check(
        "el hijo de OpenCode arranca con --standalone",
        argv.iter().any(|value| value == "--standalone"),
        format!("{argv:?}"),
    );
    let env = fake.get("env").cloned().unwrap_or(Value::Null);
    let content = text(&env, "OPENCODE_CONFIG_CONTENT");
    report.check(
        "el hijo de OpenCode fija el modelo y el esfuerzo por entorno",
        content.contains("opencode-go/deepseek-v4.1-flash#high"),
        content,
    );
}

/// Reads a JSON file.
fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("no existe {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("{} no es JSON: {error}", path.display()))
}

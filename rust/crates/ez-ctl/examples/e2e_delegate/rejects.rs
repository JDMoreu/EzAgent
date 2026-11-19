//! Los rechazos: lo que `ez delegate` no deja delegar (§5 y §4).

use serde_json::Value;

use super::checks::Report;
use super::children::{ez, hc_json, Parent};
use super::proc;
use super::setup::{Sandbox, CODEX_MISSING, MCODE_MISSING};

/// Un caso de rechazo o de uso incorrecto.
struct Case {
    /// Qué se prueba.
    pub name: &'static str,
    /// El código que se espera.
    pub code: i32,
    /// Los argumentos de `ez delegate` (sin el comando).
    pub args: Vec<String>,
    /// Si el caso necesita que falte `HC_TERMINAL_ID`.
    pub without_terminal: bool,
}

/// Los casos de la prueba.
fn cases() -> Vec<Case> {
    let mut cases = vec![
        rejection(
            "un tipo desconocido",
            vec!["--agent".to_string(), "nada".to_string()],
        ),
        rejection(
            "un modelo fuera del catálogo de Claude Code",
            vec![
                "--agent".to_string(),
                "claude".to_string(),
                "--model".to_string(),
                "gpt-5".to_string(),
            ],
        ),
        rejection(
            "un esfuerzo que Claude Code no tiene",
            vec![
                "--agent".to_string(),
                "claude".to_string(),
                "--effort".to_string(),
                "ultra".to_string(),
            ],
        ),
        rejection(
            "un esfuerzo que el modelo de OpenCode no tiene",
            vec![
                "--agent".to_string(),
                "opencode".to_string(),
                "--model".to_string(),
                "deepseek-v4-pro".to_string(),
                "--effort".to_string(),
                "low".to_string(),
            ],
        ),
        rejection(
            "un modelo que Codex no tiene en su caché",
            vec![
                "--agent".to_string(),
                "codex".to_string(),
                "--model".to_string(),
                CODEX_MISSING.to_string(),
            ],
        ),
        rejection(
            "un modelo que mcode no tiene",
            vec![
                "--agent".to_string(),
                "mcode".to_string(),
                "--model".to_string(),
                MCODE_MISSING.to_string(),
            ],
        ),
        rejection(
            "un esfuerzo en Qwen Code",
            vec![
                "--agent".to_string(),
                "qwen".to_string(),
                "--effort".to_string(),
                "high".to_string(),
            ],
        ),
        usage(
            "una ubicación que no existe",
            vec![
                "--agent".to_string(),
                "claude".to_string(),
                "--show".to_string(),
                "arriba".to_string(),
            ],
        ),
        usage(
            "un nombre con mayúsculas",
            vec![
                "--agent".to_string(),
                "claude".to_string(),
                "--name".to_string(),
                "T9".to_string(),
            ],
        ),
    ];
    // DECISION: el caso sin `HC_TERMINAL_ID` lleva un tipo válido: si no, el
    // comando se rechaza antes por el tipo y no se llega a la comprobación.
    // Su código es el 3 de §4 («sin `hc`/sin `HC_TERMINAL_ID`»), no el 2.
    let mut missing = usage(
        "sin `HC_TERMINAL_ID`",
        vec!["--agent".to_string(), "claude".to_string()],
    );
    missing.without_terminal = true;
    missing.code = 3;
    cases.push(missing);
    cases
}

/// Un rechazo del catálogo (código 1).
fn rejection(name: &'static str, args: Vec<String>) -> Case {
    Case {
        name,
        code: 1,
        args,
        without_terminal: false,
    }
}

/// Un uso incorrecto (código 2).
fn usage(name: &'static str, args: Vec<String>) -> Case {
    Case {
        name,
        code: 2,
        args,
        without_terminal: false,
    }
}

/// Corre los casos de rechazo y comprueba que no crean terminales.
pub fn run(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    let before = terminals(sandbox);
    for case in cases() {
        check_case(sandbox, parent, &case, &before, report);
    }
    let after = terminals(sandbox);
    report.check(
        "no quedan terminales nuevas",
        after.len() <= before.len(),
        format!("{} antes, {} después", before.len(), after.len()),
    );
}

/// Un caso: el código de salida y que no se cree nada.
fn check_case(
    sandbox: &Sandbox,
    parent: &Parent,
    case: &Case,
    before: &[String],
    report: &mut Report,
) {
    let (brief, _) = match super::setup::write_brief(sandbox, 90) {
        Ok(value) => value,
        Err(error) => {
            report.fail(case.name, error.message().to_string());
            return;
        }
    };
    let mut args = vec![
        "delegate".to_string(),
        "developer".to_string(),
        "--brief".to_string(),
        brief.to_string_lossy().into_owned(),
        "--cwd".to_string(),
        sandbox.work.to_string_lossy().into_owned(),
        "--json".to_string(),
    ];
    args.extend(case.args.iter().cloned());
    let output = if case.without_terminal {
        let mut env = sandbox.env.clone();
        env.remove("HC_TERMINAL_ID");
        proc::run(&sandbox.ez, &args, &env, Some(&sandbox.work))
    } else {
        ez(sandbox, parent, &args)
    };
    let ok_code = output.code == case.code;
    report.check(
        case.name,
        ok_code,
        format!(
            "código {} (se esperaba {}): {}",
            output.code,
            case.code,
            output.message()
        ),
    );
    let after = terminals(sandbox);
    report.check(
        &format!("{} no crea terminal", case.name),
        after.len() <= before.len(),
        format!("{} antes, {} después", before.len(), after.len()),
    );
}

/// Las terminales que ve `hc` (vivas o terminadas).
pub fn terminals(sandbox: &Sandbox) -> Vec<String> {
    let args = vec!["ls".to_string(), "--all".to_string(), "--json".to_string()];
    let Ok(data) = hc_json(sandbox, &args) else {
        return Vec::new();
    };
    data.get("terminals")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("address").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Las terminales con una etiqueta.
pub fn terminals_with_label(sandbox: &Sandbox, label: &str) -> Vec<String> {
    let args = vec![
        "ls".to_string(),
        "--all".to_string(),
        "--json".to_string(),
        "--label".to_string(),
        label.to_string(),
    ];
    let Ok(data) = hc_json(sandbox, &args) else {
        return Vec::new();
    };
    data.get("terminals")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("address").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

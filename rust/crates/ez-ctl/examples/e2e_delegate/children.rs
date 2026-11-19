//! El padre falso y las seis delegaciones, con lo que se comprueba de cada hijo.

use serde_json::{json, Map, Value};

use super::checks::Report;
use super::proc::{self, EnvVars};
use super::setup::{self, Sandbox, CODEX_EFFORT, CODEX_MODEL, MCODE_MODEL};

/// El padre falso: su terminal, su espacio y el foco que había antes.
pub struct Parent {
    /// La dirección de la terminal del padre.
    pub address: String,
    /// El identificador del espacio de la prueba.
    pub space_id: String,
    /// El estado de la ventana antes de la prueba (para devolver el foco).
    pub focus: Value,
}

/// Un hijo delegado.
#[derive(Debug, Clone)]
pub struct Child {
    /// El papel pedido.
    pub role: String,
    /// El tipo pedido.
    pub agent: String,
    /// El modelo pedido.
    pub model: Option<String>,
    /// El nivel de esfuerzo pedido.
    pub effort: Option<String>,
    /// Dónde se pidió verlo.
    pub show: String,
    /// El nombre que debería tener (§6).
    pub name: String,
    /// La marca del encargo.
    pub mark: String,
    /// La terminal del padre (el mensaje la nombra).
    pub parent_hint: String,
    /// La terminal donde quedó.
    pub terminal: String,
    /// Dónde quedó de verdad.
    pub placed: String,
    /// Los avisos del lanzamiento.
    pub warnings: Vec<String>,
}

/// Las seis delegaciones de la prueba (§11 del diseño).
pub const CASES: [Case; 6] = [
    Case {
        role: "planner",
        agent: "claude",
        model: Some("sonnet"),
        effort: Some("low"),
        show: "tree",
        name: "plan1-claude",
    },
    Case {
        role: "developer",
        agent: "opencode",
        model: Some("deepseek-v4.1-flash"),
        effort: Some("high"),
        show: "tab",
        name: "dev1-opencode",
    },
    Case {
        role: "developer",
        agent: "qwen",
        model: Some("qwen3-coder-plus"),
        effort: None,
        show: "right",
        name: "dev2-qwen",
    },
    Case {
        role: "qa",
        agent: "codex",
        model: Some(CODEX_MODEL),
        effort: Some(CODEX_EFFORT),
        show: "tree",
        name: "qa1-codex",
    },
    Case {
        role: "qa_validator",
        agent: "mcode",
        model: Some(MCODE_MODEL),
        effort: Some("high"),
        show: "tab",
        name: "valqa1-mcode",
    },
    Case {
        role: "reviewer",
        agent: "pi",
        model: Some("proveedor/libre"),
        effort: Some("medium"),
        show: "tree",
        name: "reviewer1-pi",
    },
];

/// Una delegación de la tabla.
pub struct Case {
    /// El papel.
    pub role: &'static str,
    /// El tipo de agente.
    pub agent: &'static str,
    /// El modelo.
    pub model: Option<&'static str>,
    /// El nivel de esfuerzo.
    pub effort: Option<&'static str>,
    /// Dónde se ve en Herdr.
    pub show: &'static str,
    /// El nombre que debería tener.
    pub name: &'static str,
}

/// Abre el espacio de la prueba y su terminal padre (un agente falso).
pub fn open_parent(sandbox: &Sandbox, report: &mut Report) -> Option<Parent> {
    let focus = match app_json(sandbox, &["status".to_string()]) {
        Ok(value) => value,
        Err(message) => {
            report.fail(
                "la ventana de Herdr responde",
                format!("`herdr-app status` falló: {message}"),
            );
            return None;
        }
    };
    let argv = json!([
        sandbox.fake.to_string_lossy(),
        "--echo",
        sandbox.registros.to_string_lossy(),
        "--as",
        "padre"
    ]);
    let args = vec![
        "do".to_string(),
        "space.new".to_string(),
        "--name".to_string(),
        sandbox.id.clone(),
        "--folder".to_string(),
        sandbox.work.to_string_lossy().into_owned(),
        "--argv".to_string(),
        argv.to_string(),
        "--focus".to_string(),
        "false".to_string(),
    ];
    let data = match app_json(sandbox, &args) {
        Ok(value) => value,
        Err(message) => {
            report.fail(
                "el espacio de la prueba",
                format!("`space.new` falló: {message}"),
            );
            return None;
        }
    };
    let address = text(&data, "terminal");
    let address = if address.is_empty() {
        text(&data, "address")
    } else {
        address
    };
    if address.is_empty() {
        report.fail(
            "el espacio de la prueba",
            format!("`space.new` no devolvió terminal: {data}"),
        );
        return None;
    }
    let space_id = text(&data, "space");
    let space_id = if space_id.is_empty() {
        text(&data, "space_id")
    } else {
        space_id
    };
    report.pass(
        "el espacio de la prueba",
        format!("espacio {} con el padre {}", space_id, address),
    );
    select_space(sandbox, &space_id, report);
    Some(Parent {
        address,
        space_id,
        focus,
    })
}

/// Muestra el espacio de la prueba.
///
/// DECISION: `terminal.keep --place tab` abre la pestaña en el espacio que
/// está a la vista. Como el espacio de la prueba se crea sin robar el foco
/// (`--focus false`), sin esto la primera pestaña del hijo se abriría en el
/// espacio de la persona. El foco se devuelve al final (la limpieza).
fn select_space(sandbox: &Sandbox, space_id: &str, report: &mut Report) {
    if space_id.is_empty() {
        return;
    }
    let args = vec![
        "do".to_string(),
        "space.select".to_string(),
        "--id".to_string(),
        space_id.to_string(),
    ];
    let output = proc::run(&sandbox.app, &args, &EnvVars::default(), None);
    report.check(
        "el espacio de la prueba está a la vista",
        output.ok(),
        format!("código {}: {}", output.code, output.message()),
    );
}

/// Hace las seis delegaciones y devuelve los hijos.
pub fn delegate_all(sandbox: &Sandbox, parent: &Parent, report: &mut Report) -> Vec<Child> {
    let mut children = Vec::new();
    for (index, case) in CASES.iter().enumerate() {
        let child = delegate(sandbox, parent, case, index + 1, report);
        children.push(child);
    }
    children
}

/// Una delegación: lanza el hijo y comprueba lo que devolvió `ez`.
fn delegate(
    sandbox: &Sandbox,
    parent: &Parent,
    case: &Case,
    index: usize,
    report: &mut Report,
) -> Child {
    let (brief, mark) = match setup::write_brief(sandbox, index) {
        Ok(value) => value,
        Err(error) => {
            report.fail(
                &format!("el encargo de {}", case.role),
                error.message().to_string(),
            );
            return empty_child(case, mark_empty(index), &parent.address);
        }
    };
    let mut args = vec![
        "delegate".to_string(),
        case.role.to_string(),
        "--agent".to_string(),
        case.agent.to_string(),
        "--show".to_string(),
        case.show.to_string(),
        "--brief".to_string(),
        brief.to_string_lossy().into_owned(),
        "--cwd".to_string(),
        sandbox.work.to_string_lossy().into_owned(),
        "--json".to_string(),
    ];
    if let Some(model) = case.model {
        args.push("--model".to_string());
        args.push(model.to_string());
    }
    if let Some(effort) = case.effort {
        args.push("--effort".to_string());
        args.push(effort.to_string());
    }
    let output = ez(sandbox, parent, &args);
    let name = format!("{}-{}", case.role, case.agent);
    if !output.ok() {
        report.fail(
            &format!("delegar en {}", name),
            format!("código {}: {}", output.code, output.message()),
        );
        return empty_child(case, mark, &parent.address);
    }
    let data = match proc::json(&output) {
        Ok(value) => value,
        Err(message) => {
            report.fail(&format!("delegar en {name}"), message);
            return empty_child(case, mark, &parent.address);
        }
    };
    let terminal = text(&data, "terminal");
    let placed = text(&data, "placed");
    let real_name = text(&data, "name");
    let warnings = strings(&data, "warnings");
    report.pass(
        &format!("delegar en {name}"),
        format!(
            "{real_name} ({terminal}) en {placed}{}",
            if warnings.is_empty() {
                String::new()
            } else {
                format!("; avisos: {}", warnings.join(" | "))
            }
        ),
    );
    Child {
        role: case.role.to_string(),
        agent: case.agent.to_string(),
        model: case.model.map(str::to_string),
        effort: case.effort.map(str::to_string),
        show: case.show.to_string(),
        name: case.name.to_string(),
        mark,
        parent_hint: parent.address.clone(),
        terminal,
        placed,
        warnings,
    }
}

/// Un hijo vacío (cuando la delegación falló).
fn empty_child(case: &Case, mark: String, parent: &str) -> Child {
    Child {
        role: case.role.to_string(),
        agent: case.agent.to_string(),
        model: case.model.map(str::to_string),
        effort: case.effort.map(str::to_string),
        show: case.show.to_string(),
        name: case.name.to_string(),
        mark,
        parent_hint: parent.to_string(),
        terminal: String::new(),
        placed: String::new(),
        warnings: Vec::new(),
    }
}

/// Una marca vacía.
fn mark_empty(index: usize) -> String {
    format!("E2E-{index}")
}

/// Comprueba cada hijo: su terminal en `hc`, sus etiquetas y su registro.
pub fn check_children(sandbox: &Sandbox, parent: &Parent, children: &[Child], report: &mut Report) {
    for child in children {
        if child.terminal.is_empty() {
            report.fail(
                &format!("el hijo {}", child.name),
                "no se pudo delegar (sin terminal)",
            );
            continue;
        }
        check_labels(sandbox, parent, child, report);
        check_record(sandbox, child, report);
    }
}

/// Las etiquetas y el padre real de la terminal (§3).
fn check_labels(sandbox: &Sandbox, parent: &Parent, child: &Child, report: &mut Report) {
    let data = match hc_json(sandbox, &["get".to_string(), child.terminal.clone()]) {
        Ok(value) => value,
        Err(message) => {
            report.fail(
                &format!("`hc get` de {}", child.name),
                format!("{}: {message}", child.terminal),
            );
            return;
        }
    };
    let real_parent = text(&data, "parent");
    let ok_parent = real_parent == parent.address;
    report.check(
        &format!("el padre de {}", child.name),
        ok_parent,
        format!("{real_parent} (se esperaba {})", parent.address),
    );
    let real_name = text(&data, "name");
    report.check(
        &format!("el nombre de {}", child.name),
        real_name == child.name,
        format!("«{real_name}» (se esperaba «{}»)", child.name),
    );
    let labels = data.get("labels").cloned().unwrap_or(Value::Null);
    let wanted = wanted_labels(child);
    for (key, value) in wanted {
        let real = label(&labels, &key);
        report.check(
            &format!("la etiqueta {key} de {}", child.name),
            real == value,
            format!("«{real}» (se esperaba «{value}»)",),
        );
    }
}

/// Las etiquetas que debería tener el hijo (§3).
fn wanted_labels(child: &Child) -> Vec<(String, String)> {
    let mut wanted = vec![
        ("agent".to_string(), child.agent.clone()),
        ("ez.agent".to_string(), child.agent.clone()),
        ("ez.role".to_string(), child.role.clone()),
    ];
    if let Some(model) = &child.model {
        wanted.push(("ez.model".to_string(), model.clone()));
    }
    if let Some(effort) = &child.effort {
        wanted.push(("ez.effort".to_string(), effort.clone()));
    }
    wanted
}

/// El registro del agente falso: el primer mensaje y el argv de su tipo.
fn check_record(sandbox: &Sandbox, child: &Child, report: &mut Report) {
    let path = sandbox.registros.join(format!("{}.json", child.terminal));
    let Ok(text) = std::fs::read_to_string(&path) else {
        report.fail(
            &format!("el registro de {}", child.name),
            format!("no existe {}", path.display()),
        );
        return;
    };
    let Ok(record) = serde_json::from_str::<Value>(&text) else {
        report.fail(
            &format!("el registro de {}", child.name),
            format!("no es JSON: {}", path.display()),
        );
        return;
    };
    let lines = strings(&record, "lines");
    let message = lines.join("\n");
    let role = format!("Eres un subagente de EzAgent con el papel «{}»", child.role);
    report.check(
        &format!("el primer mensaje de {}", child.name),
        message.contains(&role),
        if message.contains(&role) {
            "el mensaje trae el papel".to_string()
        } else {
            format!("{:?}", first_line(&lines))
        },
    );
    let names_parent = !child.parent_hint.is_empty() && message.contains(&child.parent_hint);
    report.check(
        &format!("la terminal del padre en el mensaje de {}", child.name),
        names_parent,
        format!("el mensaje nombra a {}", child.parent_hint),
    );
    report.check(
        &format!("la marca del encargo en {}", child.name),
        message.contains(&child.mark),
        format!("el mensaje trae {}", child.mark),
    );
    let argv = strings(&record, "argv");
    check_argv(child, &argv, report);
    check_env(child, &record, report);
}

/// El entorno que el producto fija para el tipo del agente.
fn check_env(child: &Child, record: &Value, report: &mut Report) {
    if child.agent != "opencode" {
        return;
    }
    let env = record.get("env").cloned().unwrap_or(Value::Null);
    let content = env
        .get("OPENCODE_CONFIG_CONTENT")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let model = child.model.clone().unwrap_or_default();
    let effort = child.effort.clone().unwrap_or_default();
    let wanted = format!("opencode-go/{model}#{effort}");
    report.check(
        "el OPENCODE_CONFIG_CONTENT de dev1-opencode",
        content.contains(&wanted),
        format!("{content} (se esperaba {wanted})"),
    );
}

/// La primera línea del mensaje.
fn first_line(lines: &[String]) -> String {
    lines.first().cloned().unwrap_or_default()
}

/// El argv y el entorno que corresponden al tipo del agente.
fn check_argv(child: &Child, argv: &[String], report: &mut Report) {
    let model = child.model.clone().unwrap_or_default();
    let effort = child.effort.clone().unwrap_or_default();
    let name = child.name.clone();
    match child.agent.as_str() {
        "claude" => {
            report.check(
                &format!("el argv de {name}"),
                has_flag(argv, "--model", &model) && has_flag(argv, "--effort", &effort),
                format!("{argv:?}"),
            );
        }
        "opencode" => {
            report.check(
                &format!("el argv de {name}"),
                argv.iter().any(|value| value == "--standalone"),
                format!("{argv:?}"),
            );
        }
        "qwen" => {
            report.check(
                &format!("el argv de {name}"),
                has_flag(argv, "--model", &model) && !argv.iter().any(|value| value == "--effort"),
                format!("{argv:?}"),
            );
        }
        "codex" => {
            let effort_arg = format!("model_reasoning_effort={effort}");
            report.check(
                &format!("el argv de {name}"),
                has_flag(argv, "--model", &model) && has_flag(argv, "-c", &effort_arg),
                format!("{argv:?}"),
            );
        }
        "mcode" => {
            report.check(
                &format!("el argv de {name}"),
                has_flag(argv, "--model", &model),
                format!("{argv:?}"),
            );
        }
        "pi" => {
            report.check(
                &format!("el argv de {name}"),
                has_flag(argv, "--model", &model) && has_flag(argv, "--thinking", &effort),
                format!("{argv:?}"),
            );
        }
        other => {
            report.fail(
                &format!("el argv de {name}"),
                format!("tipo desconocido: {other}"),
            );
        }
    }
}

/// ¿El argv tiene esa opción con ese valor?
fn has_flag(argv: &[String], flag: &str, value: &str) -> bool {
    argv.windows(2)
        .any(|pair| pair[0] == flag && pair[1] == value)
}

/// Una etiqueta del `hc get`.
fn label(labels: &Value, key: &str) -> String {
    labels
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Corre `ez` con el entorno de la prueba y el padre como terminal.
pub fn ez(sandbox: &Sandbox, parent: &Parent, args: &[String]) -> proc::Output {
    let mut env = sandbox.env.clone();
    env.set("HC_TERMINAL_ID", parent.address.clone());
    proc::run(&sandbox.ez, args, &env, Some(&sandbox.work))
}

/// Corre `ez` con el entorno de la prueba y otra terminal.
pub fn ez_as(sandbox: &Sandbox, terminal: &str, args: &[String]) -> proc::Output {
    let mut env = sandbox.env.clone();
    env.set("HC_TERMINAL_ID", terminal.to_string());
    proc::run(&sandbox.ez, args, &env, Some(&sandbox.work))
}

/// Corre `hc` y devuelve su JSON.
pub fn hc_json(sandbox: &Sandbox, args: &[String]) -> Result<Value, String> {
    let output = proc::run(&sandbox.hc, args, &EnvVars::default(), None);
    proc::json(&output)
}

/// Corre `herdr-app` y devuelve su JSON.
pub fn app_json(sandbox: &Sandbox, args: &[String]) -> Result<Value, String> {
    let output = proc::run(&sandbox.app, args, &EnvVars::default(), None);
    proc::json(&output)
}

/// Un texto del JSON.
pub fn text(data: &Value, key: &str) -> String {
    data.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Una lista de textos del JSON.
pub fn strings(data: &Value, key: &str) -> Vec<String> {
    match data.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().unwrap_or_default().to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// Los datos de un hijo para el JSON final.
pub fn to_value(child: &Child) -> Value {
    json!({
        "role": child.role,
        "agent": child.agent,
        "model": child.model,
        "effort": child.effort,
        "show": child.show,
        "name": child.name,
        "mark": child.mark,
        "terminal": child.terminal,
        "placed": child.placed,
        "warnings": child.warnings,
    })
}

/// Los datos de un padre para el JSON final.
pub fn parent_value(parent: &Parent) -> Value {
    let mut value = Map::new();
    value.insert("address".to_string(), json!(parent.address));
    value.insert("space".to_string(), json!(parent.space_id));
    Value::Object(value)
}

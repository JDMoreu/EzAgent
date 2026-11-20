//! Prueba de punta a punta de `ez retry` con Herdr real y agentes falsos.
//!
//! Un hijo falso muestra «Free limit reached», el vigilante lo deja
//! `bloqueado`, `ez retry <nombre> --auto` elige otro modelo del mismo tipo y
//! lanza `<nombre>-r1` bajo el mismo padre; el registro viejo queda `fallido`
//! («reemplazado por …») y su terminal se cierra. Sin agentes reales ni cuota.
//!
//! ```text
//! cargo build -p ez-ctl --bin ez -p ez-fakes --bin fake-ws-agent
//! cargo run -p ez-ctl --example e2e_retry
//! ```
//!
//! Sale con 0 si todo pasó, 1 si algo falló y 2 si falta Herdr o un binario.
//! Abre su propio espacio de Herdr y lo cierra al terminar; no toca las
//! terminales de la persona.

#![allow(dead_code)]

#[path = "e2e_delegate/checks.rs"]
mod checks;
#[path = "e2e_delegate/children.rs"]
mod children;
#[path = "e2e_delegate/proc.rs"]
mod proc;
#[path = "e2e_delegate/setup.rs"]
mod setup;

use std::io::Write;
use std::path::Path;
use std::process::{ExitCode, Stdio};
use std::time::{Duration, Instant};

use ez_herdr::subagents::{Change, Record, Registry, TaskState};
use serde_json::{json, Value};

use checks::Report;
use children::Parent;
use proc::EnvVars;
use setup::Sandbox;

/// El nombre del hijo de la prueba.
const CHILD: &str = "retry1-opencode";
/// El modelo gratuito que se agota.
const OLD_MODEL: &str = "glm-5.3";
/// El modelo de plan que debe elegir `--auto`.
const PLAN_MODEL: &str = "deepseek-v4.1-flash";

fn main() -> ExitCode {
    match run() {
        Ok(ok) => ExitCode::from(if ok { 0 } else { 1 }),
        Err(message) => {
            let _ = writeln!(std::io::stderr(), "[EzAgent] {message}");
            ExitCode::from(2)
        }
    }
}

/// La prueba completa.
fn run() -> Result<bool, String> {
    let sandbox = setup::prepare().map_err(|error| error.message().to_string())?;
    let mut report = Report::default();
    if let Err(message) = prepare_sandbox(&sandbox) {
        cleanup(&sandbox, None);
        return Err(message);
    }
    if let Err(message) = children::app_json(&sandbox, &["ls".to_string()]) {
        cleanup(&sandbox, None);
        return Err(format!(
            "la ventana de Herdr no responde (`herdr-app ls`): {message}\n¿Está abierta la ventana de Herdr?"
        ));
    }
    let parent = match children::open_parent(&sandbox, &mut report) {
        Some(parent) => parent,
        None => {
            cleanup(&sandbox, None);
            report_table(&report);
            return Ok(false);
        }
    };
    let result = scenario(&sandbox, &parent, &mut report);
    let leftovers = cleanup(&sandbox, Some(&parent));
    report.check(
        "no quedan terminales de la prueba",
        leftovers.is_empty(),
        leftovers.join(", "),
    );
    report_table(&report);
    result.map(|()| report.ok())
}

/// Deja `voice.json` y `modelos.toml` con dos tipos instalados y predecibles.
fn prepare_sandbox(sandbox: &Sandbox) -> Result<(), String> {
    let bin = sandbox.root.join("bin");
    std::fs::create_dir_all(&bin).map_err(|error| format!("{}: {error}", bin.display()))?;
    let opencode = bin.join("opencode.exe");
    let claude = bin.join("claude.exe");
    for copy in [&opencode, &claude] {
        std::fs::copy(&sandbox.fake, copy)
            .map_err(|error| format!("no pude copiar el agente falso: {error}"))?;
    }
    let appdata = sandbox.root.join("appdata").join("EzAgent");
    let voice = appdata.join("voice.json");
    let mut data: Value = serde_json::from_str(
        &std::fs::read_to_string(&voice)
            .map_err(|error| format!("no pude leer {}: {error}", voice.display()))?,
    )
    .map_err(|error| format!("voice.json no es JSON: {error}"))?;
    let agents = json!({
        "opencode": [
            opencode.to_string_lossy(),
            "--echo",
            sandbox.registros.to_string_lossy(),
            "--as",
            "opencode"
        ],
        "claude": [
            claude.to_string_lossy(),
            "--echo",
            sandbox.registros.to_string_lossy(),
            "--as",
            "claude"
        ]
    });
    data["agents"] = agents;
    std::fs::write(
        &voice,
        serde_json::to_string_pretty(&data).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("{}: {error}", voice.display()))?;
    // En la prueba el modelo «gratuito» es `glm-5.3`; el de plan, deepseek.
    let modelos = format!(
        "[{OLD_MODEL}]\nagente = \"opencode\"\nnivel = \"medio\"\ncosto = \"gratis\"\nfamilia = \"zhipu\"\n\n\
         [{PLAN_MODEL}]\nagente = \"opencode\"\nnivel = \"medio\"\ncosto = \"plan\"\nfamilia = \"deepseek\"\n"
    );
    std::fs::write(appdata.join("modelos.toml"), modelos)
        .map_err(|error| format!("modelos.toml: {error}"))?;
    Ok(())
}

/// El guion: hijo bloqueado por límite, reintento automático y comprobaciones.
fn scenario(sandbox: &Sandbox, parent: &Parent, report: &mut Report) -> Result<(), String> {
    let (brief, mark) = setup::write_brief(sandbox, 1)
        .map_err(|error| format!("no pude escribir el encargo: {}", error.message()))?;
    let terminal = launch_child(sandbox, &parent.address)?;
    report.pass("el hijo falso arranca", format!("{CHILD} en {terminal}"));
    let root = sandbox.root.join("subagentes");
    let registry = Registry::at(&root);
    let record = old_record(sandbox, &parent.address, &terminal, &brief, &mark);
    registry
        .create(&record)
        .map_err(|error| format!("no pude crear el registro viejo: {error}"))?;

    send_limit(sandbox, &terminal)?;
    std::thread::sleep(Duration::from_millis(700));
    let mut watcher = spawn_watcher(sandbox, &parent.address)?;
    let blocked = wait_blocked(&registry, &parent.address, Duration::from_secs(20));
    stop_watcher(&mut watcher);
    report.check(
        "el vigilante deja el hijo bloqueado",
        blocked,
        match blocked {
            true => "estado bloqueado por límite de uso".to_string(),
            false => "no llegó a bloqueado".to_string(),
        },
    );
    if !blocked {
        return Err("el vigilante no marcó bloqueado; no sigo con el reintento".to_string());
    }
    let usage = std::fs::read_to_string(root.join("uso.json")).unwrap_or_default();
    report.check(
        "el agotamiento queda en uso.json",
        usage.contains(OLD_MODEL),
        usage.lines().next().unwrap_or_default().to_string(),
    );

    let data = retry_auto(sandbox, parent, report)?;
    check_result(report, &data);
    check_records(sandbox, &registry, parent, report, &data, &terminal)?;
    Ok(())
}

/// Corre `ez retry <nombre> --auto` y devuelve su JSON.
fn retry_auto(sandbox: &Sandbox, parent: &Parent, report: &mut Report) -> Result<Value, String> {
    let output = children::ez(
        sandbox,
        parent,
        &[
            "retry".to_string(),
            CHILD.to_string(),
            "--auto".to_string(),
            "--json".to_string(),
        ],
    );
    report.check(
        "ez retry --auto termina bien",
        output.ok(),
        format!("código {}: {}", output.code, output.message()),
    );
    if !output.ok() {
        return Err(format!("ez retry falló: {}", output.message()));
    }
    proc::json(&output).map_err(|error| format!("ez retry no devolvió JSON: {error}"))
}

/// Lo que dice el resultado del reintento.
fn check_result(report: &mut Report, data: &Value) {
    let new_name = children::text(data, "name");
    let new_model = children::text(data, "model");
    report.check(
        "el hijo nuevo se llama -r1",
        new_name == format!("{CHILD}-r1"),
        format!("«{new_name}»"),
    );
    report.check(
        "el hijo nuevo usa otro modelo",
        new_model != OLD_MODEL && new_model.contains(PLAN_MODEL),
        format!("«{new_model}»"),
    );
    report.check(
        "el resultado enlaza el registro viejo",
        children::text(data, "retry_of") == CHILD,
        children::text(data, "retry_of"),
    );
    report.check(
        "la terminal vieja se cierra",
        data.get("closed_old").and_then(Value::as_bool) == Some(true),
        format!("{}", data.get("closed_old").unwrap_or(&Value::Null)),
    );
}

/// Los registros enlazados y las terminales.
fn check_records(
    sandbox: &Sandbox,
    registry: &Registry,
    parent: &Parent,
    report: &mut Report,
    data: &Value,
    old_terminal: &str,
) -> Result<(), String> {
    let new_name = children::text(data, "name");
    let new_terminal = children::text(data, "terminal");
    let old = registry
        .get(&parent.address, CHILD)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "desapareció el registro viejo".to_string())?;
    report.check(
        "el registro viejo queda fallido",
        old.state == TaskState::Failed,
        old.state.as_str(),
    );
    let detail = old
        .history
        .last()
        .map(|change| change.detail.clone())
        .unwrap_or_default();
    report.check(
        "el motivo dice reemplazado por",
        detail == format!("reemplazado por {CHILD}-r1"),
        detail,
    );
    let new = registry
        .get(&parent.address, &new_name)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "no hay registro del hijo nuevo".to_string())?;
    report.check(
        "el registro nuevo nace en proceso",
        new.state == TaskState::InProgress,
        new.state.as_str(),
    );
    report.check(
        "el registro nuevo guarda retry_of",
        new.retry_of.as_deref() == Some(CHILD),
        format!("{:?}", new.retry_of),
    );
    report.check(
        "el registro nuevo guarda el encargo",
        new.brief_path
            .as_deref()
            .is_some_and(|path| Path::new(path).is_file()),
        format!("{:?}", new.brief_path),
    );
    report.check(
        "el hijo nuevo está vivo",
        hc_live(sandbox, &new_terminal),
        new_terminal.clone(),
    );
    report.check(
        "la terminal vieja ya no está viva",
        !hc_live(sandbox, old_terminal),
        old_terminal.to_string(),
    );
    Ok(())
}

/// Lanza el hijo falso con padre y etiquetas, como `ez delegate`.
fn launch_child(sandbox: &Sandbox, parent: &str) -> Result<String, String> {
    let args = vec![
        "run".to_string(),
        "--json".to_string(),
        "--bg".to_string(),
        "--parent".to_string(),
        parent.to_string(),
        "--name".to_string(),
        CHILD.to_string(),
        "--cwd".to_string(),
        sandbox.work.to_string_lossy().into_owned(),
        "--no-notify".to_string(),
        "--label".to_string(),
        format!("ez.parent={parent}"),
        "--label".to_string(),
        "ez.role=developer".to_string(),
        "--label".to_string(),
        "ez.agent=opencode".to_string(),
        "--".to_string(),
        sandbox.fake.to_string_lossy().into_owned(),
        "--echo".to_string(),
        sandbox.registros.to_string_lossy().into_owned(),
        "--as".to_string(),
        "opencode".to_string(),
    ];
    let output = proc::run(&sandbox.hc, &args, &EnvVars::default(), None);
    let data = proc::json(&output).map_err(|error| format!("hc run: {error}"))?;
    let terminal = children::text(&data, "terminal");
    let terminal = if terminal.is_empty() {
        children::text(&data, "address")
    } else {
        terminal
    };
    if terminal.is_empty() {
        return Err(format!("hc run no devolvió terminal: {data}"));
    }
    Ok(terminal)
}

/// El registro viejo, como lo dejaría `ez delegate`.
fn old_record(sandbox: &Sandbox, parent: &str, terminal: &str, brief: &Path, mark: &str) -> Record {
    let now = ez_herdr::subagents::epoch_ms();
    Record {
        name: CHILD.to_string(),
        terminal: terminal.to_string(),
        parent: parent.to_string(),
        agent: "opencode".to_string(),
        label: "OpenCode".to_string(),
        model: Some(OLD_MODEL.to_string()),
        effort: Some("low".to_string()),
        role: "developer".to_string(),
        cwd: sandbox.work.to_string_lossy().into_owned(),
        brief_head: format!("Trabaja en {mark}"),
        brief_path: Some(brief.to_string_lossy().into_owned()),
        retry_of: None,
        state: TaskState::InProgress,
        since_ms: now,
        history: vec![Change {
            state: TaskState::InProgress,
            at_ms: now,
            by: "ez delegate".to_string(),
            detail: "encargo entregado".to_string(),
        }],
        summary: None,
        report: None,
        validation: None,
        notified: None,
    }
}

/// Envía al hijo la frase de límite que lee el vigilante.
fn send_limit(sandbox: &Sandbox, terminal: &str) -> Result<(), String> {
    let args = vec![
        "send".to_string(),
        terminal.to_string(),
        "Free limit reached. Resets at 7:00 p.m.".to_string(),
        "--json".to_string(),
    ];
    let output = proc::run(&sandbox.hc, &args, &EnvVars::default(), None);
    if output.ok() {
        Ok(())
    } else {
        Err(format!("hc send: {}", output.message()))
    }
}

/// Espera a que el vigilante deje el registro en `bloqueado`.
fn wait_blocked(registry: &Registry, parent: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(Some(record)) = registry.get(parent, CHILD) {
            if record.state == TaskState::Blocked {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

/// Si la terminal sigue viva, según `hc get`.
fn hc_live(sandbox: &Sandbox, terminal: &str) -> bool {
    if terminal.is_empty() {
        return false;
    }
    let args = vec!["get".to_string(), terminal.to_string()];
    let output = proc::run(&sandbox.hc, &args, &EnvVars::default(), None);
    let Ok(data) = proc::json(&output) else {
        return false;
    };
    children::text(&data, "state") == "live"
        || data
            .get("status")
            .and_then(|status| status.get("state"))
            .and_then(Value::as_str)
            == Some("live")
}

/// Cierra las terminales de la prueba, su espacio y la carpeta temporal.
///
/// Devuelve las terminales de la prueba que quedaron (debería ser vacío).
fn cleanup(sandbox: &Sandbox, parent: Option<&Parent>) -> Vec<String> {
    if let Some(parent) = parent {
        for terminal in test_terminals(sandbox, parent) {
            kill(sandbox, &terminal);
            remove(sandbox, &terminal);
        }
        if !parent.space_id.is_empty() {
            let args = vec![
                "do".to_string(),
                "space.close".to_string(),
                "--id".to_string(),
                parent.space_id.clone(),
                "--confirmed".to_string(),
                "true".to_string(),
            ];
            let _ = proc::run(&sandbox.app, &args, &EnvVars::default(), None);
        }
        restore_focus(sandbox, parent);
        let leftovers = test_terminals(sandbox, parent);
        for terminal in &leftovers {
            kill(sandbox, terminal);
            remove(sandbox, terminal);
        }
        setup::remove_root(sandbox);
        return test_terminals(sandbox, parent);
    }
    setup::remove_root(sandbox);
    Vec::new()
}

/// Las terminales de `hc` que son de esta prueba.
fn test_terminals(sandbox: &Sandbox, parent: &Parent) -> Vec<String> {
    all_terminals(sandbox)
        .into_iter()
        .filter(|item| {
            let text = Value::Object(item.clone()).to_string();
            text.contains(&parent.address) || text.contains(&sandbox.id)
        })
        .filter_map(|item| {
            item.get("address")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}

/// Todas las terminales de `hc`, vivas y terminadas.
fn all_terminals(sandbox: &Sandbox) -> Vec<serde_json::Map<String, Value>> {
    let args = vec!["ls".to_string(), "--all".to_string(), "--json".to_string()];
    let output = proc::run(&sandbox.hc, &args, &EnvVars::default(), None);
    let Ok(data) = proc::json(&output) else {
        return Vec::new();
    };
    data.get("terminals")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_object).cloned().collect())
        .unwrap_or_default()
}

/// Las terminales de `hc` que llevan esa etiqueta.
fn terminals_with_label(sandbox: &Sandbox, label: &str) -> Vec<String> {
    let args = vec![
        "ls".to_string(),
        "--all".to_string(),
        "--json".to_string(),
        "--label".to_string(),
        label.to_string(),
    ];
    let output = proc::run(&sandbox.hc, &args, &EnvVars::default(), None);
    let Ok(data) = proc::json(&output) else {
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

/// `hc kill` de una terminal de la prueba.
fn kill(sandbox: &Sandbox, terminal: &str) {
    let args = vec!["kill".to_string(), terminal.to_string()];
    let _ = proc::run(&sandbox.hc, &args, &EnvVars::default(), None);
}

/// `hc rm` de una terminal terminada de la prueba.
fn remove(sandbox: &Sandbox, terminal: &str) {
    let args = vec!["rm".to_string(), terminal.to_string()];
    let _ = proc::run(&sandbox.hc, &args, &EnvVars::default(), None);
}

/// Devuelve el foco a donde estaba antes de abrir el espacio.
fn restore_focus(sandbox: &Sandbox, parent: &Parent) {
    if let Some(pane) = parent.focus.get("focused_pane").and_then(Value::as_str) {
        if !pane.is_empty() {
            let args = vec![
                "do".to_string(),
                "pane.focus".to_string(),
                "--pane".to_string(),
                pane.to_string(),
            ];
            if proc::run(&sandbox.app, &args, &EnvVars::default(), None).ok() {
                return;
            }
        }
    }
    if let Some(agent) = parent.focus.get("agent").and_then(Value::as_str) {
        if !agent.is_empty() {
            let args = vec![
                "do".to_string(),
                "pane.focus".to_string(),
                "--terminal".to_string(),
                agent.to_string(),
            ];
            let _ = proc::run(&sandbox.app, &args, &EnvVars::default(), None);
        }
    }
}

/// Imprime la tabla de comprobaciones.
fn report_table(report: &Report) {
    let mut out = std::io::stdout();
    for line in report.table().lines() {
        let _ = writeln!(out, "{line}");
    }
}

/// Detiene el vigilante de la prueba.
fn stop_watcher(watcher: &mut std::process::Child) {
    let _ = watcher.kill();
    let _ = watcher.wait();
}

/// Las carpetas que el vigilante usa en la prueba.
fn watch_env(sandbox: &Sandbox) -> EnvVars {
    let mut env = sandbox.env.clone();
    env.set("EZ_WATCH_INTERVAL_MS", "500");
    env.set("EZ_WATCH_GRACE_MS", "100");
    env
}

/// Deja el vigilante de la prueba corriendo en su propio proceso.
fn spawn_watcher(sandbox: &Sandbox, parent: &str) -> Result<std::process::Child, String> {
    let mut command = std::process::Command::new(&sandbox.ez);
    command
        .args([
            "watch".to_string(),
            "--parent".to_string(),
            parent.to_string(),
            "--dir".to_string(),
            sandbox
                .root
                .join("subagentes")
                .to_string_lossy()
                .into_owned(),
            "--hc".to_string(),
            sandbox.hc.to_string_lossy().into_owned(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    watch_env(sandbox).apply(&mut command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
        .spawn()
        .map_err(|error| format!("no pude arrancar el vigilante: {error}"))
}

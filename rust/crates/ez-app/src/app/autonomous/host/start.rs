//! Lanzamiento del ejecutor y de sus correcciones (9.5 y 10.9).

use ez_agent::autonomous::texts;
use ez_agent::tools_herdr::port::HerdrPort;
use ez_herdr::herdr::RunAgent;
use serde_json::Map;

use super::super::{approve, AutonomousLoop, Session};
use super::{address_of, error_of, ok_of};

/// `run_agent` sin cerrojos tomados.
pub(crate) fn run(
    port: &dyn HerdrPort,
    session: &Session,
    agent: &str,
    task: &str,
) -> Result<Map<String, serde_json::Value>, String> {
    let params = RunAgent {
        agent,
        task,
        cwd: Some(&session.state.workdir),
        space: Some(&session.space),
        confirmed: true,
        ..RunAgent::default()
    };
    port.run_agent(params)
        .map_err(|error| texts::launch_failed("el agente", &error.to_string()))
}

/// Lanza el ejecutor con la ruta de `encargo.md` (9.5) y cubre su terminal.
pub fn launch_executor(loop_: &AutonomousLoop, session: &mut Session) -> Result<(), String> {
    let agent = session.executor_agent.clone();
    let task = session.encargo_path().to_string_lossy().to_string();
    let result = run(loop_.port.as_ref(), session, &agent, &task)?;
    let Some(address) = address_of(&result) else {
        return Err(texts::launch_failed(
            "el ejecutor",
            &format!("Herdr no devolvió la terminal: {}", error_of(&result)),
        ));
    };
    session.state.executor_started(&address, &agent);
    if ok_of(&result) {
        session.sent_at = loop_.now();
        return Ok(());
    }
    // 10.9: si falló por el diálogo de confianza, se aprueba y se reenvía.
    if approve::approve_trust(loop_, session, &address)
        && send(loop_.port.as_ref(), &address, &task)?
    {
        session.sent_at = loop_.now();
        return Ok(());
    }
    Err(texts::launch_failed("el ejecutor", &error_of(&result)))
}

/// `send_agent` cuya fila lleva `ok`.
pub(crate) fn send(port: &dyn HerdrPort, address: &str, text: &str) -> Result<bool, String> {
    let result = port
        .send_agent(address, text, true)
        .map_err(|error| error.to_string())?;
    Ok(ok_of(&result))
}

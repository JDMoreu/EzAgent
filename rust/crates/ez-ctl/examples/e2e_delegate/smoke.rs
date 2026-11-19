//! El humo opt-in con los CLI reales (HD-6 §9).
//!
//! Solo corre con `EZ_E2E_REAL=1` y avisa antes: abre la interfaz de cada
//! tipo del catálogo **instalado** con la ruta del producto (`Herdr::run_agent`
//! con el padre falso, `show = tree` y tarea vacía), espera a que el programa
//! arranque, lee la pantalla y lo cierra. Nunca escribe nada en esas
//! interfaces ni les manda mensajes.
//!
//! El `voice.json` real se lee en solo lectura: es el catálogo que hay que
//! probar.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_core::config::VoiceConfig;
use ez_herdr::driver::AgentDriver;
use ez_herdr::herdr::{Herdr, Placement, RunAgent};
use serde_json::Value;

use super::checks::Report;
use super::children::Parent;
use super::proc::{self, EnvVars};
use super::setup::Sandbox;

/// Cuánto se espera a que el programa arranque (segundos).
const START_S: f64 = 30.0;

/// Corre el humo con los CLI reales.
pub fn run(sandbox: &Sandbox, parent: &Parent, report: &mut Report) {
    let Ok(config) = VoiceConfig::load(None) else {
        report.fail("el humo real", "no pude leer el voice.json real");
        return;
    };
    let keys = catalog(&config);
    let herdr = Arc::new(Herdr::new(config.clone()));
    let driver = AgentDriver::new(Arc::clone(&herdr), config.clone());
    for key in keys {
        smoke_one(sandbox, parent, &herdr, &driver, &key, report);
    }
}

/// Los tipos del catálogo, en orden.
fn catalog(config: &VoiceConfig) -> Vec<String> {
    let mut keys: Vec<String> = config
        .get("agents")
        .and_then(Value::as_object)
        .map(|agents| agents.keys().cloned().collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

/// Un tipo del catálogo: se abre, se lee y se cierra.
fn smoke_one(
    sandbox: &Sandbox,
    parent: &Parent,
    herdr: &Herdr,
    driver: &AgentDriver<Arc<Herdr>>,
    key: &str,
    report: &mut Report,
) {
    let program = program_of(herdr, key);
    if program.is_empty() || proc::which(&program).is_none() {
        report.pass(
            &format!("el humo real de {key}"),
            "no está instalado en esta máquina",
        );
        return;
    }
    let params = RunAgent {
        agent: key,
        task: "",
        cwd: Some(&sandbox.work.to_string_lossy()),
        parent: Some(&parent.address),
        show: Placement::Tree,
        confirmed: true,
        ..RunAgent::default()
    };
    let result = match herdr.run_agent(driver.clone(), params) {
        Ok(result) => result,
        Err(error) => {
            report.fail(
                &format!("el humo real de {key}"),
                format!("no se pudo abrir: {error}"),
            );
            return;
        }
    };
    let terminal = result
        .get("terminal")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if terminal.is_empty() {
        report.fail(
            &format!("el humo real de {key}"),
            format!("no se pudo abrir: {result:?}"),
        );
        return;
    }
    let state = wait_started(sandbox, &terminal);
    let screen = proc::run(
        &sandbox.hc,
        &[
            "read".to_string(),
            terminal.clone(),
            "--lines".to_string(),
            "0".to_string(),
        ],
        &EnvVars::default(),
        None,
    );
    let _ = proc::run(
        &sandbox.hc,
        &["kill".to_string(), terminal.clone()],
        &EnvVars::default(),
        None,
    );
    let _ = proc::run(
        &sandbox.hc,
        &["rm".to_string(), terminal.clone()],
        &EnvVars::default(),
        None,
    );
    let detail = format!("{state}; {}", proc::head(screen.stdout.trim(), 300));
    report.pass(&format!("el humo real de {key}"), detail);
}

/// El programa de un tipo del catálogo.
fn program_of(herdr: &Herdr, key: &str) -> String {
    let config = herdr.config();
    config
        .get("agents")
        .and_then(Value::as_object)
        .and_then(|agents| agents.get(key))
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Espera a que el programa arranque (o termine con un código).
fn wait_started(sandbox: &Sandbox, terminal: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs_f64(START_S);
    loop {
        let output = proc::run(
            &sandbox.hc,
            &["get".to_string(), terminal.to_string()],
            &EnvVars::default(),
            None,
        );
        if let Ok(data) = proc::json(&output) {
            let state = data
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if state != "live" {
                let code = data.get("exit_code").and_then(Value::as_i64).unwrap_or(-1);
                return format!("salió con código {code}");
            }
            let status = data
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !status.is_empty() {
                return format!("arrancó ({status})");
            }
        }
        if Instant::now() >= deadline {
            return "no arrancó en 30 s".to_string();
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

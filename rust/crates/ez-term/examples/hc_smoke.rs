//! Humo contra el `hc` real: suscribe, lanza `hostname`, espera su final, lo
//! lee, lo borra y sale con 0 solo si todo funcionó.
//!
//! `cargo run -p ez-term --example hc_smoke`

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use ez_term::{locate_hc, EventStream, HcClient, RunSpec, TermEvent};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    match smoke() {
        Ok(()) => {
            tracing::info!("humo de hc: todo bien");
            ExitCode::SUCCESS
        }
        Err(message) => {
            tracing::error!("humo de hc: {message}");
            ExitCode::FAILURE
        }
    }
}

fn smoke() -> Result<(), String> {
    let hc = locate_hc(None).ok_or_else(|| {
        "no se encontró hc; instala herdr-core o revisa la ruta configurada".to_string()
    })?;
    tracing::info!(hc = %hc.display(), "humo de hc: encontrado");

    let pid = std::process::id();
    let mut labels = BTreeMap::new();
    labels.insert("ez_test".to_string(), pid.to_string());
    let name = format!("ez-smoke-{pid}");

    let stream = EventStream::subscribe(&hc, &labels).map_err(|error| error.to_string())?;
    tracing::info!(foto = stream.snapshot().len(), "suscripción lista");

    let client = HcClient::connect(&hc).map_err(|error| error.to_string())?;
    tracing::info!(version = %client.core_version(), "hello");

    let terminal = client
        .run(&RunSpec {
            argv: vec!["hostname".to_string()],
            name: Some(name.clone()),
            labels: labels.clone(),
            ..RunSpec::default()
        })
        .map_err(|error| error.to_string())?;
    tracing::info!(address = %terminal.address, name = %name, "terminal lanzada");

    let listed = client.list(&labels).map_err(|error| error.to_string())?;
    if !listed
        .iter()
        .any(|item| item.name.as_deref() == Some(name.as_str()))
    {
        return Err(format!("la terminal {name} no aparece en terminal.list"));
    }
    tracing::info!(vivas = listed.len(), "terminal listada");

    if !wait_for_finished(&stream, &terminal.address, &name) {
        return Err("no llegó el evento finished en 15 s".to_string());
    }

    let read = client
        .read(&terminal.address, 50)
        .map_err(|error| error.to_string())?;
    tracing::info!(completo = read.complete, "texto:\n{}", read.text.trim_end());

    client
        .rm(&terminal.address)
        .map_err(|error| error.to_string())?;
    tracing::info!(address = %terminal.address, "terminal borrada");
    Ok(())
}

/// Espera hasta 15 s el `Finished` de esa terminal en la conexión de eventos.
fn wait_for_finished(stream: &EventStream, address: &str, name: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        match stream.events().recv_timeout(left) {
            Ok(TermEvent::Finished {
                terminal,
                exit_code,
                reason,
                ..
            }) if terminal == address || terminal == name => {
                tracing::info!(%terminal, ?exit_code, ?reason, "evento finished");
                return true;
            }
            Ok(TermEvent::Closed) => {
                tracing::warn!("la conexión de eventos se cerró antes del final");
                return false;
            }
            Ok(_) => {}
            Err(_) => return false,
        }
    }
}

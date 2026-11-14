//! Lanzamiento de un miembro en su terminal (hilo `ezagent-swarm-launch`).

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use ez_core::pyvalue::{head_chars, py_strip, python_str, text_or_empty, truthy};
use serde_json::{Map, Value};
use tracing::{info, warn};

use super::{
    Announce, Book, LaunchRequest, Nudge, Signature, Supervisor, SwarmHost, BRIEF_RETRY_SECS,
    LAUNCH_SETTLE_SECS,
};
use crate::db::Clock;
use crate::models::{Member, SwarmInfo};
use crate::service::SwarmService;

/// Todo lo que necesita el hilo de lanzamiento, sin prestar `Supervisor`.
pub(super) struct LaunchContext {
    pub service: Arc<SwarmService>,
    pub host: Arc<dyn SwarmHost>,
    pub seat_dir: PathBuf,
    pub announce: Announce,
    pub command_dir: String,
    pub clock: Clock,
    pub book: Arc<Book>,
}

impl Supervisor {
    /// Abre al miembro en su propio hilo; un fallo se informa y el enjambre sigue.
    pub(super) fn launch(&self, info: &SwarmInfo, member: &Member) {
        let key = (info.id.clone(), member.name.clone());
        {
            let mut state = self.book.lock();
            state.launching.insert(key.clone());
            *state.launches.entry(key.clone()).or_insert(0) += 1;
        }
        let context = Arc::clone(&self.launch);
        let info = info.clone();
        let member = member.clone();
        let spawned = thread::Builder::new()
            .name("ezagent-swarm-launch".to_string())
            .spawn(move || launch_work(&context, &info, &member));
        if let Err(error) = spawned {
            warn!("No pude lanzar el hilo de apertura del enjambre: {error}");
            self.book.lock().launching.remove(&key);
            self.book.wake.set();
        }
    }
}

fn launch_work(context: &LaunchContext, info: &SwarmInfo, member: &Member) {
    let key = (info.id.clone(), member.name.clone());
    if let Err(error) = open_member(context, info, member) {
        warn!(
            "Enjambre {}: no pude abrir a {}: {}",
            info.id, member.name, error
        );
        if let Err(store_error) =
            context
                .service
                .store
                .set_member_state(&info.id, &member.name, "gone")
        {
            warn!(
                "Enjambre {}: no pude marcar a {}: {store_error}",
                info.id, member.name
            );
        }
        (context.announce)(&format!(
            "Enjambre: no pude abrir a {} con {}. {}",
            member.name,
            member.agent,
            head_chars(&error, 160)
        ));
    }
    context.book.lock().launching.remove(&key);
    context.book.wake.set();
}

/// Abre la terminal, guarda su dirección y deja el aviso inicial pendiente.
fn open_member(context: &LaunchContext, info: &SwarmInfo, member: &Member) -> Result<(), String> {
    let key = (info.id.clone(), member.name.clone());
    let seat = write_seat(context, &info.id, &member.name)?;
    let request = launch_request(context, info, member, &seat);
    // Se abre sin tarea: la línea de inicio se escribe después, con el
    // programa ya asentado, y se repite hasta que el agente use `ezs`.
    let result = context.host.run_agent(&member.agent, "", request)?;
    let address = launched_address(&result)?;
    let store = &context.service.store;
    store
        .set_address(&info.id, &member.name, &address)
        .map_err(|error| error.to_string())?;
    context.book.lock().unbriefed.insert(key.clone());
    if let Some(Value::Array(warnings)) = result.get("warnings") {
        for warning in warnings {
            warn!(
                "Enjambre {}: {}: {}",
                info.id,
                member.name,
                python_str(warning)
            );
        }
    }
    let settled = (context.clock)() - BRIEF_RETRY_SECS + LAUNCH_SETTLE_SECS;
    context.book.lock().nudged.insert(
        key,
        Nudge {
            signature: Signature::Brief,
            count: 0,
            at: settled,
        },
    );
    info!(
        "Enjambre {}: {} abierto en {}.",
        info.id, member.name, address
    );
    Ok(())
}

fn launch_request(
    context: &LaunchContext,
    info: &SwarmInfo,
    member: &Member,
    seat: &std::path::Path,
) -> LaunchRequest {
    let extra = context.host.swarm_args(&member.agent);
    let mut env = Map::new();
    env.insert(
        "EZS_SEAT".to_string(),
        Value::String(seat.to_string_lossy().into_owned()),
    );
    LaunchRequest {
        cwd: (!info.cwd.is_empty()).then(|| info.cwd.clone()),
        name: format!("{} · enjambre", member.name),
        env,
        path_prepend: (!context.command_dir.is_empty()).then(|| vec![context.command_dir.clone()]),
        extra_args: (!extra.is_empty()).then_some(extra),
        model: (!member.model.is_empty()).then(|| member.model.clone()),
        effort: (!member.effort.is_empty()).then(|| member.effort.clone()),
    }
}

/// `address` o `terminal` del resultado, o el error que devolvió Herdr.
fn launched_address(result: &Map<String, Value>) -> Result<String, String> {
    let mut address = text_or_empty(result.get("address"));
    if address.is_empty() {
        address = text_or_empty(result.get("terminal"));
    }
    let address = py_strip(&address).to_string();
    if !truthy(result.get("ok").unwrap_or(&Value::Null)) || address.is_empty() {
        let error = text_or_empty(result.get("error"));
        return Err(if error.is_empty() {
            "Herdr no devolvió una terminal.".to_string()
        } else {
            error
        });
    }
    Ok(address)
}

/// Guarda el token del miembro en su archivo; la terminal solo ve la ruta.
fn write_seat(context: &LaunchContext, swarm: &str, member: &str) -> Result<PathBuf, String> {
    let token = context
        .service
        .issue_token(swarm, member)
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&context.seat_dir).map_err(|error| error.to_string())?;
    let seat = context.seat_dir.join(format!("{swarm}-{member}.seat"));
    let name = seat
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let tmp = seat.with_file_name(format!("{name}.tmp"));
    fs::write(&tmp, token).map_err(|error| error.to_string())?;
    fs::rename(&tmp, &seat).map_err(|error| error.to_string())?;
    Ok(seat)
}

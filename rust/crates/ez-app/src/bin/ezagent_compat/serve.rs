//! Servidor del enjambre sobre el tablero real, para el enjambre mixto.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ez_swarm::db::Db;
use ez_swarm::server::SwarmServer;
use ez_swarm::service::{MemberSpec, SwarmService};
use serde_json::{json, Value};

/// Siembra un enjambre, publica `endpoint.json` y atiende hasta que se cierra
/// la entrada estándar. Escribe `{url, swarm, token}` en `out` antes de servir
/// para que el arnés sepa a dónde apuntar.
pub fn run(flags: &BTreeMap<String, String>, out: &Path) -> Result<(), String> {
    let base = flags
        .get("base")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "falta --base".to_string())?;
    let started = start(&base);
    let (result, server) = match started {
        Ok(started) => started,
        Err(error) => {
            let _ = write_json(out, &json!({ "error": error }));
            return Err(error);
        }
    };
    if write_json(out, &result).is_err() {
        server.stop();
        return Err("no pude escribir --out".into());
    }
    let mut sink = String::new();
    let _ = std::io::stdin().read_to_string(&mut sink);
    server.stop();
    Ok(())
}

type Started = (Value, SwarmServer);

fn start(base: &Path) -> Result<Started, String> {
    std::fs::create_dir_all(base).map_err(|error| error.to_string())?;
    let db = Db::open(base.join("board.db")).map_err(|error| error.to_string())?;
    let service = Arc::new(SwarmService::with_defaults(db));
    // DECISION: se siembra con `SwarmService::start`, el mismo alta (miembros,
    // tareas encadenadas y tokens) que usa la app y el servidor de Python.
    let members = [
        MemberSpec::new("ana", "dev", "opencode"),
        MemberSpec::new("bo", "qa", "codex"),
    ];
    let tasks = [
        (
            "Primera tarea de compatibilidad".to_string(),
            "dev".to_string(),
        ),
        (
            "Segunda tarea de compatibilidad".to_string(),
            "dev".to_string(),
        ),
    ];
    let (swarm, seats) = service
        .start("Compatibilidad F7-03", &members, "", Some(&tasks))
        .map_err(|error| error.to_string())?;
    let token = seats
        .iter()
        .find(|seat| seat.name == "ana")
        .map(|seat| seat.token.clone())
        .ok_or_else(|| "el servicio no sentó a ana".to_string())?;
    let server = SwarmServer::new(service, base.join("endpoint.json"));
    let url = server.start().map_err(|error| error.to_string())?;
    let result = json!({ "url": url, "swarm": swarm, "token": token });
    Ok((result, server))
}

fn write_json(path: &Path, value: &Value) -> Result<(), ()> {
    let text = serde_json::to_vec(value).map_err(|_| ())?;
    std::fs::write(path, text).map_err(|_| ())
}

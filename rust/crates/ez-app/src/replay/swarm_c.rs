//! Enjambre del arnés (escenarios 18-20).
//!
//! Porta el bloque `swarm:` de `parity/replay/harness_ext.py`: monta el
//! `SwarmStack` real con un reloj de prueba, avanza el supervisor a mano
//! (`swarm_tick`) y lanza `ezs` con el asiento de cada miembro. El estado
//! compartido es de una sola corrida por proceso, como `ACTIVE` del Python.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pyvalue::{py_strip, python_str};
use ez_swarm::db::Clock;
use ez_swarm::stack::{local_offset_seconds, SwarmStack};
use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::app::core::{SwarmArgs, SwarmFactory};

use super::scenario::scalar_text;

/// Prefijo de los avisos de reanudación que cuenta `resumes_sent`.
const RESUME_PREFIX: &str = "[EzAgent] Ya se restableció tu uso.";

/// Reloj de prueba: el supervisor y el tablero comparten estos segundos.
pub struct TestClock {
    wall: Mutex<f64>,
    mono: Mutex<f64>,
}

impl TestClock {
    /// Hora de pared de 2025-01-01 12:00 UTC y monotónico en 1000.
    ///
    /// El Python fija `TZ=UTC` en el arnés; como `chrono::Local` no lee `TZ`,
    /// el reloj se desplaza por el desfase local para que las horas locales
    /// (avisos y nombres de archivo) coincidan con esa corrida.
    fn new() -> Arc<Self> {
        const WALL_UTC: f64 = 1_735_732_800.0;
        Arc::new(Self {
            wall: Mutex::new(WALL_UTC - local_offset_seconds(WALL_UTC)),
            mono: Mutex::new(1000.0),
        })
    }

    /// Adelanta los dos relojes a la vez.
    pub fn advance(&self, seconds: f64) {
        *self.wall.lock() += seconds;
        *self.mono.lock() += seconds;
    }
}

struct Active {
    clock: Arc<TestClock>,
    stack: Option<Arc<SwarmStack>>,
    swarm_id: String,
    launches: Vec<Vec<String>>,
    root: PathBuf,
}

impl Default for Active {
    fn default() -> Self {
        Self {
            clock: TestClock::new(),
            stack: None,
            swarm_id: String::new(),
            launches: Vec::new(),
            root: PathBuf::new(),
        }
    }
}

/// Estado del enjambre de la corrida (el `ACTIVE` del Python).
static ACTIVE: OnceLock<Mutex<Active>> = OnceLock::new();

fn active() -> &'static Mutex<Active> {
    ACTIVE.get_or_init(|| Mutex::new(Active::default()))
}

/// Prepara el estado de una corrida: raíz del repo y reloj nuevo.
pub fn configure(root: &Path) {
    let mut state = active().lock();
    state.root = root.to_path_buf();
    state.clock = TestClock::new();
    state.stack = None;
    state.swarm_id.clear();
    state.launches.clear();
}

/// Fábrica que la app usa para montar el enjambre con el reloj de prueba.
pub fn factory() -> SwarmFactory {
    Arc::new(build)
}

fn build(args: SwarmArgs) -> Option<Arc<SwarmStack>> {
    let clock = Arc::clone(&active().lock().clock);
    let wall_source = Arc::clone(&clock);
    let wall: Clock = Arc::new(move || *wall_source.wall.lock());
    let mono_source = Arc::clone(&clock);
    let mono: Clock = Arc::new(move || *mono_source.mono.lock());
    match SwarmStack::with_clocks(
        args.base_dir,
        args.host,
        args.announce,
        args.ask,
        &args.command,
        wall,
        mono,
        false,
    ) {
        Ok(stack) => {
            let stack = Arc::new(stack);
            active().lock().stack = Some(Arc::clone(&stack));
            Some(stack)
        }
        Err(error) => {
            tracing::warn!("Modo enjambre no disponible: {error}");
            None
        }
    }
}

/// El stack montado, o el mismo error del Python.
pub fn stack() -> Result<Arc<SwarmStack>, String> {
    active()
        .lock()
        .stack
        .clone()
        .ok_or_else(|| "el escenario no montó el enjambre".to_string())
}

/// Id del enjambre creado por `swarm_start`.
pub fn swarm_id() -> String {
    active().lock().swarm_id.clone()
}

/// Guarda el id del enjambre recién creado.
pub fn set_swarm_id(swarm: String) {
    active().lock().swarm_id = swarm;
}

/// Grupos de miembros lanzados, en orden.
pub fn launches() -> Vec<Vec<String>> {
    active().lock().launches.clone()
}

/// Anota un grupo de lanzamientos.
pub fn push_launch(group: Vec<String>) {
    active().lock().launches.push(group);
}

/// Adelanta el reloj de prueba.
pub fn advance_clock(seconds: f64) {
    active().lock().clock.advance(seconds);
}

/// Espera a que el supervisor termine de abrir a los agentes.
pub fn settle_launches(stack: &SwarmStack, timeout: f64) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs_f64(timeout.max(0.0));
    while Instant::now() < deadline {
        if !stack.supervisor.launching() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err("el supervisor no terminó de abrir a los agentes".to_string())
}

/// Ruta del estado de `fake-hc` (`FAKE_HC_STATE`).
pub fn hc_path() -> Result<PathBuf, String> {
    std::env::var_os("FAKE_HC_STATE")
        .map(PathBuf::from)
        .ok_or_else(|| "falta FAKE_HC_STATE".to_string())
}

/// `_hc_state`: estado actual de `fake-hc`.
pub fn hc_state() -> Result<Value, String> {
    let path = hc_path()?;
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("no pude leer {}: {error}", path.display()))?;
    ez_core::pyloads::loads(&text).map_err(|error| error.to_string())
}

/// `_hc_save`: escribe el estado completo.
pub fn hc_save(data: &Value) -> Result<(), String> {
    let path = hc_path()?;
    std::fs::write(&path, dumps_with(data, &DumpOptions::utf8()))
        .map_err(|error| format!("no pude escribir {}: {error}", path.display()))
}

/// `find_terminal`: primero por `address`, después por `name`.
#[cfg(test)]
pub fn find_terminal<'a>(state: &'a Value, key: &str) -> Option<&'a Value> {
    let terminals = state.get("terminals").and_then(Value::as_array)?;
    terminals
        .iter()
        .find(|terminal| python_str(terminal.get("address").unwrap_or(&Value::Null)) == key)
        .or_else(|| {
            terminals
                .iter()
                .find(|terminal| python_str(terminal.get("name").unwrap_or(&Value::Null)) == key)
        })
}

/// `_address`: terminal del miembro en el enjambre.
pub fn address(stack: &SwarmStack, member: &str) -> Result<String, String> {
    let swarm = swarm_id();
    let found = stack
        .store()
        .members(&swarm)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|item| item.name == member);
    match found {
        Some(item) if !item.address.is_empty() => Ok(item.address),
        _ => Err(format!("{member} no tiene terminal en el enjambre")),
    }
}

/// `hc_lock`: el mismo cerrojo `<estado>.lock` que usa `fake-hc`.
///
/// Sin él, una escritura del arnés puede perderse entre la lectura y la
/// escritura de `fake-hc` (bajo carga, `swarm_lost: shell` desaparecía y el
/// supervisor no veía salir al agente).
pub fn with_hc_lock<T>(run: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let path = hc_path()?;
    let lock = path.with_file_name(format!(
        "{}.lock",
        path.file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    ));
    if let Some(parent) = lock.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("no pude crear {}: {error}", parent.display()))?;
    }
    let mut held = false;
    for _ in 0..400 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)
        {
            Ok(_) => {
                held = true;
                break;
            }
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    if !held {
        return Err(format!("el estado {} sigue bloqueado", path.display()));
    }
    let result = run();
    let _ = std::fs::remove_file(&lock);
    result
}

/// `_terminal(member, **fields)`: edita la terminal del miembro.
pub fn update_terminal(member: &str, fields: &[(&str, Value)]) -> Result<(), String> {
    with_hc_lock(|| {
        let stack = stack()?;
        let address = address(&stack, member)?;
        let mut data = hc_state()?;
        let terminals = data
            .get_mut("terminals")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "fake-hc no tiene terminales".to_string())?;
        let terminal = terminals
            .iter_mut()
            .find(|item| python_str(item.get("address").unwrap_or(&Value::Null)) == address)
            .ok_or_else(|| format!("fake-hc no tiene la terminal {address}"))?;
        for (key, value) in fields {
            terminal[*key] = value.clone();
        }
        hc_save(&data)
    })
}

/// `_swarm_view`: estado del enjambre para las aserciones.
pub fn view() -> Result<Value, String> {
    let stack = stack()?;
    let swarm = swarm_id();
    let info = stack
        .store()
        .get(&swarm)
        .map_err(|error| error.to_string())?;
    let tasks = stack
        .store()
        .tasks(&swarm)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|task| {
            let owner = if task.owner.is_empty() {
                "-"
            } else {
                task.owner.as_str()
            };
            format!("T{} {} {owner}", task.id, task.state)
        })
        .collect::<Vec<_>>();
    let members = stack
        .store()
        .members(&swarm)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|member| format!("{} {}", member.name, member.state))
        .collect::<Vec<_>>();
    let incidents = stack
        .store()
        .incidents(&swarm, false)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|incident| {
            format!(
                "I{} {} {} {} {}",
                incident.id, incident.member, incident.kind, incident.state, incident.action
            )
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "status": info.map_or_else(String::new, |info| info.status),
        "tasks": tasks,
        "members": members,
        "incidents": incidents,
        "launches": launches(),
        "resumes_sent": resumes_sent(),
    }))
}

/// Avisos de reanudación enviados a las terminales de `fake-hc`.
fn resumes_sent() -> i64 {
    hc_state()
        .ok()
        .and_then(|data| data.get("terminals").and_then(Value::as_array).cloned())
        .map_or(0, |terminals| {
            terminals
                .iter()
                .flat_map(|terminal| {
                    terminal
                        .get("sent")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                })
                .filter(|sent| {
                    sent.as_str()
                        .is_some_and(|text| text.starts_with(RESUME_PREFIX))
                })
                .count() as i64
        })
}

/// Ejecutable `ezs` del mismo `cargo build` (o `EZ_REPLAY_EZS`).
pub fn ezs_path() -> PathBuf {
    if let Some(path) = std::env::var_os("EZ_REPLAY_EZS") {
        return PathBuf::from(path);
    }
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            let candidate = parent.join("ezs.exe");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from("ezs")
}

/// Miembro de un paso `swarm_start`.
pub fn member_spec(item: &Value) -> ez_swarm::service::MemberSpec {
    let field = |key: &str| item.get(key).map_or_else(String::new, scalar_text);
    ez_swarm::service::MemberSpec {
        name: field("name"),
        role: field("role"),
        agent: item
            .get("agent")
            .map_or_else(|| "opencode".to_string(), scalar_text),
        model: field("model"),
        effort: field("effort"),
        task: field("task"),
        parallel: item.get("parallel").is_some_and(ez_core::config::truthy),
    }
}

/// `subprocess.run` de `ezs` con el asiento y el endpoint del enjambre.
pub fn run_ezs(
    stack: &SwarmStack,
    member: &str,
    argv: &[String],
) -> Result<(bool, String), String> {
    let seat = stack
        .base_dir
        .join("seats")
        .join(format!("{}-{member}.seat", swarm_id()));
    if !seat.is_file() {
        return Err(format!(
            "{member} no tiene asiento en {}",
            seat.file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
        ));
    }
    let root = active().lock().root.clone();
    let output = std::process::Command::new(ezs_path())
        .args(argv)
        .current_dir(&root)
        .env("EZS_SEAT", seat.to_string_lossy().into_owned())
        .env(
            "EZS_ENDPOINT",
            stack
                .base_dir
                .join("endpoint.json")
                .to_string_lossy()
                .into_owned(),
        )
        .output()
        .map_err(|error| format!("no pude lanzar ezs: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let raw = if stdout.is_empty() { stderr } else { stdout };
    Ok((output.status.success(), py_strip(&raw).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_are_found_by_address_and_name() {
        let state = json!({"terminals": [
            {"name": "dev · enjambre", "address": "t1"},
            {"name": "qa", "address": "t2"},
        ]});
        assert_eq!(
            find_terminal(&state, "t2").and_then(|item| item.get("name")),
            Some(&json!("qa"))
        );
        assert_eq!(
            find_terminal(&state, "dev · enjambre").and_then(|item| item.get("address")),
            Some(&json!("t1"))
        );
        assert!(find_terminal(&state, "otro").is_none());
    }

    #[test]
    fn member_specs_use_the_python_defaults() {
        let spec = member_spec(&json!({"name": "plan", "role": "plan"}));
        assert_eq!(spec.name, "plan");
        assert_eq!(spec.role, "plan");
        assert_eq!(spec.agent, "opencode");
        assert!(!spec.parallel);
        let parallel = member_spec(&json!({"name": "qa", "parallel": true}));
        assert!(parallel.parallel);
    }

    #[test]
    fn the_test_clock_moves_both_hands() {
        let clock = TestClock::new();
        let before = *clock.wall.lock();
        clock.advance(25860.0);
        assert_eq!(*clock.wall.lock(), before + 25860.0);
        assert_eq!(*clock.mono.lock(), 1000.0 + 25860.0);
    }
}

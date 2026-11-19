//! Motor del espacio de trabajo: árbitro, servidor `ez`, lanzador y eventos de hc.
//!
//! Diseño: `docs/designs/espacio-elefante.md` §7, §9, §14 y §15. No se crea en
//! modo demostración (ECO-6a).

pub mod events;
pub mod handler;
pub mod launcher;
pub mod spaces;
mod tree;
mod types;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{unbounded, Receiver, Sender};
use ez_arbiter::{Arbiter, Card, ElephantKey, EventKind, MainAgent, Space, SystemClock};
use ez_ctl::{new_token, Caller, CtlServer, Tokens};
use ez_term::{locate_hc, HcClient};
use parking_lot::{Mutex, RwLock};
use serde_json::{json, Value};

use self::handler::ArbiterHandler;
use self::launcher::{HcTerminals, RuntimeLauncher, TeamInstructionWriter};
use self::spaces::{ElephantTokens, MainChoice, Principales, SpaceConfig};

pub use self::types::{FishInfo, MainAgentInfo, RuntimeStatus, SpaceTree, TreeNode};

/// Opciones de arranque del motor (ECO-8). Permite a un ejemplo decir dónde
/// están `ez.exe` y los binarios de los agentes cuando no arranca desde la
/// carpeta del ejecutable de la ventana.
#[derive(Debug, Clone, Default)]
pub struct RuntimeOptions {
    /// Carpeta que se antepone al `PATH` de los agentes; por omisión, la del
    /// ejecutable actual.
    pub ez_dir: Option<PathBuf>,
}

const HC_MISSING: &str = "No encontré herdr-core (hc). Instálalo desde https://releases.herdr.dev y vuelve a abrir EzAgent.";
const EZ_PORT: u16 = 47830;
const EVENT_LOG_LIMIT: usize = 500;

/// Motor ya arrancado. Los campos opcionales están vacíos si el motor está
/// degradado (sin herdr-core o sin `ez.exe`).
pub struct HubRuntime {
    status: RuntimeStatus,
    spaces: Vec<SpaceConfig>,
    tokens: Arc<Tokens>,
    launcher: Option<Arc<RuntimeLauncher>>,
    arbiter: Option<Arc<Arbiter>>,
    handler: Option<Arc<ArbiterHandler>>,
    elephant_tokens: ElephantTokens,
    principales: Principales,
    need_relaunch: Arc<AtomicBool>,
    relaunched: Mutex<BTreeSet<String>>,
    changed: (Sender<()>, Receiver<()>),
    event_log: Arc<Mutex<Vec<ez_arbiter::ArbiterEvent>>>,
}

mod engine;

fn main_labels(space: &str, elephant: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("ez".to_string(), "1".to_string()),
        ("ez_space".to_string(), space.to_string()),
        ("ez_elephant".to_string(), elephant.to_string()),
        ("ez_role".to_string(), "main".to_string()),
    ])
}

fn find_address(nodes: &[TreeNode], id: &str) -> Option<String> {
    for node in nodes {
        if node.id == id {
            return node.address.clone();
        }
        if let Some(address) = find_address(&node.children, id) {
            return Some(address);
        }
    }
    None
}

fn cards_from(events: &[ez_arbiter::ArbiterEvent]) -> Vec<Card> {
    let mut cards: BTreeMap<String, Card> = BTreeMap::new();
    for event in events {
        match &event.kind {
            EventKind::CardOpened(card) => {
                cards.insert(card.id.clone(), card.clone());
            }
            EventKind::CardClosed { id } => {
                cards.remove(id);
            }
            _ => {}
        }
    }
    cards.into_values().collect()
}

fn load_spaces(path: &Path) -> Result<Vec<SpaceConfig>, String> {
    spaces::load_spaces(path)
}

fn label(labels: &BTreeMap<String, String>, name: &str) -> String {
    labels.get(name).cloned().unwrap_or_default()
}

fn build_engine(
    data_dir: &Path,
    spaces: &[SpaceConfig],
    profiles: Vec<ez_team::AgentProfile>,
    client: HcClient,
    tokens: &Arc<Tokens>,
    url: &Arc<RwLock<Option<String>>>,
    exe_dir: PathBuf,
) -> Result<(Arc<RuntimeLauncher>, Arc<Arbiter>), String> {
    let launcher = Arc::new(RuntimeLauncher {
        terminals: Arc::new(HcTerminals::new(client)),
        tokens: Arc::clone(tokens),
        profiles,
        url: Arc::clone(url),
        exe_dir,
    });
    let arbiter = Arc::new(Arbiter::new(
        data_dir.to_path_buf(),
        Arc::clone(&launcher) as Arc<dyn ez_arbiter::FishLauncher>,
        Arc::new(TeamInstructionWriter),
        Arc::new(SystemClock),
    )?);
    for space in spaces {
        arbiter.register_space(Space {
            id: space.id.clone(),
            name: space.name.clone(),
            folder: space.folder.clone(),
        });
    }
    Ok((launcher, arbiter))
}

fn register_elephant_tokens(
    tokens_file: &ElephantTokens,
    tokens: &Tokens,
    arbiter: &Arbiter,
    spaces: &[SpaceConfig],
) {
    let mut owners: HashMap<String, String> = HashMap::new();
    for space in spaces {
        for agent in arbiter.main_agents(&space.id) {
            owners.insert(agent.key.elephant, space.id.clone());
        }
    }
    for (elephant, token) in tokens_file.all() {
        if let Some(space) = owners.get(&elephant) {
            tokens.register(
                &token,
                Caller::Elephant {
                    space: space.clone(),
                    elephant: elephant.clone(),
                },
            );
        }
    }
}

/// Guarda la traza del árbitro en memoria para `hub_state`; el hilo no retiene
/// el candado mientras espera eventos (§4).
fn collect_arbiter_events(
    arbiter: &Arbiter,
    log: Arc<Mutex<Vec<ez_arbiter::ArbiterEvent>>>,
) -> Result<(), String> {
    let receiver = arbiter.subscribe();
    thread::Builder::new()
        .name("ez-hub-arbiter-events".to_string())
        .spawn(move || {
            for event in receiver {
                let mut events = log.lock();
                // DECISION: se conservan los últimos 500 eventos para que la
                // traza de la vista crezca sin límite de memoria.
                if events.len() >= EVENT_LOG_LIMIT {
                    events.remove(0);
                }
                events.push(event);
            }
        })
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn exe_directory() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "No pude localizar la carpeta del ejecutable.".to_string())
}

fn exe_name() -> &'static str {
    // DECISION: en Windows el ejecutable lleva extensión; en el resto, no.
    if cfg!(windows) {
        "ez.exe"
    } else {
        "ez"
    }
}

fn start_server(
    handler: Arc<ArbiterHandler>,
    tokens: &Arc<Tokens>,
    need_relaunch: &Arc<AtomicBool>,
) -> Result<String, String> {
    let server = match CtlServer::start_on(EZ_PORT, handler.clone(), Arc::clone(tokens)) {
        Ok(server) => server,
        Err(error) => {
            tracing::warn!(%error, "el puerto 47830 está ocupado; uso uno libre");
            need_relaunch.store(true, Ordering::SeqCst);
            CtlServer::start(handler, Arc::clone(tokens)).map_err(|error| error.to_string())?
        }
    };
    // DECISION: el servidor se suelta a propósito: sus hilos viven mientras la
    // ventana esté abierta y el proceso los termina al salir.
    Ok(server.url())
}

fn connect_hc() -> Result<Option<(HcClient, PathBuf)>, String> {
    if let Some(pair) = try_connect() {
        return Ok(Some(pair));
    }
    start_hc_server()?;
    Ok(try_connect())
}

fn try_connect() -> Option<(HcClient, PathBuf)> {
    let hc = locate_hc(None)?;
    HcClient::connect(&hc).ok().map(|client| (client, hc))
}

fn start_hc_server() -> Result<(), String> {
    let Some(hc) = locate_hc(None) else {
        return Ok(());
    };
    let mut command = std::process::Command::new(hc);
    command.args(["server", "start"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    match command.spawn() {
        Ok(_) => Ok(()),
        Err(error) => Err(format!("No pude arrancar herdr-core: {error}")),
    }
}

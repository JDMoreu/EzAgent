//! Motor del espacio de trabajo: árbitro, servidor `ez`, lanzador y eventos de hc.
//!
//! Diseño: `docs/designs/espacio-elefante.md` §7, §8, §9 y §14. No se crea en
//! modo demostración (ECO-6a).

pub mod events;
pub mod handler;
pub mod launcher;
pub mod spaces;
mod types;

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};
use ez_arbiter::{Arbiter, Space, SystemClock};
use ez_ctl::{new_token, Caller, CtlServer, Tokens};
use ez_term::{locate_hc, HcClient};
use parking_lot::{Mutex, RwLock};

use self::handler::ArbiterHandler;
use self::launcher::{HcTerminals, RuntimeLauncher, TeamInstructionWriter};
use self::spaces::{load_spaces, ElephantTokens, SpaceConfig};

pub use self::types::{FishInfo, RuntimeStatus};

const HC_MISSING: &str = "No encontré herdr-core (hc). Instálalo desde https://releases.herdr.dev y vuelve a abrir EzAgent.";
const EZ_PORT: u16 = 47830;

/// Motor ya arrancado. Los campos opcionales están vacíos si el motor está
/// degradado (sin herdr-core o sin `ez.exe`).
pub struct HubRuntime {
    status: RuntimeStatus,
    spaces: Vec<SpaceConfig>,
    tokens: Arc<Tokens>,
    launcher: Option<Arc<RuntimeLauncher>>,
    elephant_tokens: ElephantTokens,
    need_relaunch: Arc<AtomicBool>,
    relaunched: Mutex<BTreeSet<String>>,
    changed: (Sender<()>, Receiver<()>),
}

impl HubRuntime {
    /// Arranca el motor en modo normal. Nunca se llama en `--demo`.
    pub fn start(data_dir: &Path) -> Result<HubRuntime, String> {
        let spaces = load_spaces(&data_dir.join("espacios.json"))?;
        let profiles =
            ez_team::profiles::load_or_init(data_dir).map_err(|error| error.to_string())?;
        let subscriptions =
            ez_team::subscriptions::load(data_dir).map_err(|error| error.to_string())?;
        let exe_dir = exe_directory()?;
        let Some((client, hc_path)) = connect_hc()? else {
            return Ok(Self::degraded(spaces, data_dir, HC_MISSING));
        };
        let ez_ok = exe_dir.join(exe_name()).is_file();
        let tokens = Arc::new(Tokens::new());
        let url = Arc::new(RwLock::new(None::<String>));
        let (launcher, arbiter) =
            build_engine(data_dir, &spaces, profiles, client, &tokens, &url, exe_dir)?;
        let elephant_tokens = ElephantTokens::load(data_dir.join("elefantes.json"));
        register_elephant_tokens(&elephant_tokens, &tokens);
        let handler = Arc::new(ArbiterHandler::new(
            Arc::clone(&arbiter) as Arc<dyn handler::ArbiterApi>,
            launcher.profiles.clone(),
            subscriptions,
            spaces.clone(),
        ));
        let need_relaunch = Arc::new(AtomicBool::new(false));
        let service_url = start_server(handler, &tokens, &need_relaunch)?;
        *url.write() = Some(service_url.clone());
        let changed = unbounded();
        events::spawn(&hc_path, arbiter, Arc::clone(&launcher), changed.0.clone())?;
        let status = if ez_ok {
            RuntimeStatus::normal(service_url)
        } else {
            RuntimeStatus::degraded(
                "No encontré ez.exe junto a la ventana; el comando `ez` no funcionará hasta reinstalar EzAgent.",
            )
        };
        Ok(HubRuntime {
            status,
            spaces,
            tokens,
            launcher: Some(launcher),
            elephant_tokens,
            need_relaunch,
            relaunched: Mutex::new(BTreeSet::new()),
            changed,
        })
    }

    /// Motor caído antes de conectarse, con un motivo para la vista.
    pub fn failed(data_dir: &Path, reason: &str) -> HubRuntime {
        Self::degraded(Vec::new(), data_dir, reason)
    }

    fn degraded(spaces: Vec<SpaceConfig>, data_dir: &Path, reason: &str) -> HubRuntime {
        HubRuntime {
            status: RuntimeStatus::degraded(reason),
            spaces,
            tokens: Arc::new(Tokens::new()),
            launcher: None,
            elephant_tokens: ElephantTokens::load(data_dir.join("elefantes.json")),
            need_relaunch: Arc::new(AtomicBool::new(false)),
            relaunched: Mutex::new(BTreeSet::new()),
            changed: unbounded(),
        }
    }

    /// Estado actual del motor para la vista.
    pub fn status(&self) -> RuntimeStatus {
        self.status.clone()
    }

    /// Canal de avisos para la vista; el hilo de Tauri emite `hub://changed`.
    pub fn subscribe_changed(&self) -> Receiver<()> {
        self.changed.1.clone()
    }

    /// Dirección hc del elefante de `space`; lo lanza si no está vivo.
    pub fn ensure_elephant(&self, space: &str) -> Result<String, String> {
        let launcher = self
            .launcher
            .as_ref()
            .ok_or_else(|| self.status.reason.clone())?;
        let config = self
            .spaces
            .iter()
            .find(|item| item.id == space)
            .ok_or_else(|| format!("No conozco el espacio «{space}»."))?;
        let labels = BTreeMap::from([
            ("ez".to_string(), "1".to_string()),
            ("ez_space".to_string(), space.to_string()),
            ("ez_role".to_string(), "elephant".to_string()),
        ]);
        let alive: Vec<_> = launcher
            .list(&labels)?
            .into_iter()
            .filter(|terminal| terminal.state.eq_ignore_ascii_case("live"))
            .collect();
        if let Some(terminal) = alive.first() {
            if self.must_relaunch(space) {
                launcher.kill(&terminal.address)?;
            } else {
                return Ok(terminal.address.clone());
            }
        }
        let token = self
            .elephant_tokens
            .get(space)
            .filter(|token| !token.is_empty())
            .unwrap_or_else(new_token);
        self.tokens.register(
            &token,
            Caller::Elephant {
                space: space.to_string(),
            },
        );
        match launcher.launch_elephant(config, &token) {
            Ok(address) => {
                self.elephant_tokens.set(space, &token)?;
                self.relaunched.lock().insert(space.to_string());
                Ok(address)
            }
            Err(error) => {
                self.tokens.revoke(&token);
                Err(error)
            }
        }
    }

    fn must_relaunch(&self, space: &str) -> bool {
        self.need_relaunch.load(Ordering::SeqCst) && !self.relaunched.lock().contains(space)
    }

    /// Peces vivos de `space`, con dirección, papel, agente, modelo y estado.
    pub fn space_fish(&self, space: &str) -> Result<Vec<FishInfo>, String> {
        let launcher = self
            .launcher
            .as_ref()
            .ok_or_else(|| self.status.reason.clone())?;
        let labels = BTreeMap::from([("ez".to_string(), "1".to_string())]);
        let terminals = launcher.list(&labels)?;
        Ok(terminals
            .into_iter()
            .filter(|terminal| {
                terminal.labels.get("ez_space").map(String::as_str) == Some(space)
                    && terminal.labels.get("ez_role").map(String::as_str) == Some("fish")
            })
            .map(|terminal| FishInfo {
                status: terminal
                    .status
                    .as_ref()
                    .map(|status| status.state.clone())
                    .unwrap_or_else(|| terminal.state.clone()),
                address: terminal.address,
                fish: label(&terminal.labels, "ez_fish"),
                role: label(&terminal.labels, "ez_role_fish"),
                agent: label(&terminal.labels, "ez_agent"),
                model: label(&terminal.labels, "ez_model"),
            })
            .collect())
    }

    /// Envía texto crudo a la terminal del elefante.
    pub fn elephant_send(&self, space: &str, text: &str) -> Result<(), String> {
        let address = self.ensure_elephant(space)?;
        let launcher = self
            .launcher
            .as_ref()
            .ok_or_else(|| self.status.reason.clone())?;
        launcher.send_text(&address, text)
    }

    #[cfg(test)]
    pub(crate) fn with_parts(
        spaces: Vec<SpaceConfig>,
        launcher: Arc<RuntimeLauncher>,
        tokens: Arc<Tokens>,
        elephant_tokens: ElephantTokens,
        need_relaunch: bool,
    ) -> HubRuntime {
        HubRuntime {
            status: RuntimeStatus::normal("http://127.0.0.1:1"),
            spaces,
            tokens,
            launcher: Some(launcher),
            elephant_tokens,
            need_relaunch: Arc::new(AtomicBool::new(need_relaunch)),
            relaunched: Mutex::new(BTreeSet::new()),
            changed: unbounded(),
        }
    }
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

fn register_elephant_tokens(tokens_file: &ElephantTokens, tokens: &Tokens) {
    for (space, token) in tokens_file.all() {
        tokens.register(&token, Caller::Elephant { space });
    }
}

fn label(labels: &std::collections::BTreeMap<String, String>, name: &str) -> String {
    labels.get(name).cloned().unwrap_or_default()
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

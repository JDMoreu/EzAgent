//! Port of `src/ezagent/swarm/stack.py` (F4-13a).
//!
//! Todo el enjambre en un objeto: tablero, servicio, servidor local y
//! supervisor. La app de voz solo crea un [`SwarmStack`], lo arranca y lo
//! detiene. Mientras no haya enjambres activos no hace nada: el supervisor
//! duerme y el servidor solo ocupa un puerto local.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ez_core::metrics::monotonic;
use ez_core::pytime::timestamp;
use ez_core::pyvalue::{collapse_spaces, head_chars};

use crate::db::{Clock, Db, DbResult};
use crate::recovery::Recovery;
use crate::server::SwarmServer;
use crate::service::SwarmService;
use crate::supervisor::{Announce, Supervisor, SupervisorConfig, SwarmHost};

/// Carpeta del ejecutable `ezs` (junto al programa o en el PATH), o «».
///
/// DECISION: Python mira `sys.executable`; en Rust, `current_exe`.
pub fn find_command_dir(command: &str) -> String {
    if let Ok(current) = std::env::current_exe() {
        if let Some(beside) = current.parent() {
            for name in [format!("{command}.exe"), command.to_string()] {
                if beside.join(name).is_file() {
                    return beside.to_string_lossy().into_owned();
                }
            }
        }
    }
    which(command).map_or_else(String::new, |found| {
        found
            .parent()
            .map_or_else(String::new, |parent| parent.to_string_lossy().into_owned())
    })
}

/// `shutil.which` para el comando y, en Windows, sus extensiones del `PATH`.
fn which(command: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: Vec<String> = if Path::new(command).extension().is_some() || !cfg!(windows) {
        vec![command.to_string()]
    } else {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(|extension| format!("{command}{extension}"))
            .collect()
    };
    for folder in std::env::split_paths(&path) {
        for name in &names {
            let candidate = folder.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Punto único de montaje del modo enjambre.
pub struct SwarmStack {
    /// Carpeta con el tablero, los asientos y el endpoint.
    pub base_dir: PathBuf,
    /// Adaptador de Herdr (o un doble en las pruebas).
    pub host: Arc<dyn SwarmHost>,
    /// Servicio del tablero.
    pub service: Arc<SwarmService>,
    /// Servidor local que atiende a `ezs`.
    pub server: SwarmServer,
    /// Carpeta del ejecutable `ezs` que se antepone al `PATH`.
    pub command_dir: String,
    /// Supervisor en su hilo.
    pub supervisor: Arc<Supervisor>,
    /// `false` deja el supervisor sin hilo: el arnés lo avanza a mano (F7-01c).
    pub supervisor_thread: bool,
}

impl SwarmStack {
    /// Monta el tablero, el servicio, el servidor y el supervisor.
    pub fn new(
        base_dir: impl Into<PathBuf>,
        host: Arc<dyn SwarmHost>,
        announce: Announce,
        ask: Option<Announce>,
        command: &str,
    ) -> DbResult<Self> {
        Self::with_clocks(
            base_dir,
            host,
            announce,
            ask,
            command,
            Arc::new(timestamp),
            Arc::new(monotonic),
            true,
        )
    }

    /// Igual que [`Self::new`] con relojes explícitos y control del hilo.
    #[allow(clippy::too_many_arguments)]
    pub fn with_clocks(
        base_dir: impl Into<PathBuf>,
        host: Arc<dyn SwarmHost>,
        announce: Announce,
        ask: Option<Announce>,
        command: &str,
        wall: Clock,
        clock: Clock,
        supervisor_thread: bool,
    ) -> DbResult<Self> {
        let base_dir = base_dir.into();
        let store = Db::new(base_dir.join("board.db"), Arc::clone(&wall))?;
        let service = Arc::new(SwarmService::new(store, command, Arc::clone(&wall)));
        let server = SwarmServer::new(Arc::clone(&service), base_dir.join("endpoint.json"));
        let command_dir = find_command_dir(command);
        let mut config = SupervisorConfig::new(base_dir.join("seats"), Arc::clone(&announce));
        config.ask = ask;
        config.command_dir.clone_from(&command_dir);
        config.clock = clock;
        config.wall = wall;
        let supervisor = Arc::new(Supervisor::new(
            Arc::clone(&service),
            Arc::clone(&host),
            config,
        ));
        Ok(Self {
            base_dir,
            host,
            service,
            server,
            command_dir,
            supervisor,
            supervisor_thread,
        })
    }

    /// Tablero del enjambre.
    pub fn store(&self) -> &Db {
        &self.service.store
    }

    /// Adaptador de Herdr del stack.
    pub fn host(&self) -> &Arc<dyn SwarmHost> {
        &self.host
    }

    /// Arranca el servidor local y, si procede, el supervisor.
    pub fn start(&self) -> std::io::Result<()> {
        self.server.start()?;
        if self.supervisor_thread {
            self.supervisor.start();
        }
        Ok(())
    }

    /// Detiene el supervisor y el servidor.
    pub fn stop(&self) {
        self.supervisor.stop();
        self.server.stop();
    }

    /// Incidentes y decisiones del supervisor.
    pub fn recovery(&self) -> &Recovery {
        &self.supervisor.recovery
    }

    /// Hay algún enjambre en marcha o en pausa.
    pub fn active(&self) -> DbResult<bool> {
        Ok(!self.store().live()?.is_empty())
    }

    /// Estado de los enjambres activos para el prompt (dato, no instrucción).
    pub fn prompt_block(&self) -> DbResult<String> {
        let live = self.store().live()?;
        if live.is_empty() {
            return Ok(String::new());
        }
        let mut lines = vec!["<swarms>".to_string()];
        let start = live.len().saturating_sub(3);
        for info in &live[start..] {
            let goal = escape(head_chars(&collapse_spaces(&info.goal), 100));
            let spoken = escape(&self.service.status(&info.id)?.spoken());
            lines.push(format!("- enjambre {} · «{goal}» · {spoken}", info.id));
            for incident in self.store().incidents(&info.id, true)? {
                let state = if incident.state == "waiting" {
                    "esperando"
                } else {
                    "pide decisión"
                };
                lines.push(format!(
                    "  incidente I{} · {} · {} · {state} · \
                     opciones: wait, replace, reopen, release, pause",
                    incident.id, incident.member, incident.kind
                ));
            }
        }
        lines.push("</swarms>".to_string());
        Ok(lines.join("\n"))
    }
}

/// Offset of the local time zone at `epoch`, in seconds.
///
/// The Python harness fixes `TZ=UTC`; this lets the Rust replay use the same
/// wall clock without changing what production shows in local time.
pub fn local_offset_seconds(epoch: f64) -> f64 {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(epoch as i64, 0)
        .earliest()
        .map_or(0.0, |moment| f64::from(moment.offset().local_minus_utc()))
}

/// `html.escape(text, quote=False)`.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::service::MemberSpec;
    use crate::supervisor::test_support::FakeHost;

    /// Stack sobre un tablero temporal; el llamador limpia `base_dir`.
    fn stack() -> (SwarmStack, PathBuf, Arc<Mutex<Vec<String>>>) {
        let base =
            std::env::temp_dir().join(format!("ez-swarm-stack-{}", uuid::Uuid::new_v4().simple()));
        let said = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&said);
        let announce: Announce = Arc::new(move |text| {
            sink.lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(text.to_string());
        });
        let host: Arc<dyn SwarmHost> = Arc::new(FakeHost::new());
        let stack = SwarmStack::new(&base, host, announce, None, "ezs")
            .unwrap_or_else(|error| panic!("{error}"));
        (stack, base, said)
    }

    fn clean(base: &Path) {
        for name in ["board.db", "board.db-wal", "board.db-shm", "endpoint.json"] {
            let _ = std::fs::remove_file(base.join(name));
        }
    }

    #[test]
    fn missing_commands_have_no_folder() {
        assert_eq!(
            find_command_dir("definitely-not-an-ezs-command-386"),
            String::new()
        );
    }

    #[test]
    fn prompt_block_is_empty_without_swarms() {
        let (stack, base, _) = stack();
        assert_eq!(
            stack
                .prompt_block()
                .unwrap_or_else(|error| panic!("{error}")),
            ""
        );
        assert!(!stack.active().unwrap_or_else(|error| panic!("{error}")));
        clean(&base);
    }

    #[test]
    fn prompt_block_lists_goal_and_incidents() {
        let (stack, base, _) = stack();
        let members = [MemberSpec::new("ana", "dev", "claude")];
        let (swarm, _) = stack
            .service
            .start("<Migrar & probar>", &members, "", None)
            .unwrap_or_else(|error| panic!("{error}"));
        stack
            .store()
            .add_task(&swarm, "Portar", "dev", "", &[], 0)
            .unwrap_or_else(|error| panic!("{error}"));
        stack
            .store()
            .open_incident(&swarm, "ana", "quota", "sin saldo", 0.0)
            .unwrap_or_else(|error| panic!("{error}"));
        let block = stack
            .prompt_block()
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(block.starts_with("<swarms>\n"));
        assert!(block.ends_with("</swarms>"));
        assert!(block.contains(&format!(
            "- enjambre {swarm} · «&lt;Migrar &amp; probar&gt;» · "
        )));
        assert!(block.contains(
            "  incidente I1 · ana · quota · pide decisión · opciones: wait, replace, reopen, release, pause"
        ));
        assert!(stack.active().unwrap_or_else(|error| panic!("{error}")));
        clean(&base);
    }

    #[test]
    fn starting_and_stopping_the_stack_is_idempotent() {
        let (stack, base, _) = stack();
        stack.start().unwrap_or_else(|error| panic!("{error}"));
        assert!(!stack.server.url().is_empty());
        stack.start().unwrap_or_else(|error| panic!("{error}"));
        stack.stop();
        assert_eq!(stack.server.url(), "");
        clean(&base);
    }

    #[test]
    fn the_prompt_only_shows_the_last_three_swarms() {
        let (stack, base, _) = stack();
        let members = [MemberSpec::new("ana", "dev", "claude")];
        for index in 0..4 {
            stack
                .service
                .start(&format!("meta {index}"), &members, "", None)
                .unwrap_or_else(|error| panic!("{error}"));
        }
        let block = stack
            .prompt_block()
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(!block.contains("«meta 0»"));
        assert_eq!(block.matches("- enjambre ").count(), 3);
        assert!(block.contains("«meta 3»"));
        clean(&base);
    }
}

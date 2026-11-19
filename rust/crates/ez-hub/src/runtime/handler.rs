//! Adaptador del protocolo `ez` al árbitro (diseño §7 y §8).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_arbiter::{
    Caller as ArbiterCaller, Command, ElephantEvent, ElephantKey, FishKey, Outcome, Rejection,
    Verdict,
};
use ez_ctl::{Caller, Handler, Reply, Request};
use ez_team::{
    inventory::Inventory, library_dir, load_roles, profiles::AgentProfile, roles_text,
    subscriptions::Subscription, RealSystem, RoleId,
};
use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::runtime::spaces::SpaceConfig;

const CACHE_SECONDS: u64 = 60;
const DEFAULT_WAIT_SECONDS: u64 = 170;

/// Operaciones del árbitro que usa el adaptador. `Arbiter` las cumple; las
/// pruebas usan un doble que registra cada comando recibido.
pub trait ArbiterApi: Send + Sync {
    /// Ejecuta un comando en nombre de `caller`.
    fn execute(&self, caller: &ArbiterCaller, command: Command) -> Result<Outcome, Rejection>;
    /// Espera una novedad del agente principal o del pez que llama (§15.1).
    fn wait(&self, caller: &ArbiterCaller, timeout: Duration) -> Option<ElephantEvent>;
    /// Estado serializado de la tarea activa de `space`.
    fn snapshot(&self, space: &str) -> Option<Value>;
    /// Worktree del agente principal, si está registrado en el árbitro.
    fn worktree(&self, space: &str, elephant: &str) -> Option<PathBuf>;
    /// Ejecuta una acción de tarjeta (aprobación, pausa o integración).
    fn card_action(
        &self,
        card: &str,
        action: &str,
        comment: Option<&str>,
    ) -> Result<Outcome, Rejection>;
}

impl ArbiterApi for ez_arbiter::Arbiter {
    fn execute(&self, caller: &ArbiterCaller, command: Command) -> Result<Outcome, Rejection> {
        ez_arbiter::Arbiter::execute(self, caller, command)
    }

    fn wait(&self, caller: &ArbiterCaller, timeout: Duration) -> Option<ElephantEvent> {
        match caller {
            ArbiterCaller::Elephant(key) => self.wait_for_main(key, timeout),
            ArbiterCaller::Fish(key) => self.wait_for_fish(key, timeout),
        }
    }

    fn snapshot(&self, space: &str) -> Option<Value> {
        self.snapshot(space)
    }

    fn worktree(&self, space: &str, elephant: &str) -> Option<PathBuf> {
        self.main_agents(space)
            .into_iter()
            .find(|agent| agent.key.elephant == elephant)
            .map(|agent| agent.worktree)
    }

    fn card_action(
        &self,
        card: &str,
        action: &str,
        comment: Option<&str>,
    ) -> Result<Outcome, Rejection> {
        ez_arbiter::Arbiter::card_action(self, card, action, comment)
    }
}

/// Atiende los comandos del protocolo usando el árbitro real.
pub struct ArbiterHandler {
    arbiter: Arc<dyn ArbiterApi>,
    profiles: Vec<AgentProfile>,
    subscriptions: Vec<Subscription>,
    spaces: Vec<SpaceConfig>,
    inventory: Mutex<Option<(Instant, Inventory)>>,
    /// Descubre los modelos de Codex; las pruebas lo sustituyen por un doble.
    codex_models: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
}

impl ArbiterHandler {
    /// Crea el adaptador con el inventario vacío en caché.
    pub fn new(
        arbiter: Arc<dyn ArbiterApi>,
        profiles: Vec<AgentProfile>,
        subscriptions: Vec<Subscription>,
        spaces: Vec<SpaceConfig>,
    ) -> Self {
        Self {
            arbiter,
            profiles,
            subscriptions,
            spaces,
            inventory: Mutex::new(None),
            codex_models: Arc::new(default_codex_models),
        }
    }

    /// Sustituye el descubrimiento de modelos de Codex (para pruebas).
    pub fn with_codex_models(
        mut self,
        discover: impl Fn() -> Vec<String> + Send + Sync + 'static,
    ) -> Self {
        self.codex_models = Arc::new(discover);
        self
    }

    fn inventory(&self) -> Inventory {
        let cached = { self.inventory.lock().clone() };
        if let Some((when, found)) = cached {
            if when.elapsed() < Duration::from_secs(CACHE_SECONDS) {
                return found;
            }
        }
        // DECISION: se calcula fuera del candado; dos peticiones simultáneas
        // pueden repetir el sondeo, pero ninguna retiene un lock durante E/S.
        let profiles = self.profiles_with_codex_models();
        let found = ez_team::inventory::detect(&profiles, &self.subscriptions, &RealSystem);
        *self.inventory.lock() = Some((Instant::now(), found.clone()));
        found
    }

    fn profiles_with_codex_models(&self) -> Vec<AgentProfile> {
        let discovered = (self.codex_models)();
        self.profiles
            .iter()
            .cloned()
            .map(|mut profile| {
                if profile.id == "codex" && profile.models.is_empty() {
                    profile.models = discovered.clone();
                }
                profile
            })
            .collect()
    }

    fn execute(&self, caller: &ArbiterCaller, command: Command) -> Reply {
        match self.arbiter.execute(caller, command) {
            Ok(outcome) => Reply::ok(outcome.message, outcome.data),
            Err(rejection) => Reply::failure(rejection.message),
        }
    }

    fn role_defs(&self, space: &str, elephant: &str) -> Result<Vec<ez_team::RoleDef>, String> {
        let folder = self.folder(space, elephant);
        let drafts = folder.join(".ez").join("borrador").join("papeles");
        load_roles(Some(&drafts), &library_dir()).map_err(|error| error.to_string())
    }

    /// Texto de `ez roles` para el agente principal dado (lo usa también la vista).
    pub fn roles_message(&self, space: &str, elephant: &str) -> Result<String, String> {
        Ok(roles_text(&self.role_defs(space, elephant)?))
    }

    /// Datos de `ez inventory` para la vista.
    pub fn inventory_reply(&self) -> Reply {
        let found = self.inventory();
        Reply::ok(found.text(), found.json())
    }

    fn team_propose(&self, space: &str, elephant: &str, args: &Value) -> Result<Command, String> {
        let file = PathBuf::from(require_text(args, "file")?);
        let folder = self.folder(space, elephant);
        let file = inside(&folder, &file)?;
        let text = std::fs::read_to_string(&file)
            .map_err(|error| format!("No pude leer {}: {error}.", file.display()))?;
        let roles = self.role_defs(space, elephant)?;
        let seats = ez_team::team::parse(&text, &roles).map_err(|error| error.to_string())?;
        let found = self.inventory();
        let check = ez_team::team::validate(&seats, &found, &roles);
        if !check.errors.is_empty() {
            return Err(check.errors.join("\n"));
        }
        let title = self
            .arbiter
            .snapshot(space)
            .and_then(|snapshot| {
                snapshot
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "Tarea".to_string());
        let card = ez_team::team::card(&title, &seats, &check, &found, &roles);
        Ok(Command::TeamPropose { seats, roles, card })
    }

    /// Carpeta de trabajo del agente principal: su worktree si existe, si no la
    /// carpeta del espacio. Las reglas y los archivos de la tarea viven ahí (§15.1).
    fn folder(&self, space: &str, elephant: &str) -> PathBuf {
        self.arbiter
            .worktree(space, elephant)
            .or_else(|| {
                self.spaces
                    .iter()
                    .find(|item| item.id == space)
                    .map(|item| item.folder.clone())
            })
            .unwrap_or_default()
    }

    fn wait(&self, caller: &ArbiterCaller, args: &Value) -> Reply {
        let seconds = args
            .get("timeout")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_WAIT_SECONDS);
        match self.arbiter.wait(caller, Duration::from_secs(seconds)) {
            Some(event) => {
                let message = event.message.clone();
                Reply::ok(message, json!({ "event": event }))
            }
            None => Reply::ok(
                "Sin novedades; vuelve a ejecutar `ez wait`.",
                json!({ "event": null }),
            ),
        }
    }
}

impl Handler for ArbiterHandler {
    fn handle(&self, caller: &Caller, request: &Request) -> Reply {
        let result = self.dispatch(caller, request);
        match result {
            Ok(reply) => reply,
            Err(message) => Reply::failure(message),
        }
    }
}

impl ArbiterHandler {
    fn dispatch(&self, caller: &Caller, request: &Request) -> Result<Reply, String> {
        let space = space_of(caller);
        let elephant = elephant_of(caller);
        let arbiter_caller = to_arbiter_caller(caller);
        let args = &request.args;
        match request.command.as_str() {
            "status" => Ok(self.execute(&arbiter_caller, Command::Status)),
            "inventory" => {
                let found = self.inventory();
                Ok(Reply::ok(found.text(), found.json()))
            }
            "roles" => Ok(Reply::ok(self.roles_message(&space, &elephant)?, json!({}))),
            "wait" => Ok(self.wait(&arbiter_caller, args)),
            "task.new" => Ok(self.execute(
                &arbiter_caller,
                Command::TaskNew {
                    title: require_text(args, "title")?,
                    brief: PathBuf::from(require_text(args, "brief")?),
                    criteria: PathBuf::from(require_text(args, "criteria")?),
                },
            )),
            "team.propose" => {
                Ok(self.execute(&arbiter_caller, self.team_propose(&space, &elephant, args)?))
            }
            "round.start" => Ok(self.execute(&arbiter_caller, Command::RoundStart)),
            "agent.delegate" => {
                // El elefante lanza un pez; un subagente delega en uno de sus hijos.
                // El árbitro comprueba el papel y los límites en cada caso (§15.5).
                let role = RoleId::parse(&require_text(args, "role")?)?;
                let brief = PathBuf::from(require_text(args, "brief")?);
                let command = match caller {
                    Caller::Fish { .. } => Command::AgentDelegate { role, brief },
                    Caller::Elephant { .. } => Command::FishNew { role, brief },
                };
                Ok(self.execute(&arbiter_caller, command))
            }
            "fish.send" => Ok(self.execute(
                &arbiter_caller,
                Command::FishSend {
                    fish: require_text(args, "fish")?,
                    file: PathBuf::from(require_text(args, "file")?),
                },
            )),
            "verify" => Ok(self.execute(&arbiter_caller, Command::Verify)),
            "round.close" => Ok(self.execute(&arbiter_caller, Command::RoundClose)),
            "task.close" => Ok(self.execute(&arbiter_caller, Command::TaskClose)),
            "pause" => Ok(self.execute(
                &arbiter_caller,
                Command::Pause {
                    reason: require_text(args, "reason")?,
                },
            )),
            "report" => Ok(self.execute(&arbiter_caller, self.report(args)?)),
            other => Err(format!("Comando desconocido: {other}.")),
        }
    }

    fn report(&self, args: &Value) -> Result<Command, String> {
        let verdict = match require_text(args, "verdict")?.as_str() {
            "pass" => Verdict::Pass,
            "fail" => Verdict::Fail,
            _ => return Err("--verdict debe ser pass o fail.".to_string()),
        };
        Ok(Command::Report {
            verdict,
            file: PathBuf::from(require_text(args, "file")?),
            blocking: require_number(args, "blocking")?,
            major: require_number(args, "major")?,
            minor: require_number(args, "minor")?,
            mutations_tried: optional_number(args, "mutations_tried"),
            mutations_caught: optional_number(args, "mutations_caught"),
        })
    }
}

fn to_arbiter_caller(caller: &Caller) -> ArbiterCaller {
    match caller {
        Caller::Elephant { space, elephant } => ArbiterCaller::Elephant(ElephantKey {
            space: space.clone(),
            elephant: elephant.clone(),
        }),
        Caller::Fish {
            space,
            task,
            elephant,
            fish,
            attempt,
            parent,
        } => ArbiterCaller::Fish(FishKey {
            space: space.clone(),
            task: task.clone(),
            elephant: elephant.clone(),
            fish: fish.clone(),
            attempt: *attempt,
            parent: parent.clone(),
        }),
    }
}

fn space_of(caller: &Caller) -> String {
    match caller {
        Caller::Elephant { space, .. } => space.clone(),
        Caller::Fish { space, .. } => space.clone(),
    }
}

fn elephant_of(caller: &Caller) -> String {
    match caller {
        Caller::Elephant { elephant, .. } => elephant.clone(),
        Caller::Fish { elephant, .. } => elephant.clone(),
    }
}

fn default_codex_models() -> Vec<String> {
    ez_herdr::prelaunch::discover_codex_models()
        .keys()
        .cloned()
        .collect()
}

fn require_text(args: &Value, name: &str) -> Result<String, String> {
    args.get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("Falta el argumento «{name}»."))
}

fn require_number(args: &Value, name: &str) -> Result<u32, String> {
    args.get(name)
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| format!("«{name}» debe ser un número entero."))
}

fn optional_number(args: &Value, name: &str) -> Option<u32> {
    args.get(name)
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
}

/// Comprueba que `file` esté dentro de `root` por componentes (§14.7).
fn inside(root: &Path, file: &Path) -> Result<PathBuf, String> {
    let root = std::fs::canonicalize(root).map_err(|error| error.to_string())?;
    let target = if file.exists() {
        std::fs::canonicalize(file)
    } else {
        let parent = file.parent().unwrap_or(file);
        std::fs::canonicalize(parent).map(|path| path.join(file.file_name().unwrap_or_default()))
    }
    .map_err(|error| error.to_string())?;
    if target.starts_with(&root) {
        Ok(target)
    } else {
        Err("El archivo está fuera de la carpeta del espacio.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_both_callers() {
        assert_eq!(
            to_arbiter_caller(&Caller::Elephant {
                space: "s".to_string(),
                elephant: "e12345678".to_string(),
            }),
            ArbiterCaller::Elephant(ElephantKey {
                space: "s".to_string(),
                elephant: "e12345678".to_string(),
            })
        );
        assert_eq!(
            to_arbiter_caller(&Caller::Fish {
                space: "s".to_string(),
                task: "t".to_string(),
                elephant: "e12345678".to_string(),
                fish: "dev".to_string(),
                attempt: 2,
                parent: Some("plan-r1".to_string()),
            }),
            ArbiterCaller::Fish(FishKey {
                space: "s".to_string(),
                task: "t".to_string(),
                elephant: "e12345678".to_string(),
                fish: "dev".to_string(),
                attempt: 2,
                parent: Some("plan-r1".to_string()),
            })
        );
    }

    #[test]
    fn parses_report_numbers_and_verdict() {
        let args = json!({
            "verdict": "pass", "file": "r.md", "blocking": 0, "major": 1, "minor": 2,
            "mutations_tried": 3
        });
        match require_number(&args, "major") {
            Ok(1) => {}
            _ => panic!("número mal leído"),
        }
        assert_eq!(optional_number(&args, "mutations_tried"), Some(3));
        assert_eq!(optional_number(&args, "mutations_caught"), None);
        assert!(require_text(&args, "missing").is_err());
    }

    #[test]
    fn rejects_paths_outside_the_space_folder() {
        let dir = crate::testutil::TempDir::new("runtime-handler-inside");
        let inside_file = dir.path().join("equipo.toml");
        std::fs::write(&inside_file, "x").expect("archivo");
        let other = crate::testutil::TempDir::new("runtime-handler-outside");
        let outside_file = other.path().join("equipo.toml");
        std::fs::write(&outside_file, "x").expect("archivo");
        assert!(inside(dir.path(), &inside_file).is_ok());
        assert!(inside(dir.path(), &outside_file).is_err());
    }
}

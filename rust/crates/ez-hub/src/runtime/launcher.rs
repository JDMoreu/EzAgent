//! Lanzamiento de peces y elefantes sobre herdr-core (diseño §14.2).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ez_arbiter::{
    Delivery as ArbiterDelivery, FishLauncher, InstructionWriter, LaunchRequest, Launched,
    MainAgent,
};
use ez_ctl::{Caller, Tokens};
use ez_team::{
    builtin, library_dir, load_roles, AgentProfile, InstructionContext, LaunchCommand, RoleId,
    Seat,
};
use ez_term::{Delivery as TermDelivery, HcClient, RunSpec, Terminal};
use parking_lot::RwLock;

use crate::runtime::spaces::MainChoice;

const READY_TIMEOUT: Duration = Duration::from_secs(60);
const MISSING_MODEL: &str =
    "El agente principal necesita configurar su modelo en el formulario.";

/// Operaciones sobre terminales que necesita el motor. `HcClient` las cumple;
/// las pruebas usan un doble que no abre procesos.
pub trait Terminals: Send + Sync {
    /// Crea una terminal y entrega la primera línea.
    fn launch_and_deliver(
        &self,
        spec: &RunSpec,
        first_line: &str,
        timeout: Duration,
    ) -> Result<(Terminal, TermDelivery), String>;
    /// Envía texto a una terminal.
    fn send(&self, terminal: &str, text: &str) -> Result<(), String>;
    /// Estado de una terminal.
    fn get(&self, terminal: &str) -> Result<Terminal, String>;
    /// Terminales con esas etiquetas.
    fn list(&self, labels: &BTreeMap<String, String>) -> Result<Vec<Terminal>, String>;
    /// Mata una terminal.
    fn kill(&self, terminal: &str) -> Result<(), String>;
}

/// Adaptador del cliente real de herdr-core.
pub struct HcTerminals(HcClient);

impl HcTerminals {
    /// Envuelve un `HcClient` ya conectado.
    pub fn new(client: HcClient) -> Self {
        Self(client)
    }
}

impl Terminals for HcTerminals {
    fn launch_and_deliver(
        &self,
        spec: &RunSpec,
        first_line: &str,
        timeout: Duration,
    ) -> Result<(Terminal, TermDelivery), String> {
        self.0
            .launch_and_deliver(spec, first_line, timeout)
            .map_err(|error| error.to_string())
    }

    fn send(&self, terminal: &str, text: &str) -> Result<(), String> {
        self.0
            .send(terminal, text)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn get(&self, terminal: &str) -> Result<Terminal, String> {
        self.0.get(terminal).map_err(|error| error.to_string())
    }

    fn list(&self, labels: &BTreeMap<String, String>) -> Result<Vec<Terminal>, String> {
        self.0.list(labels).map_err(|error| error.to_string())
    }

    fn kill(&self, terminal: &str) -> Result<(), String> {
        self.0.kill(terminal).map_err(|error| error.to_string())
    }
}

/// Lanza peces y elefantes y gestiona los tokens de ambos.
pub struct RuntimeLauncher {
    pub(crate) terminals: Arc<dyn Terminals>,
    pub(crate) tokens: Arc<Tokens>,
    pub(crate) profiles: Vec<AgentProfile>,
    pub(crate) url: Arc<RwLock<Option<String>>>,
    pub(crate) exe_dir: PathBuf,
}

impl RuntimeLauncher {
    /// Lanza un agente principal en su worktree y devuelve la dirección hc.
    pub fn launch_main(
        &self,
        agent: &MainAgent,
        choice: &MainChoice,
        token: &str,
    ) -> Result<String, String> {
        if choice.model.trim().is_empty() {
            return Err(MISSING_MODEL.to_string());
        }
        let profile = self.profile(&choice.agent)?;
        let seat = Seat {
            role: RoleId::parse(builtin::DEVELOPER).expect("id del papel"),
            agent: choice.agent.clone(),
            model: choice.model.clone(),
            effort: choice.effort.clone(),
            count: 1,
            stage: 1,
            delegates_to: Vec::new(),
        };
        let command = ez_team::launch::command(profile, &seat);
        let mut extra = BTreeMap::new();
        if command.prelaunch.as_deref() == Some("opencode_state") {
            let effort = choice.effort.as_deref().unwrap_or_default();
            for (name, value) in ez_herdr::prelaunch::prepare_opencode_state(&choice.model, effort) {
                if let Some(text) = value.as_str() {
                    extra.insert(name, text.to_string());
                }
            }
        }
        let mut labels = BTreeMap::new();
        labels.insert("ez".to_string(), "1".to_string());
        labels.insert("ez_space".to_string(), agent.key.space.clone());
        labels.insert("ez_elephant".to_string(), agent.key.elephant.clone());
        labels.insert("ez_role".to_string(), "main".to_string());
        labels.insert("ez_agent".to_string(), choice.agent.clone());
        labels.insert("ez_model".to_string(), choice.model.clone());
        let env = self.environment(&extra, &command, token, "main");
        let spec = RunSpec {
            argv: command.argv,
            cwd: Some(agent.worktree.clone()),
            name: Some(main_agent_name(&agent.key.elephant)),
            labels,
            env,
            size: None,
            no_parent: true,
            no_notify: true,
        };
        let (terminal, delivery) = self.terminals.launch_and_deliver(
            &spec,
            &ez_team::launch::main_first_line(),
            READY_TIMEOUT,
        )?;
        check_delivery(delivery)?;
        Ok(terminal.address)
    }

    fn profile(&self, agent: &str) -> Result<&AgentProfile, String> {
        self.profiles
            .iter()
            .find(|profile| profile.id == agent)
            .ok_or_else(|| format!("No hay un perfil configurado para el agente «{agent}»."))
    }

    fn environment(
        &self,
        request_env: &BTreeMap<String, String>,
        command: &LaunchCommand,
        token: &str,
        role: &str,
    ) -> BTreeMap<String, String> {
        let mut env = request_env.clone();
        env.extend(command.env.clone());
        if let Some(url) = self.url.read().as_ref() {
            env.insert("EZ_URL".to_string(), url.clone());
        }
        env.insert("EZ_TOKEN".to_string(), token.to_string());
        env.insert("EZ_ROLE".to_string(), role.to_string());
        let path = self.path_with_exe();
        env.insert("PATH".to_string(), path);
        env
    }

    fn path_with_exe(&self) -> String {
        let current = std::env::var("PATH")
            .or_else(|_| std::env::var("Path"))
            .unwrap_or_default();
        if current.is_empty() {
            self.exe_dir.to_string_lossy().into_owned()
        } else {
            format!("{};{}", self.exe_dir.display(), current)
        }
    }

    /// Terminales vivas con un juego de etiquetas dado.
    pub(crate) fn list(&self, labels: &BTreeMap<String, String>) -> Result<Vec<Terminal>, String> {
        self.terminals.list(labels)
    }

    /// Mata una terminal.
    pub(crate) fn kill(&self, terminal: &str) -> Result<(), String> {
        self.terminals.kill(terminal)
    }

    /// Envía texto crudo a una terminal (barra del elefante).
    pub(crate) fn send_text(&self, terminal: &str, text: &str) -> Result<(), String> {
        self.terminals.send(terminal, text)
    }
}

impl FishLauncher for RuntimeLauncher {
    fn launch(&self, request: &LaunchRequest) -> Result<Launched, String> {
        let key = caller_from_labels(request);
        // El token se registra antes de abrir el proceso (§14.1).
        self.tokens.register(&request.token, key);
        let profile = self.profile(&request.seat.agent)?;
        let command = ez_team::launch::command(profile, &request.seat);
        let mut extra = BTreeMap::new();
        if command.prelaunch.as_deref() == Some("opencode_state") {
            let effort = request.seat.effort.as_deref().unwrap_or_default();
            for (name, value) in
                ez_herdr::prelaunch::prepare_opencode_state(&request.seat.model, effort)
            {
                if let Some(text) = value.as_str() {
                    extra.insert(name, text.to_string());
                }
            }
        }
        let mut labels = request.labels.clone();
        labels.insert("ez_agent".to_string(), request.seat.agent.clone());
        labels.insert("ez_model".to_string(), request.seat.model.clone());
        labels.insert(
            "ez_role_fish".to_string(),
            request.role.as_str().to_string(),
        );
        let mut env = request.env.clone();
        env.extend(extra);
        for (name, value) in &command.env {
            env.insert(name.clone(), value.clone());
        }
        let env = self.environment(&env, &command, &request.token, "sub");
        let spec = RunSpec {
            argv: command.argv,
            cwd: Some(request.cwd.clone()),
            name: Some(request.hc_name.clone()),
            labels,
            env,
            size: None,
            no_parent: true,
            no_notify: true,
        };
        let (terminal, delivery) =
            self.terminals
                .launch_and_deliver(&spec, &request.first_line, READY_TIMEOUT)?;
        Ok(Launched {
            terminal: terminal.address,
            delivery: map_delivery(delivery),
        })
    }

    fn send(&self, terminal: &str, text: &str) -> Result<(), String> {
        self.terminals.send(terminal, text)
    }

    fn alive(&self, terminal: &str) -> bool {
        self.terminals
            .get(terminal)
            .map(|item| item.state.eq_ignore_ascii_case("live"))
            .unwrap_or(false)
    }

    fn revoke(&self, token: &str) {
        self.tokens.revoke(token);
    }
}

/// Escribe las instrucciones de los peces desde las plantillas de `ez-team`.
pub struct TeamInstructionWriter;

impl InstructionWriter for TeamInstructionWriter {
    fn render(&self, context: &InstructionContext) -> Result<String, String> {
        // DECISION: los papeles de la tarea viven en el worktree; se cargan desde
        // ahí para que sus instrucciones usen el cuerpo definido por la persona.
        let folder = context.folder.join(".ez").join("borrador").join("papeles");
        let roles = load_roles(Some(&folder), &library_dir())
            .map_err(|error| error.to_string())?;
        let def = roles
            .into_iter()
            .find(|def| def.id == context.role)
            .ok_or_else(|| format!("No hay una definición para el papel «{}».", context.role))?;
        ez_team::launch::render_instructions(&def, context).map_err(|error| error.to_string())
    }

    fn first_line(&self, role_name: &str, instructions: &Path) -> String {
        ez_team::launch::first_line(role_name, instructions)
    }
}

fn caller_from_labels(request: &LaunchRequest) -> Caller {
    // DECISION: las etiquetas del árbitro son la fuente de identidad del pez;
    // si falta alguna, se usan valores vacíos y el árbitro rechazará la acción.
    Caller::Fish {
        space: request.labels.get("ez_space").cloned().unwrap_or_default(),
        task: request.labels.get("ez_task").cloned().unwrap_or_default(),
        elephant: request
            .labels
            .get("ez_elephant")
            .cloned()
            .unwrap_or_default(),
        fish: request.labels.get("ez_fish").cloned().unwrap_or_default(),
        attempt: request
            .labels
            .get("ez_attempt")
            .and_then(|value| value.parse().ok())
            .unwrap_or(1),
        parent: request.labels.get("ez_parent").cloned(),
    }
}

fn map_delivery(delivery: TermDelivery) -> ArbiterDelivery {
    match delivery {
        TermDelivery::Delivered => ArbiterDelivery::Delivered,
        TermDelivery::Unconfirmed => ArbiterDelivery::Unconfirmed,
        TermDelivery::Failed(motive) => ArbiterDelivery::Failed(motive),
    }
}

fn check_delivery(delivery: TermDelivery) -> Result<(), String> {
    match delivery {
        TermDelivery::Failed(motive) => Err(motive),
        _ => Ok(()),
    }
}

fn main_agent_name(elephant: &str) -> String {
    let chars = elephant.chars().count();
    let tail: String = elephant.chars().skip(chars.saturating_sub(15)).collect();
    format!("ez-el-{tail}")
}

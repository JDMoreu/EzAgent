//! Pruebas del motor con dobles de terminales y del árbitro.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ez_arbiter::{Caller as ArbiterCaller, Command, Delivery, FishKey, LaunchRequest};
use ez_ctl::{Caller, Handler, Reply, Request, Tokens};
use ez_team::{AgentProfile, Role, Seat, Subscription};
use ez_term::{Delivery as TermDelivery, RunSpec, Terminal};
use parking_lot::{Mutex, RwLock};
use serde_json::{json, Value};

use super::handler::{ArbiterApi, ArbiterHandler};
use super::launcher::{RuntimeLauncher, Terminals};
use super::spaces::SpaceConfig;
use super::HubRuntime;

#[derive(Clone)]
struct LaunchRecord {
    spec: RunSpec,
    first_line: String,
    token_registered: bool,
}

#[derive(Default)]
struct FakeTerminals {
    launches: Mutex<Vec<LaunchRecord>>,
    sent: Mutex<Vec<(String, String)>>,
    killed: Mutex<Vec<String>>,
    terminals: Mutex<BTreeMap<String, Terminal>>,
    tokens: Mutex<Option<Arc<Tokens>>>,
    failing: Mutex<bool>,
}

impl FakeTerminals {
    fn with_tokens(tokens: Arc<Tokens>) -> Arc<Self> {
        Arc::new(Self {
            tokens: Mutex::new(Some(tokens)),
            ..FakeTerminals::default()
        })
    }

    fn add(&self, address: &str, labels: BTreeMap<String, String>, state: &str) {
        let terminal = Terminal {
            address: address.to_string(),
            labels,
            state: state.to_string(),
            ..Terminal::default()
        };
        self.terminals.lock().insert(address.to_string(), terminal);
    }
}

impl Terminals for FakeTerminals {
    fn launch_and_deliver(
        &self,
        spec: &RunSpec,
        first_line: &str,
        _timeout: Duration,
    ) -> Result<(Terminal, TermDelivery), String> {
        let token = spec.env.get("EZ_TOKEN").cloned().unwrap_or_default();
        let registered = self
            .tokens
            .lock()
            .as_ref()
            .is_some_and(|tokens| tokens.lookup(&token).is_some());
        self.launches.lock().push(LaunchRecord {
            spec: spec.clone(),
            first_line: first_line.to_string(),
            token_registered: registered,
        });
        if *self.failing.lock() {
            return Ok((
                Terminal::default(),
                TermDelivery::Failed("no arrancó".to_string()),
            ));
        }
        let address = format!("t{}", self.launches.lock().len());
        let mut terminal = Terminal {
            address: address.clone(),
            state: "live".to_string(),
            ..Terminal::default()
        };
        terminal.labels = spec.labels.clone();
        self.terminals
            .lock()
            .insert(address.clone(), terminal.clone());
        Ok((terminal, TermDelivery::Delivered))
    }

    fn send(&self, terminal: &str, text: &str) -> Result<(), String> {
        self.sent
            .lock()
            .push((terminal.to_string(), text.to_string()));
        Ok(())
    }

    fn get(&self, terminal: &str) -> Result<Terminal, String> {
        self.terminals
            .lock()
            .get(terminal)
            .cloned()
            .ok_or_else(|| "sin terminal".to_string())
    }

    fn list(&self, labels: &BTreeMap<String, String>) -> Result<Vec<Terminal>, String> {
        Ok(self
            .terminals
            .lock()
            .values()
            .filter(|terminal| {
                labels.iter().all(|(name, value)| {
                    terminal.labels.get(name).map(String::as_str) == Some(value.as_str())
                })
            })
            .cloned()
            .collect())
    }

    fn kill(&self, terminal: &str) -> Result<(), String> {
        self.killed.lock().push(terminal.to_string());
        self.terminals.lock().remove(terminal);
        Ok(())
    }
}

fn profile() -> AgentProfile {
    AgentProfile {
        id: "opencode".to_string(),
        name: "OpenCode".to_string(),
        argv: vec!["opencode".to_string(), "--standalone".to_string()],
        models_cmd: None,
        models: vec!["opencode-go/deepseek-v4.1-flash".to_string()],
        family: None,
        effort_args: Vec::new(),
        env: BTreeMap::from([(
            "OPENCODE_CONFIG_CONTENT".to_string(),
            "{\"model\":\"{modelo}{variante}\"}".to_string(),
        )]),
        prelaunch: None,
    }
}

fn seat() -> Seat {
    Seat {
        role: Role::Developer,
        agent: "opencode".to_string(),
        model: "opencode-go/deepseek-v4.1-flash".to_string(),
        effort: Some("max".to_string()),
        count: 1,
    }
}

fn labels() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("ez".to_string(), "1".to_string()),
        ("ez_space".to_string(), "s1".to_string()),
        ("ez_task".to_string(), "t1".to_string()),
        ("ez_role".to_string(), "fish".to_string()),
        ("ez_fish".to_string(), "dev-r1-1".to_string()),
        ("ez_attempt".to_string(), "1".to_string()),
    ])
}

fn launcher(terminals: Arc<FakeTerminals>, tokens: Arc<Tokens>) -> Arc<RuntimeLauncher> {
    Arc::new(RuntimeLauncher {
        terminals,
        tokens,
        profiles: vec![profile()],
        url: Arc::new(RwLock::new(Some("http://127.0.0.1:47830".to_string()))),
        exe_dir: PathBuf::from("C:\\ez"),
    })
}

fn launch_request(token: &str) -> LaunchRequest {
    LaunchRequest {
        fish: "dev-r1-1".to_string(),
        hc_name: "ez-t1-dev".to_string(),
        role: Role::Developer,
        seat: seat(),
        cwd: PathBuf::from("C:\\proyecto"),
        env: BTreeMap::from([("EXTRA".to_string(), "valor".to_string())]),
        labels: labels(),
        first_line: "Eres un pez dorado.".to_string(),
        token: token.to_string(),
    }
}

fn space_config() -> SpaceConfig {
    space_config_in(PathBuf::from("C:\\proyecto"))
}

fn space_config_in(folder: PathBuf) -> SpaceConfig {
    SpaceConfig {
        id: "s1".to_string(),
        name: "Ez".to_string(),
        folder,
        agent: "opencode".to_string(),
        model: "opencode-go/deepseek-v4.1-flash".to_string(),
        effort: None,
    }
}

#[test]
fn launcher_registers_the_token_before_launching_and_builds_the_environment() {
    let tokens = Arc::new(Tokens::new());
    let terminals = FakeTerminals::with_tokens(Arc::clone(&tokens));
    let launcher = launcher(Arc::clone(&terminals), Arc::clone(&tokens));
    let launched =
        ez_arbiter::FishLauncher::launch(&*launcher, &launch_request("tk")).expect("lanzar");
    assert_eq!(launched.delivery, Delivery::Delivered);
    let records = terminals.launches.lock();
    let record = &records[0];
    assert!(
        record.token_registered,
        "el token debe estar registrado antes de abrir"
    );
    assert_eq!(
        record.spec.env.get("EZ_TOKEN").map(String::as_str),
        Some("tk")
    );
    assert_eq!(
        record.spec.env.get("EZ_URL").map(String::as_str),
        Some("http://127.0.0.1:47830")
    );
    assert_eq!(
        record.spec.env.get("EZ_ROLE").map(String::as_str),
        Some("fish")
    );
    assert_eq!(
        record.spec.env.get("EXTRA").map(String::as_str),
        Some("valor")
    );
    assert!(record
        .spec
        .env
        .get("PATH")
        .is_some_and(|path| path.starts_with("C:\\ez")));
    assert_eq!(
        record
            .spec
            .env
            .get("OPENCODE_CONFIG_CONTENT")
            .map(String::as_str),
        Some("{\"model\":\"opencode-go/deepseek-v4.1-flash#max\"}")
    );
    assert_eq!(
        record.spec.labels.get("ez_agent").map(String::as_str),
        Some("opencode")
    );
    assert_eq!(record.first_line, "Eres un pez dorado.");
    drop(records);
    assert!(matches!(
        tokens.lookup("tk"),
        Some(Caller::Fish { attempt: 1, .. })
    ));
    ez_arbiter::FishLauncher::revoke(&*launcher, "tk");
    assert!(tokens.lookup("tk").is_none());
}

#[test]
fn launcher_maps_a_failed_delivery() {
    let tokens = Arc::new(Tokens::new());
    let terminals = FakeTerminals::with_tokens(Arc::clone(&tokens));
    *terminals.failing.lock() = true;
    let launcher = launcher(terminals, tokens);
    let launched =
        ez_arbiter::FishLauncher::launch(&*launcher, &launch_request("tk2")).expect("respuesta");
    assert_eq!(
        launched.delivery,
        Delivery::Failed("no arrancó".to_string())
    );
}

#[test]
fn alive_uses_the_live_terminal_state() {
    let tokens = Arc::new(Tokens::new());
    let terminals = FakeTerminals::with_tokens(Arc::clone(&tokens));
    terminals.add("live", BTreeMap::new(), "live");
    terminals.add("dead", BTreeMap::new(), "finished");
    let launcher = launcher(terminals, tokens);
    assert!(ez_arbiter::FishLauncher::alive(&*launcher, "live"));
    assert!(!ez_arbiter::FishLauncher::alive(&*launcher, "dead"));
}

fn elephant_labels() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("ez".to_string(), "1".to_string()),
        ("ez_space".to_string(), "s1".to_string()),
        ("ez_role".to_string(), "elephant".to_string()),
    ])
}

#[test]
fn ensure_elephant_reuses_a_live_terminal() {
    let dir = crate::testutil::TempDir::new("runtime-reuse");
    let tokens = Arc::new(Tokens::new());
    let terminals = FakeTerminals::with_tokens(Arc::clone(&tokens));
    terminals.add("e1", elephant_labels(), "live");
    let runtime = HubRuntime::with_parts(
        vec![space_config()],
        launcher(terminals, tokens),
        Arc::new(Tokens::new()),
        super::spaces::ElephantTokens::load(dir.path().join("elefantes.json")),
        false,
    );
    assert_eq!(runtime.ensure_elephant("s1").expect("elefante"), "e1");
}

#[test]
fn ensure_elephant_relaunches_when_required_and_launches_when_absent() {
    let dir = crate::testutil::TempDir::new("runtime-relaunch");
    let tokens = Arc::new(Tokens::new());
    let terminals = FakeTerminals::with_tokens(Arc::clone(&tokens));
    terminals.add("e1", elephant_labels(), "live");
    let saved = super::spaces::ElephantTokens::load(dir.path().join("elefantes.json"));
    let runtime = HubRuntime::with_parts(
        vec![space_config()],
        launcher(Arc::clone(&terminals), Arc::clone(&tokens)),
        Arc::clone(&tokens),
        saved,
        true,
    );
    assert_eq!(runtime.ensure_elephant("s1").expect("elefante"), "t1");
    assert_eq!(terminals.killed.lock().as_slice(), &["e1".to_string()]);
    assert_eq!(terminals.launches.lock().len(), 1);
    let saved = runtime.elephant_tokens.get("s1").expect("token guardado");
    assert!(matches!(
        tokens.lookup(&saved),
        Some(Caller::Elephant { .. })
    ));
}

#[test]
fn space_fish_reports_address_role_agent_model_and_status() {
    let dir = crate::testutil::TempDir::new("runtime-fish");
    let tokens = Arc::new(Tokens::new());
    let terminals = FakeTerminals::with_tokens(Arc::clone(&tokens));
    let mut labels = labels();
    labels.insert("ez_agent".to_string(), "opencode".to_string());
    labels.insert("ez_model".to_string(), "opencode-go/x".to_string());
    labels.insert("ez_role_fish".to_string(), "developer".to_string());
    terminals.add("f1", labels, "live");
    let runtime = HubRuntime::with_parts(
        vec![space_config()],
        launcher(terminals, tokens),
        Arc::new(Tokens::new()),
        super::spaces::ElephantTokens::load(dir.path().join("elefantes.json")),
        false,
    );
    let fish = runtime.space_fish("s1").expect("peces");
    assert_eq!(fish.len(), 1);
    assert_eq!(fish[0].address, "f1");
    assert_eq!(fish[0].role, "developer");
    assert_eq!(fish[0].agent, "opencode");
    assert_eq!(fish[0].model, "opencode-go/x");
    assert_eq!(fish[0].status, "live");
}

#[test]
fn elephant_send_uses_the_raw_text() {
    let dir = crate::testutil::TempDir::new("runtime-send");
    let tokens = Arc::new(Tokens::new());
    let terminals = FakeTerminals::with_tokens(Arc::clone(&tokens));
    terminals.add("e1", elephant_labels(), "live");
    let runtime = HubRuntime::with_parts(
        vec![space_config()],
        launcher(Arc::clone(&terminals), Arc::clone(&tokens)),
        Arc::new(Tokens::new()),
        super::spaces::ElephantTokens::load(dir.path().join("elefantes.json")),
        false,
    );
    runtime
        .elephant_send("s1", "hola\n\nmundo")
        .expect("enviar");
    assert_eq!(
        terminals.sent.lock().as_slice(),
        &[("e1".to_string(), "hola\n\nmundo".to_string())]
    );
}

mod handler;

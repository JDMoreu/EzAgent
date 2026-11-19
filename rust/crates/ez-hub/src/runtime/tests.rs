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

#[derive(Default)]
struct FakeArbiter {
    calls: Mutex<Vec<(ArbiterCaller, Command)>>,
    event: Mutex<Option<ez_arbiter::ElephantEvent>>,
}

impl ArbiterApi for FakeArbiter {
    fn execute(
        &self,
        caller: &ArbiterCaller,
        command: Command,
    ) -> Result<ez_arbiter::Outcome, ez_arbiter::Rejection> {
        self.calls.lock().push((caller.clone(), command));
        Ok(ez_arbiter::Outcome::new("hecho"))
    }

    fn wait(&self, _space: &str, _timeout: Duration) -> Option<ez_arbiter::ElephantEvent> {
        self.event.lock().clone()
    }

    fn snapshot(&self, _space: &str) -> Option<Value> {
        Some(json!({ "title": "Tarea de prueba" }))
    }
}

fn handler(api: Arc<FakeArbiter>) -> (ArbiterHandler, crate::testutil::TempDir) {
    let dir = crate::testutil::TempDir::new("runtime-handler");
    let brief = dir.path().join("encargo.md");
    let criteria = dir.path().join("criterios.toml");
    std::fs::write(&brief, "encargo").expect("brief");
    std::fs::write(&criteria, "[[criterio]]\n").expect("criteria");
    let h = ArbiterHandler::new(
        api,
        vec![profile()],
        vec![Subscription {
            name: "Todo".to_string(),
            agents: vec!["opencode".to_string()],
        }],
        vec![space_config_in(dir.path().to_path_buf())],
    );
    (h, dir)
}

fn elephant_call() -> Caller {
    Caller::Elephant {
        space: "s1".to_string(),
    }
}

fn fish_call() -> Caller {
    Caller::Fish {
        space: "s1".to_string(),
        task: "t1".to_string(),
        fish: "dev-r1-1".to_string(),
        attempt: 1,
    }
}

fn call(h: &ArbiterHandler, caller: &Caller, command: &str, args: Value) -> Reply {
    h.handle(
        caller,
        &Request {
            command: command.to_string(),
            args,
        },
    )
}

#[test]
fn handler_translates_every_elephant_command_and_the_fish_caller() {
    let api = Arc::new(FakeArbiter::default());
    let (h, dir) = handler(Arc::clone(&api));
    let file = dir.path().join("archivo.md").to_string_lossy().into_owned();
    assert!(call(&h, &elephant_call(), "status", json!({})).ok);
    assert!(
        call(
            &h,
            &elephant_call(),
            "task.new",
            json!({
                "title": "Tarea", "brief": file, "criteria": file
            })
        )
        .ok
    );
    assert!(call(&h, &elephant_call(), "round.start", json!({})).ok);
    assert!(
        call(
            &h,
            &elephant_call(),
            "fish.new",
            json!({
                "role": "developer", "brief": file
            })
        )
        .ok
    );
    assert!(
        call(
            &h,
            &elephant_call(),
            "fish.send",
            json!({
                "fish": "dev-r1-1", "file": file
            })
        )
        .ok
    );
    assert!(call(&h, &elephant_call(), "verify", json!({})).ok);
    assert!(call(&h, &elephant_call(), "round.close", json!({})).ok);
    assert!(call(&h, &elephant_call(), "task.close", json!({})).ok);
    assert!(call(&h, &elephant_call(), "pause", json!({ "reason": "espera" })).ok);
    assert!(
        call(
            &h,
            &fish_call(),
            "report",
            json!({ "verdict": "pass", "file": file, "blocking": 0, "major": 0, "minor": 0 })
        )
        .ok
    );
    let calls = api.calls.lock();
    assert_eq!(calls.len(), 10);
    assert_eq!(
        calls[0].0,
        ArbiterCaller::Elephant {
            space: "s1".to_string()
        }
    );
    assert_eq!(
        calls[9].0,
        ArbiterCaller::Fish(FishKey {
            space: "s1".to_string(),
            task: "t1".to_string(),
            fish: "dev-r1-1".to_string(),
            attempt: 1,
        })
    );
    assert!(matches!(
        calls[3].1,
        Command::FishNew {
            role: Role::Developer,
            ..
        }
    ));
    assert!(matches!(calls[9].1, Command::Report { .. }));
}

#[test]
fn handler_wait_without_and_with_an_event() {
    let api = Arc::new(FakeArbiter::default());
    let (h, _) = handler(Arc::clone(&api));
    let reply = call(&h, &elephant_call(), "wait", json!({ "timeout": 1 }));
    assert!(reply.ok);
    assert_eq!(reply.message, "Sin novedades; vuelve a ejecutar `ez wait`.");
    assert_eq!(reply.data, json!({ "event": null }));

    *api.event.lock() = Some(ez_arbiter::ElephantEvent::new(
        "fish_reported",
        "s1",
        1.0,
        "El pez informó.",
        json!({}),
    ));
    let reply = call(&h, &elephant_call(), "wait", json!({}));
    assert!(reply.ok);
    assert_eq!(reply.message, "El pez informó.");
    assert!(reply
        .data
        .get("event")
        .is_some_and(|value| !value.is_null()));
}

#[test]
fn handler_rejects_unknown_commands_and_bad_arguments() {
    let api = Arc::new(FakeArbiter::default());
    let (h, _) = handler(api);
    assert!(!call(&h, &elephant_call(), "otro", json!({})).ok);
    assert!(!call(&h, &elephant_call(), "task.new", json!({ "title": "" })).ok);
    assert!(
        !call(
            &h,
            &fish_call(),
            "report",
            json!({
                "verdict": "mal", "file": "x", "blocking": 0, "major": 0, "minor": 0
            })
        )
        .ok
    );
}

#[test]
fn handler_team_propose_rejects_an_invalid_team() {
    let api = Arc::new(FakeArbiter::default());
    let (h, dir) = handler(Arc::clone(&api));
    let file = dir.path().join("equipo.toml");
    std::fs::write(
        &file,
        "[[papel]]\npapel = \"elephant\"\nagente = \"a\"\nmodelo = \"m\"\n",
    )
    .expect("equipo");
    let reply = call(
        &h,
        &elephant_call(),
        "team.propose",
        json!({ "file": file.to_string_lossy() }),
    );
    assert!(!reply.ok, "{reply:?}");
    assert!(api.calls.lock().is_empty());
}

#[test]
fn handler_inventory_returns_text_and_json() {
    let api = Arc::new(FakeArbiter::default());
    let (h, _) = handler(api);
    let reply = call(&h, &elephant_call(), "inventory", json!({}));
    assert!(reply.ok);
    assert!(reply.data.is_array());
}

fn dummy_profile(dir: &crate::testutil::TempDir, id: &str, model: &str) -> AgentProfile {
    let program = dir.path().join(id);
    std::fs::write(&program, "doble").expect("programa falso");
    AgentProfile {
        id: id.to_string(),
        name: id.to_string(),
        argv: vec![program.to_string_lossy().into_owned()],
        models_cmd: None,
        models: vec![model.to_string()],
        family: None,
        effort_args: Vec::new(),
        env: BTreeMap::new(),
        prelaunch: None,
    }
}

#[test]
fn handler_team_propose_accepts_a_valid_team() {
    let dir = crate::testutil::TempDir::new("runtime-team-valid");
    let team = dir.path().join("equipo.toml");
    std::fs::write(
        &team,
        r#"[[papel]]
papel = "planner"
agente = "opencode"
modelo = "opencode-go/deepseek-v4.1-flash"

[[papel]]
papel = "developer"
agente = "claude"
modelo = "opus"

[[papel]]
papel = "qa"
agente = "codex"
modelo = "gpt-6"

[[papel]]
papel = "qa_validator"
agente = "opencode"
modelo = "qwen4-max"
"#,
    )
    .expect("equipo");
    let mut opencode = dummy_profile(&dir, "opencode", "opencode-go/deepseek-v4.1-flash");
    opencode.models.push("qwen4-max".to_string());
    let profiles = vec![
        opencode,
        dummy_profile(&dir, "claude", "opus"),
        dummy_profile(&dir, "codex", "gpt-6"),
    ];
    let api = Arc::new(FakeArbiter::default());
    let h = ArbiterHandler::new(
        Arc::clone(&api) as Arc<dyn ArbiterApi>,
        profiles,
        vec![Subscription {
            name: "Todo".to_string(),
            agents: vec![
                "opencode".to_string(),
                "claude".to_string(),
                "codex".to_string(),
            ],
        }],
        vec![space_config_in(dir.path().to_path_buf())],
    );
    let reply = call(
        &h,
        &elephant_call(),
        "team.propose",
        json!({ "file": team.to_string_lossy() }),
    );
    assert!(reply.ok, "{reply:?}");
    assert!(matches!(api.calls.lock()[0].1, Command::TeamPropose { .. }));
}

fn real_arbiter(dir: &crate::testutil::TempDir) -> Arc<ez_arbiter::Arbiter> {
    let mut command = std::process::Command::new("git");
    command.args(["init"]).current_dir(dir.path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let status = command.status().expect("git init");
    assert!(status.success());
    let tokens = Arc::new(Tokens::new());
    let terminals = FakeTerminals::with_tokens(Arc::clone(&tokens));
    let launcher = launcher(terminals, tokens);
    let arbiter = ez_arbiter::Arbiter::new(
        dir.path().to_path_buf(),
        launcher as Arc<dyn ez_arbiter::FishLauncher>,
        Arc::new(super::launcher::TeamInstructionWriter),
        Arc::new(ez_arbiter::SystemClock),
    )
    .expect("árbitro");
    arbiter.register_space(ez_arbiter::Space {
        id: "s1".to_string(),
        name: "Ez".to_string(),
        folder: dir.path().to_path_buf(),
    });
    Arc::new(arbiter)
}

#[test]
fn handler_with_the_real_arbiter_creates_a_task_and_wakes_the_elephant() {
    let dir = crate::testutil::TempDir::new("runtime-real-arbiter");
    let brief = dir.path().join("encargo.md");
    let criteria = dir.path().join("criterios.toml");
    std::fs::write(&brief, "Haz la tarea.").expect("brief");
    std::fs::write(
        &criteria,
        "[[criterio]]\nid = \"C1\"\ndescripcion = \"Verdad\"\ncomando = [\"hostname\"]\n",
    )
    .expect("criterios");
    let arbiter = real_arbiter(&dir);
    let h = ArbiterHandler::new(
        Arc::clone(&arbiter) as Arc<dyn ArbiterApi>,
        vec![profile()],
        vec![Subscription {
            name: "Todo".to_string(),
            agents: vec!["opencode".to_string()],
        }],
        vec![space_config()],
    );
    let created = call(
        &h,
        &elephant_call(),
        "task.new",
        json!({
            "title": "Tarea real",
            "brief": brief.to_string_lossy(),
            "criteria": criteria.to_string_lossy(),
        }),
    );
    assert!(created.ok, "{created:?}");
    assert!(created.data.get("task").is_some());
    let status = call(&h, &elephant_call(), "status", json!({}));
    assert!(status.ok, "{status:?}");
    let waiting = call(&h, &elephant_call(), "wait", json!({ "timeout": 1 }));
    assert!(waiting.ok, "{waiting:?}");
    assert!(waiting.data.get("event").is_some());
}

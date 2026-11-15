//! Casos de `service.handle` (transcripciones de `ezs`), `recovery.resolve` y
//! el CLI (`cli.*`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use ez_parity::fixtures::FixtureCase;
use ez_swarm::db::{Db, DbError};
use ez_swarm::recovery::{Recovery, RecoveryOptions};
use ez_swarm::service::SwarmService;
use ez_swarm::supervisor::{LaunchRequest, SwarmHost};
use serde_json::{json, Map, Value};

use super::support::{base, dump_board, input_array, input_str, normalize_text, seed, TempHome};

// ---------------------------------------------------------------- ezs

pub fn service_case(case: &FixtureCase, home: &TempHome) -> Result<Value, String> {
    let input = &case.input;
    let id = input_str(input, "id");
    let spec = input.get("board").cloned().unwrap_or_else(|| json!({}));
    let store = home.board(&case.case);
    let swarm = seed(&store, &spec).unwrap_or_else(|error| panic!("siembra de ezs/{id}: {error}"));
    let service = SwarmService::new(store, "ezs", Arc::new(base));
    let mut tokens: HashMap<String, String> = HashMap::new();
    for member in service
        .store
        .members(&swarm)
        .unwrap_or_else(|error| panic!("miembros de ezs/{id}: {error}"))
    {
        let token = service
            .issue_token(&swarm, &member.name)
            .unwrap_or_else(|error| panic!("token de ezs/{id}: {error}"));
        tokens.insert(member.name, token);
    }
    let who = input_str(input, "token_of");
    let token = tokens
        .get(who)
        .unwrap_or_else(|| panic!("asiento desconocido en ezs/{id}: {who}"));
    for before in input_array(input, "before") {
        let argv = argv_of(before);
        service
            .handle(token, &argv)
            .unwrap_or_else(|error| panic!("paso previo de ezs/{id}: {error}"));
    }
    if input
        .get("closed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        service.close();
    }
    let used = if input
        .get("bad_token")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "token-de-otro-enjambre".to_string()
    } else {
        token.clone()
    };
    let reply = service
        .handle(&used, &argv_of(&input["argv"]))
        .unwrap_or_else(|error| panic!("comando de ezs/{id}: {error}"));
    let text = normalize_text(&reply.text, &swarm, home.path());
    let mut payload = json!({
        "ok": reply.ok,
        "exit": if reply.ok { 0 } else { 1 },
        "stdout": if reply.ok { text.clone() } else { String::new() },
        "stderr": if reply.ok { String::new() } else { text },
    });
    if input.get("dump").and_then(Value::as_bool).unwrap_or(true) {
        payload["board"] = dump_board(&service.store, &swarm, home.path());
    }
    Ok(payload)
}

fn argv_of(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

// ------------------------------------------------------------ recovery

/// Herdr falso: pantalla fija y agentes conocidos, sin red ni ventanas.
struct FakeHerdr {
    screen: String,
    agents: Vec<String>,
}

impl SwarmHost for FakeHerdr {
    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, String> {
        Ok(Vec::new())
    }

    fn get_agent(&self, _address: &str) -> Result<Map<String, Value>, String> {
        Err("no existe".to_string())
    }

    fn read_agent(&self, _address: &str, _lines: i64) -> Result<String, String> {
        Ok(self.screen.clone())
    }

    fn send_task(&self, _address: &str, _line: &str) -> Result<Map<String, Value>, String> {
        let mut row = Map::new();
        row.insert("ok".to_string(), Value::Bool(true));
        Ok(row)
    }

    fn run_agent(
        &self,
        _agent: &str,
        _task: &str,
        _request: LaunchRequest,
    ) -> Result<Map<String, Value>, String> {
        Err("no se lanza".to_string())
    }

    fn resolve_agent(&self, name: &str) -> Option<String> {
        self.agents
            .iter()
            .find(|agent| agent.as_str() == name)
            .cloned()
    }

    fn swarm_args(&self, _agent: &str) -> Vec<String> {
        Vec::new()
    }

    fn save_context(&self, _text: &str) -> Option<String> {
        None
    }

    fn default_cwd(&self) -> String {
        String::new()
    }

    fn agent_names(&self) -> Vec<String> {
        self.agents.clone()
    }
}

pub fn recovery_case(case: &FixtureCase, home: &TempHome) -> Result<Value, String> {
    let input = &case.input;
    let id = case.case.rsplit('/').next().unwrap_or("");
    let spec = input.get("board").cloned().unwrap_or_else(|| json!({}));
    let store = home.board(&case.case);
    let swarm =
        seed(&store, &spec).unwrap_or_else(|error| panic!("siembra de recovery/{id}: {error}"));
    let asked: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let announced: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let (recovery, service) = build_recovery(store, home, id, &asked, &announced);
    let text = recovery
        .resolve(
            input.get("incident").and_then(Value::as_i64).unwrap_or(1),
            input_str(input, "choice"),
            input_str(input, "agent"),
            input_str(input, "model"),
            input_str(input, "effort"),
        )
        .map_err(|error: DbError| normalize_text(&error.to_string(), &swarm, home.path()))?;
    let shown = |items: &Arc<Mutex<Vec<String>>>| {
        items
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .map(|text| Value::String(normalize_text(text, &swarm, home.path())))
            .collect::<Vec<_>>()
    };
    Ok(json!({
        "text": normalize_text(&text, &swarm, home.path()),
        "asked": shown(&asked),
        "announced": shown(&announced),
        "board": dump_board(&service.store, &swarm, home.path()),
    }))
}

type Texts = Arc<Mutex<Vec<String>>>;

fn build_recovery(
    store: Db,
    home: &TempHome,
    id: &str,
    asked: &Texts,
    announced: &Texts,
) -> (Recovery, Arc<SwarmService>) {
    let service = Arc::new(SwarmService::new(store, "ezs", Arc::new(base)));
    let host = Arc::new(FakeHerdr {
        screen: "pantalla de la sesión anterior".to_string(),
        agents: ["claude", "opencode", "codex"]
            .iter()
            .map(|agent| agent.to_string())
            .collect(),
    });
    let sink = Arc::clone(asked);
    let ask = Arc::new(move |text: &str| {
        sink.lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(text.to_string());
    });
    let sink = Arc::clone(announced);
    let announce = Arc::new(move |text: &str| {
        sink.lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(text.to_string());
    });
    let recovery = Recovery::with_options(
        Arc::clone(&service),
        host,
        home.path().join("handoff").join(id),
        announce,
        ask,
        RecoveryOptions {
            reset_launches: Arc::new(|_, _| {}),
            wall: Arc::new(base),
            clock: Arc::new(base),
        },
    );
    (recovery, service)
}

// -------------------------------------------------------------------- cli

const ENV_KEYS: [&str; 7] = [
    "EZS_ENDPOINT",
    "EZS_SEAT",
    "EZS_TOKEN",
    "APPDATA",
    "XDG_CONFIG_HOME",
    "HOME",
    "USERPROFILE",
];

struct EnvGuard {
    saved: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    fn capture() -> Self {
        Self {
            saved: ENV_KEYS
                .iter()
                .map(|key| ((*key).to_string(), std::env::var(key).ok()))
                .collect(),
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

/// Entorno del caso: el perfil congelado del oráculo más lo que pida la entrada.
fn case_env(case_env: Option<&Value>, home: &Path) -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("APPDATA".to_string(), home.to_string_lossy().into_owned());
    if let Some(entries) = case_env.and_then(Value::as_object) {
        for (key, value) in entries {
            match value.as_str() {
                Some(text) => {
                    env.insert(key.clone(), text.replace("<tmp>", &home.to_string_lossy()));
                }
                None => {
                    env.remove(key);
                }
            }
        }
    }
    env
}

fn lookup(env: &HashMap<String, String>) -> impl Fn(&str) -> Option<String> + '_ {
    move |name| env.get(name).cloned()
}

pub fn cli_case(case: &FixtureCase, home: &TempHome) -> Result<Value, String> {
    let input = &case.input;
    let name = case.case.rsplit('/').next().unwrap_or("");
    match case.function.as_str() {
        "cli.endpoint_path" => {
            let env = case_env(input.get("env"), home.path());
            let path = ez_swarm::cli::endpoint_path_from(&lookup(&env));
            Ok(json!({ "path": normalize_text(&path, "", home.path()) }))
        }
        "cli._read_port" => {
            let folder = home.path().join("cli").join(name);
            std::fs::create_dir_all(&folder).expect("carpeta del CLI");
            let endpoint = folder.join("endpoint.json");
            if let Some(content) = input.get("endpoint").and_then(Value::as_str) {
                std::fs::write(&endpoint, content).expect("endpoint.json del CLI");
            }
            let mut env = case_env(None, home.path());
            env.insert(
                "EZS_ENDPOINT".to_string(),
                endpoint.to_string_lossy().into_owned(),
            );
            let port = ez_swarm::cli::read_port_from(&lookup(&env));
            Ok(json!({ "port": port }))
        }
        "cli.main" => cli_main(case, home, name),
        other => Err(format!("caso sin arnés: {other}")),
    }
}

fn cli_main(case: &FixtureCase, home: &TempHome, name: &str) -> Result<Value, String> {
    let input = &case.input;
    let folder = home.path().join("cli").join(format!("main-{name}"));
    std::fs::create_dir_all(&folder).expect("carpeta del CLI");
    let file = input
        .get("endpoint")
        .and_then(Value::as_str)
        .unwrap_or("endpoint.json");
    let endpoint = folder.join(file);
    if let Some(content) = input.get("content").and_then(Value::as_str) {
        std::fs::write(&endpoint, content).expect("endpoint del CLI");
    }
    let mut env = case_env(input.get("env"), home.path());
    env.insert(
        "EZS_ENDPOINT".to_string(),
        endpoint.to_string_lossy().into_owned(),
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = ez_swarm::cli::main_with(
        &argv_of(&input["argv"]),
        &lookup(&env),
        &mut stdout,
        &mut stderr,
    );
    Ok(json!({
        "exit": code,
        "stdout": normalize_text(&String::from_utf8_lossy(&stdout), "", home.path()),
        "stderr": normalize_text(&String::from_utf8_lossy(&stderr), "", home.path()),
    }))
}

/// El grupo `cli` emula el perfil temporal congelado del oráculo.
pub fn with_frozen_profile(
    home: &TempHome,
    run: impl FnOnce() -> Result<Value, String>,
) -> Result<Value, String> {
    let _guard = EnvGuard::capture();
    for key in [
        "EZS_ENDPOINT",
        "EZS_SEAT",
        "EZS_TOKEN",
        "APPDATA",
        "XDG_CONFIG_HOME",
    ] {
        std::env::remove_var(key);
    }
    let path = home.path().to_string_lossy().into_owned();
    std::env::set_var("HOME", &path);
    std::env::set_var("USERPROFILE", &path);
    run()
}

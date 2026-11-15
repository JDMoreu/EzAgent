//! Arranque de un escenario: app real, costuras falsas y traza comparable.
//!
//! Orden de `run_scenario` de `parity/replay/run_py.py`: perfil temporal,
//! servicios falsos, ajustes, arranque de la app, guion dentro del bucle de la
//! interfaz, y cierre con `app.end`, volcado del chat, auditoría y aserciones.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_chat::store::ChatStore;
use ez_core::config::VoiceConfig;
use ez_core::pyloads::loads;
use ez_core::pyvalue::{py_splitlines, python_str};
use ez_hud::win::ui_loop::UiLoop;
use ez_llm::messenger::AgentRuntimeApi;
use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::app::core::{AppParts, AppTrace, VoiceApp};

use super::driver::{handlers, Driver, Snapshot, Step};
use super::expect::{check, Context};
use super::hud::NoWindows;
use super::mic::ReplayMic;
use super::recording::Recorder;
use super::scenario::{Paths, Scenario};
use super::services::Services;
use super::speaker::ReplaySpeaker;
use super::trace::Trace;
use super::write_trace;

/// Variables que el arnés mueve al perfil temporal (y devuelve al terminar).
const MANAGED_ENV: [&str; 13] = [
    "HOME",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "TMP",
    "TEMP",
    "LLM_PROVIDER",
    "LLM_API_STYLE",
    "LLM_MODEL",
    "LLM_BASE_URL",
    "LLM_API_KEY",
    "WHISPER_URL",
    "FAKE_HC_STATE",
];

/// Resultado de una corrida.
pub struct Outcome {
    pub id: String,
    pub stem: String,
    pub lines: Vec<String>,
    pub errors: Vec<String>,
    pub seconds: f64,
    pub trace_path: PathBuf,
}

/// Corre el escenario `path` y deja la traza en `trace_out` (o en el temporal).
pub fn run_path(path: &Path, paths: &Paths, trace_out: Option<&Path>) -> Result<Outcome, String> {
    let started = Instant::now();
    let mut scenario = Scenario::load(path)?;
    // DECISION: el perfil temporal no se borra al terminar (la puerta prohíbe
    // `remove_dir_all`); cada corrida usa un nombre único y queda para depurar.
    let home = new_home()?;
    std::fs::create_dir_all(&home)
        .map_err(|error| format!("no pude crear {}: {error}", home.display()))?;
    let guard = EnvGuard::freeze(&home);
    let mut services = Services::new(&paths.fakes);
    let result = prepare_and_run(&mut scenario, paths, &home);
    services.stop();
    drop(guard);
    let run = result?;
    // DECISION: sin `--trace-out` la traza va al temporal del sistema, no al
    // golden: `parity/replay/golden` es de solo lectura en esta fase.
    let target = trace_out.map_or_else(|| default_trace(&scenario.stem), Path::to_path_buf);
    write_trace(&target, &run.lines)?;
    Ok(Outcome {
        id: scenario.id.clone(),
        stem: scenario.stem.clone(),
        lines: run.lines,
        errors: run.errors,
        seconds: started.elapsed().as_secs_f64(),
        trace_path: target,
    })
}

/// Servicios, perfil y guion dentro del perfil temporal.
fn prepare_and_run(
    scenario: &mut Scenario,
    paths: &Paths,
    home: &Path,
) -> Result<RunOutput, String> {
    let mut tokens = std::collections::BTreeMap::new();
    tokens.insert("home".to_string(), slash(home));
    tokens.insert(
        "workspace".to_string(),
        slash(&ez_core::dirs::home().join("EzAgentWorkspace")),
    );
    tokens.insert("fakes".to_string(), slash(&paths.fakes));
    scenario.substitute(&tokens);
    let body = scenario
        .data
        .get("setup")
        .and_then(|setup| setup.get("web"))
        .and_then(Value::as_object)
        .map(|page| page.get("body").map(python_str).unwrap_or_default());
    let mut web = match body {
        Some(body) => Some(super::web::LocalPage::start(&body)?),
        None => None,
    };
    if let Some(page) = &web {
        tokens.insert("web_url".to_string(), page.url().to_string());
        scenario.substitute(&tokens);
    }
    let mut services = Services::new(&paths.fakes);
    let result = start_and_script(scenario, paths, home, &mut services);
    services.stop();
    if let Some(page) = web.as_mut() {
        page.stop();
    }
    result
}

/// Arranca los falsos, prepara el perfil y ejecuta la app.
fn start_and_script(
    scenario: &Scenario,
    paths: &Paths,
    home: &Path,
    services: &mut Services,
) -> Result<RunOutput, String> {
    let llm_port = services.start("fake-llm", &scenario.block("llm"), home)?;
    let whisper_port = services.start("fake-whisper", &scenario.block("whisper"), home)?;
    let settings = scenario.settings(llm_port, whisper_port, &paths.fakes);
    let config_path = scenario.prepare_home(home, &settings)?;
    scenario.setup_environment(home, &paths.fakes)?;
    run_app(scenario, paths, home, &config_path, &settings)
}

/// Piezas falsas de la corrida, compartidas por la app y el guion.
struct Rig {
    app: VoiceApp,
    mic: Arc<ReplayMic>,
    speaker: Arc<ReplaySpeaker>,
    trace: Arc<Trace>,
    audit: Option<PathBuf>,
}

/// App real sin hardware: dobles de micrófono, altavoz y HUD.
fn run_app(
    scenario: &Scenario,
    paths: &Paths,
    home: &Path,
    config_path: &Path,
    settings: &serde_json::Map<String, Value>,
) -> Result<RunOutput, String> {
    let rig = build_rig(scenario, paths, home, config_path, settings)?;
    let trace = Arc::clone(&rig.trace);
    trace.record("app.start", json!({"scenario": scenario.id}));
    let driver = Arc::new(Mutex::new(Driver::new(
        rig.app.clone(),
        Arc::clone(&rig.mic),
        Arc::clone(&rig.speaker),
        Arc::clone(&trace),
        home.join("requests.jsonl"),
        Some(home.join("hc.json")),
        &scenario.data,
    )));
    let failure = Arc::new(Mutex::new(None::<String>));
    let mut scripter = start_scripter(scenario, home, &trace);
    let mic_handle = drive(&rig, &driver, &failure, scenario)?;
    // La instantánea es del final del guion, antes de colgar (como `root.final`).
    let running = {
        let driver = driver.lock();
        driver.done().then(|| driver.snapshot())
    };
    rig.mic.stop();
    let _ = mic_handle.join();
    if let Some(scripter) = scripter.as_mut() {
        scripter.stop();
    }
    rig.app.shutdown();
    if let Some(error) = failure.lock().clone() {
        return Err(error);
    }
    let Some(snapshot) = running else {
        return Err(format!(
            "escenario sin terminar en {:.0} s",
            scenario.timeout
        ));
    };
    let requests = count_lines(&home.join("requests.jsonl"));
    let audit = rig
        .audit
        .clone()
        .map_or_else(Vec::new, |path| read_lines(&path));
    Ok(finish(&trace, &snapshot, &audit, requests, scenario, home))
}

/// Arranca la captura y corre el guion dentro del bucle de la interfaz.
fn drive(
    rig: &Rig,
    driver: &Arc<Mutex<Driver>>,
    failure: &Arc<Mutex<Option<String>>>,
    scenario: &Scenario,
) -> Result<std::thread::JoinHandle<()>, String> {
    let mic_handle = rig
        .mic
        .spawn(rig.app.clone())
        .map_err(|error| format!("no pude arrancar ezagent-call: {error}"))?;
    let ui = rig.app.inner.ui.ui.clone();
    let deadline = Instant::now() + Duration::from_secs_f64(scenario.timeout.max(0.0));
    arm(
        &ui,
        driver,
        failure,
        scenario.timeout,
        deadline,
        Arc::new(handlers()),
    );
    rig.app.start();
    ui.run()?;
    Ok(mic_handle)
}

/// Guion `agent.scenes` del escenario, si lo declara y no es de enjambre.
fn start_scripter(
    scenario: &Scenario,
    home: &Path,
    trace: &Arc<Trace>,
) -> Option<super::scripter::AgentScripter> {
    let spec = scenario.data.get("agent")?;
    if !ez_core::config::truthy(spec)
        || scenario
            .data
            .get("swarm")
            .is_some_and(ez_core::config::truthy)
    {
        return None;
    }
    let mut scripter =
        super::scripter::AgentScripter::new(&home.join("hc.json"), spec, Arc::clone(trace));
    scripter.start();
    Some(scripter)
}

/// Servicios ya construidos: chat, HUD sin ventana y envoltorio del runtime.
fn build_rig(
    scenario: &Scenario,
    paths: &Paths,
    home: &Path,
    config_path: &Path,
    settings: &serde_json::Map<String, Value>,
) -> Result<Rig, String> {
    let cfg = VoiceConfig::load(Some(config_path))?;
    let trace = Trace::new(home);
    let chat_path = home.join("EzAgent").join("chat.jsonl");
    let recorder = Recorder::new(Arc::clone(&trace), Some(&chat_path));
    let speaker = ReplaySpeaker::new(Arc::clone(&trace), number(scenario, "speech_secs_min", 1.8));
    let mic = ReplayMic::new(
        Arc::clone(&trace),
        paths.audio.clone(),
        vad_settings(scenario),
    );
    let mut parts = AppParts::new(
        Arc::clone(&speaker) as Arc<dyn crate::app::core::SpeakerApi>,
        Arc::new(Mutex::new(MicCapture::new(None, VadSettings::default()))),
    );
    parts.config_file = Some(config_path.to_path_buf());
    parts.chat = Some(Arc::new(ChatModel::new(
        Some(Arc::new(ChatStore::new(None))),
        cfg.get("chat_voice").is_some_and(ez_core::config::truthy),
    )));
    parts.whisper_ready = ez_stt::server::whisper_ready(&cfg_text(&cfg, "whisper_url", ""));
    parts.build_swarm = false;
    parts.build_agent = true;
    // DECISION: la app corre en modo real (vigilante y mensajero) pero sin
    // abrir la captura: el arnés alimenta el micrófono desde su hilo.
    parts.background = true;
    parts.mic_capture = false;
    parts.warm_stt = false;
    parts.tool_acks = settings.get("ack_tools") != Some(&Value::Bool(false));
    parts.hud_factory = Some(Arc::new(NoWindows));
    parts.trace = Some(Arc::clone(&recorder) as Arc<dyn AppTrace>);
    let wrap_recorder = Arc::clone(&recorder);
    parts.runtime_wrap = Some(Arc::new(move |core| {
        Arc::new(super::traced_runtime::TracedRuntime::new(
            core,
            Arc::clone(&wrap_recorder),
        )) as Arc<dyn AgentRuntimeApi>
    }));
    let app = VoiceApp::with_services(cfg, Some(home.join("tasks.json")), parts);
    if settings.get("python_available") == Some(&Value::Bool(false)) {
        use ez_agent::tools_skills::{build_skill_tools_with_hooks, SkillHooks};
        let agent = app.inner.agents.agent.lock();
        if let Some(runtime) = agent.as_ref() {
            let config = runtime.config();
            let skills = runtime.skills();
            let hooks = SkillHooks {
                interpreter: Some(None),
                ..SkillHooks::default()
            };
            runtime.add_tool_group(std::sync::Arc::new(
                move |_: &ez_agent::runtime::AgentRuntime| {
                    build_skill_tools_with_hooks(&config, std::sync::Arc::clone(&skills), &hooks)
                },
            ));
        }
    }
    app.set_hooks(Arc::new(super::hooks::ReplayHooks::new(
        &app,
        Arc::clone(&recorder),
    )));
    let audit = app
        .inner
        .agents
        .agent
        .lock()
        .as_ref()
        .map(|agent| agent.config().audit_path);
    Ok(Rig {
        app,
        mic,
        speaker,
        trace,
        audit,
    })
}

/// Cierre de la traza (`finish_trace` del Python).
fn finish(
    trace: &Trace,
    snapshot: &Snapshot,
    audit: &[String],
    requests: usize,
    scenario: &Scenario,
    home: &Path,
) -> RunOutput {
    trace.record(
        "app.end",
        json!({
            "state": snapshot.state,
            "in_call": snapshot.in_call,
            "muted": snapshot.muted,
        }),
    );
    for (role, text) in &snapshot.messages {
        trace.record("chat.snapshot", json!({"role": role, "text": text}));
    }
    record_audit(trace, audit);
    trace.record("llm.requests", json!({"count": requests}));
    let context = Context {
        hc_path: Some(home.join("hc.json")),
        requests_path: Some(home.join("requests.jsonl")),
    };
    let expect = scenario
        .data
        .get("expect")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let errors = check(&expect, snapshot, trace, requests, &context);
    RunOutput {
        lines: trace.lines(),
        errors,
    }
}

struct RunOutput {
    lines: Vec<String>,
    errors: Vec<String>,
}

/// Programa la vuelta del guion cada 5 ms (el `mainloop` de `FakeRoot`).
fn arm(
    ui: &UiLoop,
    driver: &Arc<Mutex<Driver>>,
    failure: &Arc<Mutex<Option<String>>>,
    timeout: f64,
    deadline: Instant,
    handlers: Arc<Vec<Box<dyn Step>>>,
) {
    let ui_next = ui.clone();
    let driver_next = Arc::clone(driver);
    let failure_next = Arc::clone(failure);
    ui.schedule(Duration::from_millis(5), move || {
        if Instant::now() > deadline && !driver_next.lock().done() {
            *failure_next.lock() = Some(format!("escenario sin terminar en {timeout:.0} s"));
            ui_next.quit();
            return;
        }
        let outcome = driver_next.lock().tick(&handlers);
        match outcome {
            Err(error) => {
                *failure_next.lock() = Some(error);
                ui_next.quit();
            }
            Ok(()) if driver_next.lock().done() => ui_next.quit(),
            Ok(()) => arm(
                &ui_next,
                &driver_next,
                &failure_next,
                timeout,
                deadline,
                Arc::clone(&handlers),
            ),
        }
    });
}

fn vad_settings(scenario: &Scenario) -> VadSettings {
    VadSettings {
        min_rms: number(scenario, "min_rms", 0.0006),
        silence_secs: number(scenario, "silence_secs", 1.3),
        pre_roll_secs: number(scenario, "pre_roll_secs", 0.6),
        min_speech_secs: number(scenario, "min_speech_secs", 0.2),
        max_utterance_secs: number(scenario, "max_utterance_secs", 30.0),
        noise_multiplier: number(scenario, "noise_multiplier", 1.3),
        turn_quick_secs: 0.35,
        barge_in_rms: number(scenario, "barge_in_rms", 0.008),
        barge_in_secs: number(scenario, "barge_in_secs", 0.25),
    }
}

fn number(scenario: &Scenario, key: &str, default: f64) -> f64 {
    scenario
        .data
        .get("settings")
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(default)
}

fn cfg_text(cfg: &VoiceConfig, key: &str, default: &str) -> String {
    cfg.get(key)
        .and_then(Value::as_str)
        .map_or_else(|| default.to_string(), str::to_string)
}

fn new_home() -> Result<PathBuf, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    Ok(std::env::temp_dir().join(format!("ezreplay-{}-{stamp}", std::process::id())))
}

fn slash(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn default_trace(stem: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ezagent-replay-{stem}.trace.jsonl"))
}

fn count_lines(path: &Path) -> usize {
    read_text(path).map_or(0, |text| py_splitlines(&text).len())
}

fn read_lines(path: &Path) -> Vec<String> {
    read_text(path).map_or_else(Vec::new, |text| {
        py_splitlines(&text)
            .into_iter()
            .map(str::to_string)
            .collect()
    })
}

fn read_text(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn record_audit(trace: &Trace, lines: &[String]) {
    for raw in lines {
        if raw.trim().is_empty() {
            continue;
        }
        let Ok(record) = loads(raw) else {
            continue;
        };
        trace.record(
            "audit.line",
            json!({
                "tool": record.get("tool").cloned().unwrap_or(Value::Null),
                "level": record.get("level").cloned().unwrap_or(Value::Null),
                "decision": record.get("decision").cloned().unwrap_or(Value::Null),
                "ok": record.get("result_ok").cloned().unwrap_or(Value::Null),
                "taint": record.get("taint").cloned().unwrap_or(Value::Null),
                "args": record.get("args").cloned().unwrap_or(Value::Null),
            }),
        );
    }
}

/// Guarda y restaura las variables que toca la corrida.
struct EnvGuard {
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    fn freeze(home: &Path) -> Self {
        let saved = MANAGED_ENV
            .iter()
            .map(|name| (*name, std::env::var_os(name)))
            .collect::<Vec<_>>();
        for name in [
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "TMP",
            "TEMP",
        ] {
            std::env::set_var(name, home);
        }
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

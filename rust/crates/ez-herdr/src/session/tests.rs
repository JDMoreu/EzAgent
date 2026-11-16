mod extra;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use ez_core::config::VoiceConfig;
use serde_json::{json, Map, Value};

use super::*;
use crate::herdr::{AgentLaunch, HerdrError};

struct FakeClock {
    now: Mutex<f64>,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            now: Mutex::new(0.0),
        }
    }

    fn elapsed(&self) -> f64 {
        *self.now.lock().expect("reloj")
    }
}

impl SessionClock for FakeClock {
    fn now(&self) -> f64 {
        self.elapsed()
    }

    fn sleep(&self, seconds: f64) {
        *self.now.lock().expect("reloj") += seconds;
    }
}

#[derive(Default)]
struct HostState {
    info: Map<String, Value>,
    screens: VecDeque<String>,
    screen: String,
    screen_error: bool,
    send_ok: bool,
    keys_ok: bool,
    state_after_enter: Option<String>,
    sent: Vec<(String, String, bool)>,
    keys: Vec<Vec<String>>,
    gets: usize,
    slow_get: f64,
}

struct FakeHost {
    state: Mutex<HostState>,
    clock: Option<Arc<FakeClock>>,
}

impl FakeHost {
    fn new(state: HostState) -> Self {
        Self {
            state: Mutex::new(state),
            clock: None,
        }
    }

    fn with_clock(mut self, clock: Arc<FakeClock>) -> Self {
        self.clock = Some(clock);
        self
    }

    fn state(&self) -> std::sync::MutexGuard<'_, HostState> {
        self.state.lock().expect("host")
    }
}

impl SessionHost for FakeHost {
    fn get_agent(&self, _address: &str) -> Result<Map<String, Value>, HerdrError> {
        let (info, slow) = {
            let mut state = self.state();
            state.gets += 1;
            (state.info.clone(), state.slow_get)
        };
        if slow > 0.0 {
            if let Some(clock) = &self.clock {
                *clock.now.lock().expect("reloj") += slow;
            }
        }
        Ok(info)
    }

    fn send_agent(
        &self,
        address: &str,
        text: &str,
        confirmed: bool,
    ) -> Result<Map<String, Value>, HerdrError> {
        let mut state = self.state();
        state
            .sent
            .push((address.to_string(), text.to_string(), confirmed));
        let mut row = Map::new();
        row.insert("ok".to_string(), Value::Bool(state.send_ok));
        Ok(row)
    }

    fn keys_agent(
        &self,
        _address: &str,
        keys: &[String],
    ) -> Result<Map<String, Value>, HerdrError> {
        let mut state = self.state();
        state.keys.push(keys.to_vec());
        if keys.iter().any(|key| key == "enter") {
            if let Some(value) = state.state_after_enter.clone() {
                state.info.insert("state".to_string(), Value::String(value));
            }
        }
        let mut row = Map::new();
        row.insert("ok".to_string(), Value::Bool(state.keys_ok));
        Ok(row)
    }

    fn read_agent(&self, _address: &str, _lines: Option<i64>) -> Result<String, HerdrError> {
        let mut state = self.state();
        if state.screen_error {
            return Err(HerdrError::failed("sin pantalla"));
        }
        if let Some(next) = state.screens.pop_front() {
            return Ok(next);
        }
        Ok(state.screen.clone())
    }
}

fn agent_session(state: HostState, cfg: VoiceConfig) -> AgentSession<FakeHost> {
    AgentSession::with_clock(
        FakeHost::new(state),
        cfg,
        Arc::new(FakeClock::new()) as Arc<dyn SessionClock>,
    )
}

fn config_custom(profile: Value) -> VoiceConfig {
    let data = json!({
        "agents": {"custom": ["custom"]},
        "agent_profiles": {"custom": profile},
    })
    .as_object()
    .cloned()
    .expect("objeto");
    VoiceConfig::from_json(&data)
}

fn idle_info(agent: &str) -> Map<String, Value> {
    json!({"agent": agent, "state": "idle"})
        .as_object()
        .cloned()
        .expect("objeto")
}

#[test]
fn command_line_antepone_entorno_y_cita() {
    let argv = vec![
        "opencode".to_string(),
        "--flag".to_string(),
        "a b".to_string(),
    ];
    let mut env = Map::new();
    env.insert("X".to_string(), Value::String("o'k".to_string()));
    let line = command_line(&argv, &env, Some(&["C:\\tools".to_string()]));
    assert_eq!(
        line,
        "$env:X='o''k'; $env:PATH='C:\\tools;'+$env:PATH; opencode --flag 'a b'"
    );
}

#[test]
fn valid_keys_normaliza_y_rechaza() {
    let couple = vec![" Enter ".to_string(), "CTRL+T".to_string()];
    assert_eq!(
        valid_keys(&couple),
        Some(vec!["enter".to_string(), "ctrl+t".to_string()])
    );
    assert_eq!(valid_keys(&[]), None);
    assert_eq!(valid_keys(&["nope".to_string()]), None);
    assert_eq!(valid_keys(&["f13".to_string()]), None);
    let too_many: Vec<String> = vec!["enter".to_string(); 13];
    assert_eq!(valid_keys(&too_many), None);
}

#[test]
fn fill_sustituye_modelo_esfuerzo_y_busqueda() {
    let values = FillValues::new("opencode-go/deepseek-v4.1-flash", "high");
    assert_eq!(
        fill("/model {model} {effort}", &values, false),
        "/model opencode-go/deepseek-v4.1-flash high"
    );
    assert_eq!(
        fill("{model_search}", &values, false),
        "deepseek v4.1 flash"
    );
    assert_eq!(
        fill("{model}", &values, true),
        "opencode\\-go/deepseek\\-v4\\.1\\-flash"
    );
}

#[test]
fn wait_ready_detecta_agente() {
    let mut state = HostState {
        info: idle_info("opencode"),
        ..HostState::default()
    };
    state.screen = "agente listo".to_string();
    let session = agent_session(state, VoiceConfig::new());
    assert!(session.wait_ready("t1", 1.0));
    assert_eq!(session.host().state().screen, "agente listo");
}

#[test]
fn wait_ready_detecta_programa_tras_la_shell() {
    let mut state = HostState {
        info: json!({"app": "powershell.exe", "state": "idle"})
            .as_object()
            .cloned()
            .expect("objeto"),
        ..HostState::default()
    };
    state.screen = "PS> opencode\nWelcome\nSelect model".to_string();
    let session = agent_session(state, VoiceConfig::new());
    assert!(session.wait_ready("t1", 1.0));
}

#[test]
fn wait_ready_no_confunde_la_shell_con_un_agente() {
    let mut state = HostState {
        info: json!({"app": "powershell.exe", "state": "idle"})
            .as_object()
            .cloned()
            .expect("objeto"),
        ..HostState::default()
    };
    state.screen = "linea uno\nlinea dos\nPS C:\\>".to_string();
    let session = agent_session(state, VoiceConfig::new());
    assert!(!session.wait_ready("t1", 1.0));
}

#[test]
fn wait_shell_prompt_espera_al_prompt() {
    let mut state = HostState::default();
    state.screens.push_back(String::new());
    state.screen = "PS C:\\work>".to_string();
    let session = agent_session(state, VoiceConfig::new());
    assert!(session.wait_shell_prompt("t1"));
    assert!(session.host().state().screens.is_empty());
}

#[test]
fn wait_shell_prompt_se_rinde_sin_prompt() {
    let state = HostState::default();
    let session = agent_session(state, VoiceConfig::new());
    assert!(!session.wait_shell_prompt("t1"));
}

#[test]
fn blocking_dialog_reconoce_confirmaciones() {
    let state = HostState {
        screen: "Do you trust the files in this folder?".to_string(),
        ..HostState::default()
    };
    let session = agent_session(state, VoiceConfig::new());
    assert!(session.blocking_dialog("t1"));

    let state = HostState {
        screen: "Working on it…".to_string(),
        ..HostState::default()
    };
    let session = agent_session(state, VoiceConfig::new());
    assert!(!session.blocking_dialog("t1"));
}

#[test]
fn confirm_submitted_pulsa_enter_una_vez_y_confirma() {
    let mut state = HostState {
        info: idle_info("claude"),
        send_ok: true,
        keys_ok: true,
        state_after_enter: Some("working".to_string()),
        ..HostState::default()
    };
    state.screen = "› arregla el bug del parser".to_string();
    let session = agent_session(state, VoiceConfig::new());
    assert!(session.confirm_submitted("t1", "arregla el bug del parser"));
    let keys = session.host().state().keys.clone();
    assert_eq!(keys, vec![vec!["enter".to_string()]]);
}

#[test]
fn confirm_submitted_nunca_pulsa_dos_veces() {
    let mut state = HostState {
        info: idle_info("claude"),
        keys_ok: true,
        ..HostState::default()
    };
    state.screen = "› arregla el bug del parser".to_string();
    let session = agent_session(state, VoiceConfig::new());
    assert!(!session.confirm_submitted("t1", "arregla el bug del parser"));
    assert_eq!(session.host().state().keys.len(), 1);
}

#[test]
fn confirm_submitted_sin_pantalla_no_confirma() {
    let mut state = HostState {
        info: idle_info("claude"),
        screen_error: true,
        ..HostState::default()
    };
    state.screen = "lo que sea".to_string();
    let session = agent_session(state, VoiceConfig::new());
    assert!(!session.confirm_submitted("t1", "arregla el bug del parser"));
    assert!(session.host().state().keys.is_empty());
}

#[test]
fn confirm_submitted_sin_rastro_en_la_caja_confirma() {
    let mut state = HostState {
        info: idle_info("claude"),
        ..HostState::default()
    };
    state.screen = "› otra cosa distinta".to_string();
    let session = agent_session(state, VoiceConfig::new());
    assert!(session.confirm_submitted("t1", "arregla el bug del parser"));
    assert!(session.host().state().keys.is_empty());
}

#[test]
fn send_task_avisa_sin_evidencia() {
    let mut state = HostState {
        info: idle_info("claude"),
        send_ok: true,
        screen_error: true,
        ..HostState::default()
    };
    state.screen = "lo que sea".to_string();
    let session = agent_session(state, VoiceConfig::new());
    let row = session.send_task("t1", "haz algo");
    assert_eq!(row["ok"], true);
    assert_eq!(
        row["warning"],
        "No pude confirmar que el agente recibió la tarea."
    );
    assert_eq!(session.host().state().sent.len(), 1);
    assert!(session.host().state().sent[0].2);
}

#[test]
fn finish_launch_falla_si_no_arranca() {
    let mut state = HostState {
        info: json!({"app": "powershell.exe", "state": "idle"})
            .as_object()
            .cloned()
            .expect("objeto"),
        ..HostState::default()
    };
    state.screen = "PS C:\\>".to_string();
    let session = agent_session(state, VoiceConfig::new());
    let launch = AgentLaunch::default();
    let row = session.finish_launch("t9", "claude", &launch, "");
    assert_eq!(row["ok"], false);
    assert_eq!(
        row["error"],
        "Abrí la terminal t9, pero no vi que claude arrancara; no le envié nada. \
         Revísala con action=read."
    );
}

#[test]
fn finish_launch_respeta_el_dialogo_de_confianza() {
    let mut state = HostState {
        info: idle_info("claude"),
        ..HostState::default()
    };
    state.screen = "Do you trust the files in this folder?".to_string();
    let session = agent_session(state, VoiceConfig::new());
    let row = session.finish_launch("t1", "claude", &AgentLaunch::default(), "");
    assert_eq!(row["ok"], false);
    assert_eq!(
        row["error"],
        "claude está pidiendo confirmar algo en la terminal t1 (por ejemplo, \
         confiar en la carpeta). No pulso Enter por ti: confírmalo tú y vuelve a \
         pedirme la tarea."
    );
}

#[test]
fn finish_launch_fija_modelo_con_comando_de_sesion() {
    let cfg = config_custom(json!({
        "session_commands": {"model": [{"send": "/model {model}", "soft": true}]},
    }));
    let mut state = HostState {
        info: idle_info("custom"),
        send_ok: true,
        ..HostState::default()
    };
    state.screen = "Custom funcionando".to_string();
    let session = agent_session(state, cfg);
    let mut pending = Map::new();
    pending.insert("model".to_string(), Value::String("sonnet".to_string()));
    let launch = AgentLaunch {
        pending,
        warnings: vec!["aviso previo".to_string()],
        ..AgentLaunch::default()
    };
    let row = session.finish_launch("t1", "custom", &launch, "");
    assert_eq!(row["ok"], true);
    assert_eq!(row["task_sent"], false);
    assert_eq!(row["applied"], json!(["/model sonnet"]));
    assert_eq!(row["warnings"], json!(["aviso previo"]));
}

#[test]
fn finish_launch_aborta_y_pulsa_esc_si_no_confirma() {
    let cfg = config_custom(json!({
        "session_commands": {"model": [{"send": "/model {model}", "expect": "Switched model to"}]},
    }));
    let mut state = HostState {
        info: idle_info("custom"),
        send_ok: true,
        keys_ok: true,
        ..HostState::default()
    };
    state.screen = "nada que ver".to_string();
    let session = agent_session(state, cfg);
    let mut pending = Map::new();
    pending.insert("model".to_string(), Value::String("sonnet".to_string()));
    let launch = AgentLaunch {
        pending,
        ..AgentLaunch::default()
    };
    let row = session.finish_launch("t1", "custom", &launch, "");
    assert_eq!(row["ok"], false);
    assert_eq!(row["applied"], json!(["/model sonnet"]));
    assert_eq!(
        row["error"],
        "No pude fijar el modelo: la pantalla no mostró lo esperado. Dejé custom \
         abierto en t1 sin enviar la tarea; revísalo con action=read."
    );
    let keys = session.host().state().keys.clone();
    assert_eq!(keys, vec![vec!["esc".to_string()]]);
}

#[test]
fn finish_launch_paso_opcional_solo_avisa() {
    let cfg = config_custom(json!({
        "session_commands": {
            "effort": [{"send": "/effort {effort}", "expect": "Select variant", "optional": true}]
        },
    }));
    let mut state = HostState {
        info: idle_info("custom"),
        send_ok: true,
        keys_ok: true,
        ..HostState::default()
    };
    state.screen = "sin selectores".to_string();
    let session = agent_session(state, cfg);
    let mut pending = Map::new();
    pending.insert("effort".to_string(), Value::String("high".to_string()));
    let launch = AgentLaunch {
        pending,
        ..AgentLaunch::default()
    };
    let row = session.finish_launch("t1", "custom", &launch, "");
    assert_eq!(row["ok"], true);
    assert_eq!(row["applied"], json!(["/effort high"]));
    assert_eq!(
        row["warnings"],
        json!(["No se fijó el esfuerzo: la pantalla no mostró lo esperado."])
    );
}

#[test]
fn configure_agent_valida_entrada() {
    let session = agent_session(HostState::default(), VoiceConfig::new());
    assert_eq!(
        session.configure_agent("t1", None, None)["error"],
        "Indica el modelo o el esfuerzo."
    );

    let mut state = HostState {
        info: idle_info("nada-conocido"),
        ..HostState::default()
    };
    state.screen = String::new();
    let session = agent_session(state, VoiceConfig::new());
    assert_eq!(
        session.configure_agent("t1", Some("sonnet"), None)["error"],
        "No sé qué agente es t1 (nada-conocido); no puedo cambiarle el modelo."
    );

    let mut state = HostState {
        info: idle_info("claude"),
        ..HostState::default()
    };
    state.screen = String::new();
    let session = agent_session(state, VoiceConfig::new());
    assert_eq!(
        session.configure_agent("t1", Some("-x"), None)["error"],
        "El modelo '-x' no es válido."
    );
    assert_eq!(
        session.configure_agent("t1", None, Some("turbo"))["error"],
        "El esfuerzo 'turbo' no existe en claude. Valores válidos: low, medium, high, xhigh, max."
    );
}

#[test]
fn configure_agent_aplica_el_comando() {
    let mut state = HostState {
        info: idle_info("claude"),
        send_ok: true,
        ..HostState::default()
    };
    state.screen = String::new();
    let session = agent_session(state, VoiceConfig::new());
    let row = session.configure_agent("t1", Some("sonnet"), None);
    assert_eq!(row["ok"], true);
    assert_eq!(row["agent"], "claude");
    assert_eq!(row["applied"], json!(["/model sonnet"]));
    assert_eq!(row["warnings"], json!([]));
    assert_eq!(session.host().state().sent[0].1, "/model sonnet");
}

#[test]
fn configure_agent_avisa_si_no_hay_pasos() {
    let cfg = config_custom(json!({"label": "Custom"}));
    let mut state = HostState {
        info: idle_info("custom"),
        ..HostState::default()
    };
    state.screen = String::new();
    let session = agent_session(state, cfg);
    let row = session.configure_agent("t1", Some("sonnet"), None);
    assert_eq!(row["ok"], false);
    assert_eq!(
        row["error"],
        "custom no permite cambiar el modelo dentro de la sesión; hay que \
         lanzarlo de nuevo con run."
    );
}

#[test]
fn configure_agent_cicla_teclas_hasta_ver_el_nivel() {
    let cfg = config_custom(json!({
        "session_commands": {
            "effort": [{
                "cycle": {
                    "keys": ["ctrl+t"],
                    "until": "Switched variant to {effort}",
                    "max": 2,
                },
                "message": "no está {effort}",
            }],
        },
    }));
    let mut state = HostState {
        info: idle_info("custom"),
        keys_ok: true,
        ..HostState::default()
    };
    state.screens.push_back("todavía nada".to_string());
    state.screen = "Switched variant to high".to_string();
    let session = agent_session(state, cfg);
    let row = session.configure_agent("t1", None, Some("high"));
    assert_eq!(row["ok"], true);
    assert_eq!(row["applied"], json!(["ctrl+t"]));
    assert_eq!(session.host().state().keys.len(), 2);
}

#[test]
fn session_steps_normaliza_textos_y_dicts() {
    let cfg = config_custom(json!({
        "session_commands": {
            "model": ["  ", "/model {model}", {"send": "x"}, {"foo": 1}],
        },
    }));
    let steps = session_steps(&cfg, "custom", "model");
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0]["send"], "/model {model}");
    assert_eq!(steps[1]["send"], "x");
}

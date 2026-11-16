use std::sync::{Arc, Mutex};

use ez_herdr::herdr::{NameCandidate, NameKind, RunAgent};
use serde_json::{json, Map, Value};

use super::*;
use crate::tools_herdr::args::{shared_state, NameConfirm};
use crate::tools_herdr::port::{HerdrPort, PortError};

fn args(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("objeto")
}

/// Puerto sin Herdr: solo los nombres vivos que se le pidan.
#[derive(Default)]
struct NamesPort {
    terminals: Vec<NameCandidate>,
}

impl NamesPort {
    fn with_terminals(pairs: &[(&str, &str)]) -> Self {
        Self {
            terminals: pairs
                .iter()
                .map(|(value, label)| NameCandidate::new(*value, *label))
                .collect(),
        }
    }
}

impl HerdrPort for NamesPort {
    fn resolve_agent(&self, _name: &str) -> Option<String> {
        None
    }

    fn label_for(&self, _address: &str) -> String {
        String::new()
    }

    fn name_candidates(&self, kind: NameKind, _fresh: bool) -> Vec<NameCandidate> {
        if kind == NameKind::Terminal {
            self.terminals.clone()
        } else {
            Vec::new()
        }
    }

    fn list_agents(&self) -> Result<Vec<Map<String, Value>>, PortError> {
        Ok(Vec::new())
    }

    fn get_agent(&self, _address: &str) -> Result<Map<String, Value>, PortError> {
        Ok(Map::new())
    }

    fn read_agent(&self, _address: &str, _lines: Option<i64>) -> Result<String, PortError> {
        Ok(String::new())
    }

    fn read_full(&self, _address: &str) -> Result<String, PortError> {
        Ok(String::new())
    }

    fn send_agent(
        &self,
        _address: &str,
        _text: &str,
        _confirmed: bool,
    ) -> Result<Map<String, Value>, PortError> {
        Ok(Map::new())
    }

    fn run_agent(&self, _params: RunAgent<'_>) -> Result<Map<String, Value>, PortError> {
        Ok(Map::new())
    }

    fn open_terminal(
        &self,
        _name: &str,
        _cwd: Option<&str>,
        _shell: &str,
        _space: Option<&str>,
    ) -> Result<Map<String, Value>, PortError> {
        Ok(Map::new())
    }

    fn agents_catalog(&self) -> Vec<Map<String, Value>> {
        Vec::new()
    }

    fn configure_agent(
        &self,
        _address: &str,
        _model: Option<&str>,
        _effort: Option<&str>,
    ) -> Map<String, Value> {
        Map::new()
    }

    fn keys_agent(
        &self,
        _address: &str,
        _keys: &[String],
    ) -> Result<Map<String, Value>, PortError> {
        Ok(Map::new())
    }

    fn focus_agent(&self, _address: &str) -> Result<Map<String, Value>, PortError> {
        Ok(Map::new())
    }

    fn kill_agent(
        &self,
        _address: &str,
        _confirmed: bool,
    ) -> Result<Map<String, Value>, PortError> {
        Ok(Map::new())
    }

    fn save_context(&self, _text: &str) -> Option<String> {
        None
    }
}

/// Sin nombres vivos: el texto se usa tal cual (lo que veían las pruebas).
fn voice(callbacks: &HerdrCallbacks, state: &SharedState, args: &Map<String, Value>) -> ToolResult {
    handle_voice(&NamesPort::default(), callbacks, state, args)
}

#[derive(Default)]
struct Recorded {
    calls: Mutex<Vec<String>>,
}

impl Recorded {
    fn push(&self, call: String) {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(call);
    }

    fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

fn callbacks(recorded: &Arc<Recorded>, user_text: &'static str) -> HerdrCallbacks {
    let watch = Arc::clone(recorded);
    let close = Arc::clone(recorded);
    let mute = Arc::clone(recorded);
    let mic = Arc::clone(recorded);
    let remember = Arc::clone(recorded);
    HerdrCallbacks {
        watch: Arc::new(move |address| watch.push(format!("watch {address}"))),
        request_close: Arc::new(move || close.push("close".to_string())),
        mic_mute: Arc::new(move |muted| mic.push(format!("mic {muted}"))),
        last_summary: Arc::new(|| "resumen previo".to_string()),
        mute: Arc::new(move |minutes| mute.push(format!("mute {minutes}"))),
        get_last_user_text: Arc::new(move || user_text.to_string()),
        remember_task: Arc::new(move |address, agent, task| {
            remember.push(format!("remember {address} {agent} {task}"))
        }),
        ..HerdrCallbacks::default()
    }
}

#[test]
fn close_sin_aviso_solo_cierra() {
    let recorded = Arc::new(Recorded::default());
    let state = shared_state();
    let result = voice(
        &callbacks(&recorded, "hola"),
        &state,
        &args(json!({"action": "close"})),
    );
    assert!(result.ok);
    assert_eq!(result.data["closed"], json!(true));
    assert_eq!(result.data["watching"], Value::Null);
    assert_eq!(
        result.data["message"],
        json!("Cerraré la escucha al terminar de hablar.")
    );
    assert_eq!(recorded.calls(), vec!["close".to_string()]);
}

#[test]
fn close_con_aviso_vigila_el_ultimo_agente() {
    let recorded = Arc::new(Recorded::default());
    let state = shared_state();
    set_last_address(&state, Some("t1".to_string()));
    let result = voice(
        &callbacks(&recorded, "avísame cuando termine"),
        &state,
        &args(json!({"action": "close"})),
    );
    assert_eq!(result.data["watching"], json!("t1"));
    assert_eq!(
        recorded.calls(),
        vec![
            "remember t1  avísame cuando termine".to_string(),
            "watch t1".to_string(),
            "close".to_string(),
        ]
    );
}

#[test]
fn close_and_watch_exige_direccion_o_la_ultima() {
    let recorded = Arc::new(Recorded::default());
    let state = shared_state();
    let result = voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "close_and_watch"})),
    );
    assert!(!result.ok);
    assert_eq!(
        result.error.as_deref(),
        Some("No hay ningún agente al que vigilar. Indica la dirección del agente con «address».")
    );
    let result = voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "close_and_watch", "address": " t2 "})),
    );
    assert!(result.ok);
    assert_eq!(result.data["address"], json!("t2"));
    assert_eq!(result.data["watching"], json!(true));
    assert_eq!(last_address(&state), Some("t2".to_string()));
    assert!(recorded.calls().contains(&"watch t2".to_string()));
}

#[test]
fn mic_mute_cambia_el_estado() {
    let recorded = Arc::new(Recorded::default());
    let state = shared_state();
    let missing = voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mic_mute"})),
    );
    assert_eq!(
        missing.error.as_deref(),
        Some("Para la acción «mic_mute» falta el parámetro booleano «muted».")
    );
    let muted = voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mic_mute", "muted": true})),
    );
    assert_eq!(muted.data["muted"], json!(true));
    assert_eq!(muted.data["message"], json!("Micrófono silenciado."));
    let active = voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mic_mute", "muted": false})),
    );
    assert_eq!(active.data["message"], json!("Micrófono activo."));
    assert_eq!(
        recorded.calls(),
        vec!["mic true".to_string(), "mic false".to_string()]
    );
}

#[test]
fn repeat_last_devuelve_el_ultimo_resumen() {
    let recorded = Arc::new(Recorded::default());
    let result = voice(
        &callbacks(&recorded, ""),
        &shared_state(),
        &args(json!({"action": "repeat_last"})),
    );
    assert_eq!(result.data["text"], json!("resumen previo"));
}

#[test]
fn mute_valida_minutos() {
    let recorded = Arc::new(Recorded::default());
    let state = shared_state();
    let missing = voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mute"})),
    );
    assert_eq!(
        missing.error.as_deref(),
        Some("Para la acción «mute» falta el parámetro entero «minutes».")
    );
    let out_of_range = voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mute", "minutes": 0})),
    );
    assert_eq!(
        out_of_range.error.as_deref(),
        Some("El parámetro «minutes» debe estar entre 1 y 480.")
    );
    let huge = voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mute", "minutes": 18446744073709551615u64})),
    );
    assert_eq!(
        huge.error.as_deref(),
        Some("El parámetro «minutes» debe estar entre 1 y 480.")
    );
    let muted = voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mute", "minutes": 5})),
    );
    assert_eq!(muted.data["minutes"], json!(5));
    assert_eq!(muted.data["message"], json!("Silenciado 5 minutos."));
    assert_eq!(recorded.calls(), vec!["mute 5".to_string()]);
}

#[test]
fn accion_desconocida_de_voz() {
    let result = voice(
        &HerdrCallbacks::default(),
        &shared_state(),
        &args(json!({"action": "zzz"})),
    );
    assert_eq!(
        result.error.as_deref(),
        Some(
            "Acción desconocida para «voice_control»: 'zzz'. Acciones válidas: close, \
             close_and_watch, mic_mute, repeat_last, mute."
        )
    );
}

#[test]
fn close_and_watch_resuelve_un_nombre_dudoso_tras_confirmar() {
    let recorded = Arc::new(Recorded::default());
    let state = shared_state();
    let port = NamesPort::with_terminals(&[("t3", "TEST")]);
    let mut callbacks = callbacks(&recorded, "");
    let confirm: NameConfirm = Arc::new(|question: &str| {
        assert_eq!(question, "¿Te refieres a la terminal «TEST»?");
        true
    });
    callbacks.confirm_name = Some(confirm);
    let result = handle_voice(
        &port,
        &callbacks,
        &state,
        &args(json!({"action": "close_and_watch", "address": "TES"})),
    );
    assert!(result.ok, "{:?}", result.error);
    assert_eq!(result.data["address"], json!("t3"));
    assert_eq!(result.data["name_note"], json!("usé la terminal «TEST»"));
    assert_eq!(last_address(&state), Some("t3".to_string()));
    assert!(recorded.calls().contains(&"watch t3".to_string()));
}

#[test]
fn close_and_watch_sin_respuesta_ni_parecido_lista_las_terminales() {
    let recorded = Arc::new(Recorded::default());
    let port = NamesPort::with_terminals(&[("t3", "TEST")]);
    let mut callbacks = callbacks(&recorded, "");
    let asked = Arc::new(Mutex::new(false));
    let seen = Arc::clone(&asked);
    let confirm: NameConfirm = Arc::new(move |_| {
        *seen.lock().expect("preguntado") = true;
        false
    });
    callbacks.confirm_name = Some(confirm);
    let result = handle_voice(
        &port,
        &callbacks,
        &shared_state(),
        &args(json!({"action": "close_and_watch", "address": "TES"})),
    );
    assert!(*asked.lock().expect("preguntado"));
    assert_eq!(
        result.error.as_deref(),
        Some("No encontré la terminal «TES». Terminales abiertas: «TEST».")
    );
    assert!(recorded.calls().is_empty());
}

#[test]
fn close_and_watch_usa_la_ultima_direccion_sin_resolverla() {
    let recorded = Arc::new(Recorded::default());
    let state = shared_state();
    set_last_address(&state, Some("t1".to_string()));
    let port = NamesPort::with_terminals(&[("t3", "TEST")]);
    let result = handle_voice(
        &port,
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "close_and_watch"})),
    );
    assert!(result.ok);
    assert_eq!(result.data["address"], json!("t1"));
    assert!(result.data.get("name_note").is_none());
    assert!(recorded.calls().contains(&"watch t1".to_string()));
}

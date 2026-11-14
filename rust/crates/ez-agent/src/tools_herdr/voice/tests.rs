use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use super::*;
use crate::tools_herdr::args::shared_state;

fn args(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("objeto")
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
    let result = handle_voice(
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
    let result = handle_voice(
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
    let result = handle_voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "close_and_watch"})),
    );
    assert!(!result.ok);
    assert_eq!(
        result.error.as_deref(),
        Some("No hay ningún agente al que vigilar. Indica la dirección del agente con «address».")
    );
    let result = handle_voice(
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
    let missing = handle_voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mic_mute"})),
    );
    assert_eq!(
        missing.error.as_deref(),
        Some("Para la acción «mic_mute» falta el parámetro booleano «muted».")
    );
    let muted = handle_voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mic_mute", "muted": true})),
    );
    assert_eq!(muted.data["muted"], json!(true));
    assert_eq!(muted.data["message"], json!("Micrófono silenciado."));
    let active = handle_voice(
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
    let result = handle_voice(
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
    let missing = handle_voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mute"})),
    );
    assert_eq!(
        missing.error.as_deref(),
        Some("Para la acción «mute» falta el parámetro entero «minutes».")
    );
    let out_of_range = handle_voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mute", "minutes": 0})),
    );
    assert_eq!(
        out_of_range.error.as_deref(),
        Some("El parámetro «minutes» debe estar entre 1 y 480.")
    );
    let huge = handle_voice(
        &callbacks(&recorded, ""),
        &state,
        &args(json!({"action": "mute", "minutes": 18446744073709551615u64})),
    );
    assert_eq!(
        huge.error.as_deref(),
        Some("El parámetro «minutes» debe estar entre 1 y 480.")
    );
    let muted = handle_voice(
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
    let result = handle_voice(
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

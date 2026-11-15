//! Paridad de `agent/confirm.py` contra `parity/fixtures/confirm.jsonl` (P-AGENT-A).
//!
//! La interfaz se simula dentro de `on_ask`, como en el oráculo; los casos con
//! UI rota usan un hilo y un plazo corto.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ez_agent::confirm::{ConfirmationManager, CONFIRM_WORD};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/confirm.jsonl";
const ACCEPT: [&str; 5] = ["sí", "si", "confirmo", "adelante", "dale"];
const REJECT: [&str; 3] = ["no", "cancela", "para"];

fn field<'a>(input: &'a Value, name: &str) -> &'a Value {
    input.get(name).unwrap_or(&Value::Null)
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

/// Lo que la interfaz simulada observa durante `ask`.
#[derive(Default)]
struct Seen {
    seen: Vec<Value>,
    nested: Vec<Value>,
    results: Vec<Value>,
    token_present: Vec<Value>,
}

fn apply(manager: &ConfirmationManager, action: &Value, seen: &mut Seen) {
    match action
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "text" => {
            let text = action
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let token = match action.get("token").and_then(Value::as_str) {
                Some("current") => manager.pending_token(),
                Some(_) => Some("token-atrasado".to_string()),
                None => None,
            };
            let result = manager.submit_text(text, token.as_deref());
            seen.results.push(json!(result));
        }
        "click" => {
            manager.submit_click(action.get("yes").and_then(Value::as_bool).unwrap_or(false));
            seen.results.push(Value::Null);
        }
        "cancel" => {
            manager.cancel("el usuario habló de otra cosa");
            seen.results.push(Value::Null);
        }
        "arm" => {
            manager.arm(None);
            seen.results.push(Value::Null);
        }
        "second_ask" => {
            seen.nested
                .push(json!(manager.ask("otra cosa", false, None)));
            seen.nested.push(json!(manager.last_outcome()));
        }
        _ => {}
    }
}

/// Construye el gestor con un `on_ask` que se llama a sí mismo, como el oráculo.
fn manager_with_replies(
    reply: Vec<Value>,
    timeout_s: f64,
) -> (Arc<ConfirmationManager>, Arc<Mutex<Seen>>) {
    let holder: Arc<OnceLock<Arc<ConfirmationManager>>> = Arc::new(OnceLock::new());
    let observed: Arc<Mutex<Seen>> = Arc::default();
    let (slot, seen) = (Arc::clone(&holder), Arc::clone(&observed));
    let on_ask = Box::new(move |summary: &str, strong: bool| {
        let manager = slot.get().expect("gestor ya creado").clone();
        let mut seen = seen.lock().expect("vista previa");
        seen.seen.push(json!([summary, strong]));
        seen.token_present
            .push(json!(manager.pending_token().is_some()));
        for action in &reply {
            apply(&manager, action, &mut seen);
        }
    });
    let manager = Arc::new(ConfirmationManager::new(
        &strings(&ACCEPT),
        &strings(&REJECT),
        timeout_s,
        Some(on_ask),
    ));
    let _ = holder.set(Arc::clone(&manager));
    (manager, observed)
}

fn ask_output(
    accepted: bool,
    manager: &ConfirmationManager,
    seen: &Seen,
    late: Option<bool>,
) -> Value {
    json!({
        "accepted": accepted,
        "seen": seen.seen,
        "nested": seen.nested,
        "results": seen.results,
        "token_present": seen.token_present,
        "last_outcome": manager.last_outcome(),
        "pending": manager.pending(),
        "pending_token_present": manager.pending_token().is_some(),
        "late": late,
    })
}

fn ask_case(input: &Value) -> Value {
    let reply = field(input, "reply")
        .as_array()
        .cloned()
        .unwrap_or_default();
    let timeout_s = field(input, "timeout_s").as_f64().unwrap_or(0.0);
    let (manager, seen) = manager_with_replies(reply, timeout_s);
    let accepted = manager.ask(
        field(input, "summary").as_str().unwrap_or_default(),
        field(input, "strong").as_bool().unwrap_or(false),
        field(input, "require_number").as_i64(),
    );
    let late = input
        .get("late")
        .and_then(Value::as_str)
        .and_then(|text| manager.submit_text(text, None));
    let seen = seen.lock().expect("vista previa");
    ask_output(accepted, &manager, &seen, late)
}

/// La UI revienta al plantear la pregunta: la confirmación sigue viva.
fn broken_ui_case() -> Value {
    let on_ask = Box::new(|_: &str, _: bool| panic!("ui rota"));
    let manager = Arc::new(ConfirmationManager::new(
        &strings(&ACCEPT),
        &strings(&REJECT),
        5.0,
        Some(on_ask),
    ));
    let worker = Arc::clone(&manager);
    let handle = std::thread::spawn(move || worker.ask("¿?", false, None));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !manager.pending() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    manager.cancel("prueba");
    let accepted = handle.join().unwrap_or(true);
    json!({
        "accepted": accepted,
        "last_outcome": manager.last_outcome(),
        "pending": manager.pending(),
    })
}

fn run_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    Ok(match case.function.as_str() {
        "ask" if input.get("on_ask").is_some() => broken_ui_case(),
        "ask" if input.get("reply").is_none() => {
            let manager = ConfirmationManager::new(
                &strings(&ACCEPT),
                &strings(&REJECT),
                field(input, "timeout_s").as_f64().unwrap_or(0.0),
                None,
            );
            json!({
                "accepted": manager.ask(
                    field(input, "summary").as_str().unwrap_or_default(),
                    field(input, "strong").as_bool().unwrap_or(false),
                    field(input, "require_number").as_i64(),
                ),
                "last_outcome": manager.last_outcome(),
                "pending": manager.pending(),
            })
        }
        "ask" => ask_case(input),
        "submit_text" => {
            let manager = ConfirmationManager::new(&strings(&ACCEPT), &strings(&REJECT), 0.0, None);
            let submitted = manager.submit_text(
                field(input, "text").as_str().unwrap_or_default(),
                input.get("token").and_then(Value::as_str),
            );
            manager.submit_click(true);
            manager.cancel("nada pendiente");
            json!({
                "submitted": submitted,
                "clicked": Value::Null,
                "cancelled": Value::Null,
                "last_outcome": manager.last_outcome(),
                "pending": manager.pending(),
            })
        }
        "module_constants" => json!({"confirm_word": CONFIRM_WORD}),
        other => return Err(format!("función sin portar: {other}")),
    })
}

#[test]
fn every_confirm_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de confirm legibles");
    let mut failures = Vec::new();
    let mut cases_run = 0;
    for case in &cases {
        let got = run_case(case);
        let ok = match (&got, &case.error) {
            (Ok(value), None) => value == &case.output,
            (Err(message), Some(expected)) => message == expected,
            _ => false,
        };
        if !ok {
            failures.push(format!(
                "{}: {got:?} != {:?} / {:?}",
                case.case, case.output, case.error
            ));
        }
        cases_run += 1;
    }
    assert!(
        failures.is_empty(),
        "{} fallos:\n{}",
        failures.len(),
        failures.join("\n")
    );
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 37, "faltan casos en el fixture");
}

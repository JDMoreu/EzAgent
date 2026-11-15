//! Paridad de `src/ezagent/voice/condenser.py` contra `parity/fixtures/condenser.jsonl`.

use std::path::Path;
use std::sync::Mutex;

use ez_llm::condenser::{Condenser, CondenserConfig, TextCompleter};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/condenser.jsonl";

/// La petición que arma `_complete_text` del Python, para comparar con el oráculo.
fn request_body(
    provider: &str,
    model: &str,
    messages: &[Value],
    max_tokens: u32,
) -> serde_json::Map<String, Value> {
    let mut request = json!({
        "model": model,
        "messages": messages,
        "max_tokens": max_tokens,
        "temperature": 0,
        "timeout": 10,
    });
    if provider == "opencode" || provider == "openai" {
        request["extra_body"] = json!({"reasoning_effort": "none"});
    }
    request.as_object().cloned().unwrap_or_default()
}

/// Cliente guionizado que imita el mock `_Completions` del generador.
struct Scripted {
    reply: String,
    mode: String,
    requests: Mutex<Vec<Value>>,
    opened: Mutex<bool>,
}

impl Scripted {
    fn new(reply: &str, mode: &str) -> Self {
        Self {
            reply: reply.to_string(),
            mode: mode.to_string(),
            requests: Mutex::new(Vec::new()),
            opened: Mutex::new(false),
        }
    }

    fn record(&self, provider: &str, model: &str, messages: &[Value], max_tokens: u32) {
        let mut body = request_body(provider, model, messages, max_tokens);
        body.insert("stream".to_string(), Value::Bool(true));
        self.requests
            .lock()
            .expect("peticiones del condensador")
            .push(Value::Object(body));
    }
}

impl TextCompleter for Scripted {
    fn complete_text(
        &self,
        provider: &str,
        model: &str,
        messages: &[Value],
        max_tokens: u32,
    ) -> Result<String, String> {
        if self.mode == "nostream" {
            // `TypeError` al pasar `stream=True`: se repite sin `stream`.
            let mut with_stream = request_body(provider, model, messages, max_tokens);
            with_stream.insert("stream".to_string(), Value::Bool(true));
            let mut without_stream = request_body(provider, model, messages, max_tokens);
            without_stream.remove("stream");
            let mut requests = self.requests.lock().expect("peticiones del condensador");
            requests.push(Value::Object(with_stream));
            requests.push(Value::Object(without_stream));
            return Ok(self.reply.clone());
        }
        self.record(provider, model, messages, max_tokens);
        match self.mode.as_str() {
            "ignore_stream" => Ok(self.reply.clone()),
            "raise" => Err("el modelo no responde".to_string()),
            "broken" => {
                *self.opened.lock().expect("stream del condensador") = true;
                Err("stream cortado".to_string())
            }
            "no_choices" => {
                *self.opened.lock().expect("stream del condensador") = true;
                Ok(String::new())
            }
            _ => {
                *self.opened.lock().expect("stream del condensador") = true;
                Ok(self.reply.clone())
            }
        }
    }
}

fn produced(case: &FixtureCase) -> Result<Value, String> {
    let lines = case
        .input
        .get("lines")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let reply = case
        .input
        .get("reply")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mode = case
        .input
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("stream");
    let provider = case
        .input
        .get("provider")
        .and_then(Value::as_str)
        .unwrap_or("openai");
    let client = Scripted::new(reply, mode);
    let cfg = CondenserConfig {
        llm_provider: provider.to_string(),
        llm_model: "modelo-baratito".to_string(),
    };
    let start = Condenser::new(&client, cfg)
        .conclusion_start_values(&lines)
        .map_err(|error| error.to_string())?;
    let opened = *client.opened.lock().expect("stream del condensador");
    Ok(json!({
        "start": start,
        "requests": client.requests.lock().expect("peticiones del condensador").clone(),
        "closed": if opened { Some(true) } else { None },
    }))
}

#[test]
fn every_condenser_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de condenser legibles");
    let mut cases_run = 0;
    for case in &cases {
        cases_run += 1;
        match (produced(case), &case.error) {
            (Ok(value), None) => assert_eq!(value, case.output, "caso {}", case.case),
            (Err(error), Some(expected)) => assert_eq!(&error, expected, "caso {}", case.case),
            (Ok(value), Some(expected)) => panic!(
                "caso {}: Python falla con {expected:?} y Rust devolvió {value}",
                case.case
            ),
            (Err(error), None) => panic!("caso {}: fallo inesperado: {error}", case.case),
        }
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_run >= 41, "muy pocos casos: {cases_run}");
}

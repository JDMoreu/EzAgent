//! Paridad de los textos de la app contra `parity/fixtures/app_text.jsonl` (P-APP).
//!
//! Cubre `_Sentences`, `_strip_courtesy`, `_is_filler_sentence`, `_hotkey_key`
//! y `_fast_command`. `_fast_command` se ejecuta con un doble de `FastHost` que
//! reproduce el de `parity/gen/app_text.py`: sin etiquetas de Herdr, la frase de
//! pausa «oye ez» y el trabajador de agentes ejecutado en el momento.

use std::path::Path;

use ez_app::app::core::{hotkey_key, is_filler_sentence};
use ez_app::fast::{
    agents_report, fast_command, strip_courtesy, CloseMode, FastHost, AGENTS_UNAVAILABLE,
};
use ez_app::sentences::Sentences;
use ez_core::config::{python_str, truthy};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Map, Value};

const FIXTURES: &str = "../../../parity/fixtures/app_text.jsonl";
/// `_pause_message()` del doble: `cfg.wake_phrase = "oye ez"`.
const PAUSE: &str = "En pausa. Di «oye ez» cuando me necesites.";

#[derive(Clone)]
enum Agents {
    Rows(Vec<Map<String, Value>>),
    Raise,
}

struct Host {
    close_action: String,
    last_summary: String,
    tasks: Vec<Map<String, Value>>,
    agents: Agents,
}

impl FastHost for Host {
    fn close_action(&self) -> String {
        self.close_action.clone()
    }

    fn last_summary(&self) -> String {
        self.last_summary.clone()
    }

    fn pause_message(&self) -> String {
        PAUSE.to_string()
    }

    fn pending_tasks(&self) -> Vec<Map<String, Value>> {
        self.tasks.clone()
    }
}

#[test]
fn every_app_text_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de app_text legibles");
    let mut cases_run = 0;
    for case in &cases {
        match case.function.as_str() {
            "_Sentences.feed" => compare_feed(case),
            "_Sentences.flush" => compare_flush(case),
            "_strip_courtesy" => compare_courtesy(case),
            "_is_filler_sentence" => compare_filler(case),
            "_hotkey_key" => compare_hotkey(case),
            "_fast_command" => compare_fast(case),
            other => panic!("caso sin cubrir: {} ({other})", case.case),
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(
        cases_run, cases_in_file,
        "se saltó algún caso de {FIXTURES}"
    );
    assert!(
        cases_in_file >= 169,
        "el oráculo debe traer los 169 casos: {cases_in_file}"
    );
}

fn compare_feed(case: &FixtureCase) {
    if case.error.is_some() {
        // `feed(None)` concatena `None` a un `str`; la firma Rust no lo admite.
        assert!(
            !case.input["chunks"][0].is_string(),
            "caso {}: error inesperado con texto",
            case.case
        );
        return;
    }
    let mut sentences = Sentences::new(
        number(&case.input, "max_chars"),
        number(&case.input, "first_chars"),
    );
    let mut emitted: Vec<String> = Vec::new();
    for chunk in strings(&case.input["chunks"]) {
        emitted.extend(sentences.feed(&chunk));
    }
    let flushed = if case.input["flush"].as_bool().unwrap_or(false) {
        sentences.flush()
    } else {
        Vec::new()
    };
    let produced = json!({
        "emitted": emitted,
        "flushed": flushed,
        "buffer": sentences.buf,
    });
    assert_eq!(produced, case.output, "caso {}", case.case);
}

fn compare_flush(case: &FixtureCase) {
    let mut sentences = Sentences::default();
    sentences.buf = text(&case.input, "buffer").to_string();
    let produced = json!({
        "flushed": sentences.flush(),
        "buffer": sentences.buf,
    });
    assert_eq!(produced, case.output, "caso {}", case.case);
}

fn compare_courtesy(case: &FixtureCase) {
    if case.error.is_some() {
        // `key.split()` de `None` o de un `int` no tiene equivalente en Rust.
        assert!(
            !case.input["key"].is_string(),
            "caso {}: error inesperado con texto",
            case.case
        );
        return;
    }
    let produced = json!({"text": strip_courtesy(text(&case.input, "key"))});
    assert_eq!(produced, case.output, "caso {}", case.case);
}

fn compare_filler(case: &FixtureCase) {
    if case.error.is_some() {
        // `sentence.lower()` de `None` no tiene equivalente en la firma Rust.
        assert!(
            !case.input["sentence"].is_string(),
            "caso {}: error inesperado con texto",
            case.case
        );
        return;
    }
    let produced = json!({"is_filler": is_filler_sentence(text(&case.input, "sentence"))});
    assert_eq!(produced, case.output, "caso {}", case.case);
}

fn compare_hotkey(case: &FixtureCase) {
    // `str(spec or "")` admite `None` y números: se reproducen igual que Python.
    let spec = &case.input["spec"];
    let spec = if truthy(spec) {
        python_str(spec)
    } else {
        String::new()
    };
    let produced = json!({"key": hotkey_key(&spec)});
    assert_eq!(produced, case.output, "caso {}", case.case);
}

fn compare_fast(case: &FixtureCase) {
    if case.error.is_some() {
        // `text.lower()` de `None` o de un `int` no tiene equivalente en Rust.
        assert!(
            !case.input["text"].is_string(),
            "caso {}: error inesperado con texto",
            case.case
        );
        return;
    }
    let host = host_of(case);
    let outcome = fast_command(text(&case.input, "text"), &host);
    let spoken = match &outcome.speech {
        Some((kind, said)) => json!([[said, kind]]),
        None => json!([]),
    };
    let events = if outcome.list_agents {
        let said = match &host.agents {
            Agents::Rows(rows) => agents_report(rows),
            Agents::Raise => AGENTS_UNAVAILABLE.to_string(),
        };
        json!([["say", said]])
    } else {
        json!([])
    };
    let produced = json!({
        "handled": outcome.handled,
        "spoken": spoken,
        "muted": outcome.mic.unwrap_or(false),
        "close_after_answer": outcome.close.is_some(),
        "close_mode": outcome.close.map(CloseMode::as_str).unwrap_or(""),
        "events": events,
    });
    assert_eq!(produced, case.output, "caso {}", case.case);
}

fn host_of(case: &FixtureCase) -> Host {
    let tasks = case.input["tasks"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_object().cloned())
                .collect()
        })
        .unwrap_or_default();
    let agents = match &case.input["agents"] {
        Value::String(mode) if mode == "raise" => Agents::Raise,
        Value::Array(items) => Agents::Rows(
            items
                .iter()
                .filter_map(|item| item.as_object().cloned())
                .collect(),
        ),
        _ => Agents::Rows(Vec::new()),
    };
    let close_action = case.input["close_action"].as_str().unwrap_or("pause");
    Host {
        close_action: close_action.to_string(),
        last_summary: text(&case.input, "last_summary").to_string(),
        tasks,
        agents,
    }
}

fn text<'value>(value: &'value Value, key: &str) -> &'value str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("falta el texto {key}"))
}

fn number(value: &Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("falta el entero {key}")) as usize
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("falta la lista de trozos"))
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("trozo no textual"))
                .to_string()
        })
        .collect()
}

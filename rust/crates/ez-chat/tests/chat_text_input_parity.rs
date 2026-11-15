//! Paridad de `voice/chat/text_input.py` contra `parity/fixtures/chat_text_input.jsonl`.
//!
//! Los scripts tocan un solo campo en memoria; no hay entorno ni estado global,
//! así que cada caso es independiente.

use std::path::Path;

use ez_chat::text_input::{sanitize, Move, TextInput};
use ez_core::pyvalue::{python_str, text_or_empty};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/chat_text_input.jsonl";

fn snapshot(field: &TextInput) -> Value {
    let (line, col) = field.line_col();
    json!({
        "text": field.text,
        "cursor": field.cursor,
        "anchor": field.anchor,
        "selection": field
            .selection()
            .map_or(Value::Null, |(start, end)| json!([start, end])),
        "selected": field.selected_text(),
        "line": line,
        "col": col,
        "empty": field.empty(),
    })
}

fn flag(op: &Value, key: &str) -> bool {
    op.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn key_of(op: &Value) -> Move {
    match op.get("key").and_then(Value::as_str).unwrap_or("left") {
        "left" => Move::Left,
        "right" => Move::Right,
        "up" => Move::Up,
        "down" => Move::Down,
        "home" => Move::Home,
        "end" => Move::End,
        other => panic!("tecla desconocida: {other}"),
    }
}

fn op_text(op: &Value) -> String {
    match op.get("text") {
        Some(value) => python_str(value),
        None => String::new(),
    }
}

fn apply(field: &mut TextInput, op: &Value) -> Value {
    match op.get("op").and_then(Value::as_str).unwrap_or("") {
        "insert" => {
            field.insert(&op_text(op));
            Value::Null
        }
        "newline" => {
            field.newline();
            Value::Null
        }
        "backspace" => {
            field.backspace(flag(op, "word"));
            Value::Null
        }
        "delete" => {
            field.delete(flag(op, "word"));
            Value::Null
        }
        "clear" => {
            field.clear();
            Value::Null
        }
        "take" => json!(field.take()),
        "set_text" => {
            field.set_text(&op_text(op));
            Value::Null
        }
        "select_all" => {
            field.select_all();
            Value::Null
        }
        "move" => {
            field.move_cursor(key_of(op), flag(op, "select"), flag(op, "word"));
            Value::Null
        }
        "selection" => field
            .selection()
            .map_or(Value::Null, |(start, end)| json!([start, end])),
        "selected_text" => json!(field.selected_text()),
        "line_col" => {
            let (line, col) = field.line_col();
            json!([line, col])
        }
        "empty" => json!(field.empty()),
        other => panic!("operación desconocida: {other}"),
    }
}

fn run_script(case: &FixtureCase) -> Value {
    let text = text_or_empty(case.input.get("text"));
    let mut field = TextInput::new(&text);
    let mut steps: Vec<Value> = Vec::new();
    for op in case
        .input
        .get("ops")
        .and_then(Value::as_array)
        .expect("ops del caso")
    {
        let result = apply(&mut field, op);
        let mut step = snapshot(&field);
        step["op"] = op.get("op").cloned().unwrap_or(Value::Null);
        step["result"] = result;
        steps.push(step);
    }
    json!({"steps": steps, "final": snapshot(&field)})
}

#[test]
fn every_chat_text_input_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de chat_text_input legibles");
    let mut cases_run = 0;
    for case in &cases {
        assert_eq!(case.error, None, "caso {} con error", case.case);
        if case.function == "sanitize" {
            let text = text_or_empty(case.input.get("text"));
            assert_eq!(
                json!({"text": sanitize(&text)}),
                case.output,
                "caso {}",
                case.case
            );
            cases_run += 1;
            continue;
        }
        assert_eq!(run_script(case), case.output, "caso {}", case.case);
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 299, "muy pocos casos: {cases_in_file}");
}

//! Paridad de `src/ezagent/voice/tool_markup.py` contra `parity/fixtures/tool_markup.jsonl`.

use std::path::Path;

use ez_llm::tool_markup::ToolMarkupGuard;
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/tool_markup.jsonl";

fn chunks_of(case: &FixtureCase) -> Vec<String> {
    case.input
        .get("chunks")
        .and_then(Value::as_array)
        .map(|chunks| {
            chunks
                .iter()
                .map(|chunk| chunk.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn produced(case: &FixtureCase) -> Value {
    let mut guard = ToolMarkupGuard::new();
    let mut text = String::new();
    for chunk in chunks_of(case) {
        text.push_str(&guard.feed(&chunk));
    }
    match case.function.as_str() {
        "feed" => json!({"text": text, "tripped": guard.tripped}),
        "flush" => {
            let released = guard.flush();
            let again = guard.flush();
            json!({
                "text": text,
                "flush": released,
                "flush_again": again,
                "tripped": guard.tripped,
            })
        }
        other => panic!("función desconocida: {other}"),
    }
}

#[test]
fn every_tool_markup_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de tool_markup legibles");
    let mut cases_run = 0;
    for case in &cases {
        cases_run += 1;
        assert_eq!(case.error, None, "caso {}", case.case);
        assert_eq!(produced(case), case.output, "caso {}", case.case);
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_run >= 111, "muy pocos casos: {cases_run}");
}

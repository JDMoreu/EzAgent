//! Paridad de `voice/chat/markdown.py` contra `parity/fixtures/chat_markdown.jsonl`.

use std::path::Path;

use ez_chat::markdown::{
    clean_target, is_local_target, parse, parse_inline, plain_text, Block, Span,
};
use ez_core::pyvalue::{python_str, text_or_empty};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/chat_markdown.jsonl";

fn span_json(span: &Span) -> Value {
    json!({
        "text": span.text,
        "bold": span.bold,
        "italic": span.italic,
        "code": span.code,
        "link": span.link,
    })
}

fn spans_json(spans: &[Span]) -> Value {
    Value::Array(spans.iter().map(span_json).collect())
}

fn block_json(block: &Block) -> Value {
    match block {
        Block::Heading(item) => {
            json!({"type": "heading", "level": item.level, "spans": spans_json(&item.spans)})
        }
        Block::Paragraph(item) => {
            json!({"type": "paragraph", "spans": spans_json(&item.spans)})
        }
        Block::ListItem(item) => json!({
            "type": "list_item",
            "level": item.level,
            "marker": item.marker,
            "checked": item.checked,
            "spans": spans_json(&item.spans),
        }),
        Block::Quote(item) => json!({"type": "quote", "spans": spans_json(&item.spans)}),
        Block::Code(item) => json!({"type": "code", "lang": item.lang, "text": item.text}),
        Block::Table(item) => json!({
            "type": "table",
            "header": item.header.iter().map(|cell| spans_json(cell)).collect::<Vec<_>>(),
            "rows": item
                .rows
                .iter()
                .map(|row| row.iter().map(|cell| spans_json(cell)).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            "aligns": item.aligns.iter().map(|align| align.as_str()).collect::<Vec<_>>(),
        }),
        Block::Rule(_) => json!({"type": "rule"}),
        Block::Image(item) => {
            json!({"type": "image", "alt": item.alt, "src": item.src})
        }
        Block::Document(item) => {
            json!({"type": "document", "label": item.label, "target": item.target})
        }
    }
}

fn text_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// Los casos de error del oráculo son llamadas con `None`: la firma Rust no
/// admite texto ausente. Se consumen comprobando esa frontera.
fn consumes_none_boundary(case: &FixtureCase) {
    assert_eq!(
        case.error.as_deref(),
        Some("'NoneType' object has no attribute 'strip'"),
        "caso {}: error inesperado",
        case.case
    );
    assert!(
        case.input.get("target").is_some_and(Value::is_null),
        "caso {}: el error no es por `target` nulo",
        case.case
    );
}

#[test]
fn every_chat_markdown_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de chat_markdown legibles");
    let mut cases_run = 0;
    for case in &cases {
        if case.error.is_some() {
            consumes_none_boundary(case);
            cases_run += 1;
            continue;
        }
        match case.function.as_str() {
            "clean_target" => {
                let target = text_field(&case.input, "target").expect("target");
                assert_eq!(
                    json!({"target": clean_target(target)}),
                    case.output,
                    "caso {}",
                    case.case
                );
            }
            "is_local_target" => {
                let target = text_field(&case.input, "target").expect("target");
                assert_eq!(
                    json!({"local": is_local_target(target)}),
                    case.output,
                    "caso {}",
                    case.case
                );
            }
            "parse_inline" => {
                let text = match case.input.get("text") {
                    Some(value) => python_str(value),
                    None => python_str(&Value::Null),
                };
                assert_eq!(
                    json!({"spans": spans_json(&parse_inline(&text))}),
                    case.output,
                    "caso {}",
                    case.case
                );
            }
            "parse" => {
                let text = text_or_empty(case.input.get("text"));
                let blocks: Vec<Value> = parse(&text).iter().map(block_json).collect();
                assert_eq!(json!({"blocks": blocks}), case.output, "caso {}", case.case);
            }
            "plain_text" => {
                let markdown = text_or_empty(case.input.get("markdown"));
                assert_eq!(
                    json!({"text": plain_text(&parse(&markdown))}),
                    case.output,
                    "caso {}",
                    case.case
                );
            }
            other => panic!("función desconocida en {}: {other}", case.case),
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 185, "muy pocos casos: {cases_in_file}");
}

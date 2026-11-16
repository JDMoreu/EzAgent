//! Recorte de argumentos de herramienta en cualquier punto (TMAP-4).

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_llm::tool_json::repair_arguments;
use serde_json::{json, Value};

const SAMPLE: &str = "{\"action\":\"run\",\"agent\":\"claude\",\"prompt\":\"Revisa el repositorio, \\\"cuidado\\\" con \\\\rutas\\\\ y acentos: ñandú ☕. Luego resume.\",\"options\":{\"background\":false,\"tags\":[\"a\",\"b\",{\"x\":1}],\"n\":3.5},\"empty\":\"\"}";

#[test]
fn every_possible_truncation_gives_valid_json() {
    let raw = dumps_with(
        &serde_json::from_str::<Value>(SAMPLE).expect("muestra válida"),
        &DumpOptions::utf8(),
    );
    let mut ends: Vec<usize> = raw.char_indices().map(|(index, _)| index).collect();
    ends.push(raw.len());
    for end in ends {
        let prefix = &raw[..end];
        let (fixed, complete) = repair_arguments(Some(prefix));
        let value: Value = serde_json::from_str(&fixed)
            .unwrap_or_else(|error| panic!("corte {end} ({prefix:?}): {error}"));
        assert!(value.is_object(), "corte {end}: {fixed}");
        if !prefix.trim().is_empty() && serde_json::from_str::<Value>(prefix).is_err() {
            assert!(!complete, "corte {end} reparado sin avisar: {fixed}");
        }
    }
    assert!(repair_arguments(Some(&raw)).1);
}

#[test]
fn a_cut_report_is_repaired_and_flagged() {
    let cut =
        "{\"action\":\"run\",\"agent\":\"claude\",\"prompt\":\"Revisa el repositorio y resume el estado de";
    let (fixed, complete) = repair_arguments(Some(cut));
    assert!(!complete);
    let parsed: Value = serde_json::from_str(&fixed).expect("JSON reparado");
    assert_eq!(parsed["action"], json!("run"));
    assert!(parsed["prompt"]
        .as_str()
        .unwrap_or("")
        .starts_with("Revisa el repositorio"));
}

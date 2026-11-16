//! Validación estricta del veredicto (10.11) y churn (10.14).

use crate::autonomous::verdict::{is_churn, similarity, Severity, Verdict, VerdictSpec};

const TOKEN: &str = "0123456789abcdef";

fn spec() -> VerdictSpec<'static> {
    VerdictSpec {
        token: TOKEN,
        criteria: 2,
    }
}

fn criteria_both_true() -> &'static str {
    r#"{"n":1,"cumple":true,"evidencia":"cargo build"},
       {"n":2,"cumple":true,"evidencia":"cargo test"}"#
}

fn body(cumple: &str, criteria: &str, findings: &str) -> String {
    format!(
        r#"{{"token":"{TOKEN}","cumple":{cumple},"criterios":[{criteria}],"hallazgos":[{findings}]}}"#
    )
}

#[test]
fn accepts_a_complete_verdict() {
    let verdict = Verdict::parse(&body("true", criteria_both_true(), ""), &spec()).expect("válido");
    assert!(verdict.clean());
    assert_eq!(verdict.criteria.len(), 2);
    assert_eq!(verdict.criteria[1].evidencia, "cargo test");
}

#[test]
fn accepts_the_json_inside_a_markdown_fence_and_ignores_extra_keys() {
    let json = format!(
        r#"{{"token":"{TOKEN}","cumple":true,"nota":"extra",
            "criterios":[{{"n":1,"cumple":true,"evidencia":"ok","x":1}},
                         {{"n":2,"cumple":true,"evidencia":"ok"}}],
            "hallazgos":[]}}"#
    );
    let text = format!("```json\n{json}\n```");
    assert!(Verdict::parse(&text, &spec()).is_ok());
    let plain = format!("```\n{json}\n```");
    assert!(Verdict::parse(&plain, &spec()).is_ok());
    let with_prose = format!("Aquí está el veredicto:\n\n{text}\n\nListo.");
    assert!(Verdict::parse(&with_prose, &spec()).is_ok());
}

#[test]
fn a_fence_inside_the_json_does_not_break_the_analysis() {
    let finding = r#"{"severidad":"menor","criterio":null,"problema":"estilo",
                     "evidencia":"```sh\necho hola\n```"}"#;
    let verdict =
        Verdict::parse(&body("true", criteria_both_true(), finding), &spec()).expect("válido");
    assert_eq!(verdict.findings[0].severity, Severity::Minor);
}

#[test]
fn rejects_a_wrong_or_missing_token() {
    let other = body("true", criteria_both_true(), "").replace(TOKEN, "ffffffffffffffff");
    assert!(Verdict::parse(&other, &spec()).is_err());
    let missing = format!(
        r#"{{"cumple":true,"criterios":[{criteria}],"hallazgos":[]}}"#,
        criteria = criteria_both_true()
    );
    assert!(Verdict::parse(&missing, &spec()).is_err());
}

#[test]
fn every_criterion_needs_its_own_entry_with_evidence() {
    let repeated = r#"{"n":1,"cumple":true,"evidencia":"a"},
                      {"n":1,"cumple":true,"evidencia":"b"}"#;
    assert!(Verdict::parse(&body("true", repeated, ""), &spec()).is_err());
    let out_of_range = r#"{"n":1,"cumple":true,"evidencia":"a"},
                          {"n":3,"cumple":true,"evidencia":"b"}"#;
    assert!(Verdict::parse(&body("true", out_of_range, ""), &spec()).is_err());
    let missing = r#"{"n":1,"cumple":true,"evidencia":"a"}"#;
    assert!(Verdict::parse(&body("true", missing, ""), &spec()).is_err());
    let empty = r#"{"n":1,"cumple":true,"evidencia":"a"},
                   {"n":2,"cumple":true,"evidencia":"   "}"#;
    assert!(Verdict::parse(&body("true", empty, ""), &spec()).is_err());
}

#[test]
fn cumple_must_match_all_the_criteria() {
    let one_false = r#"{"n":1,"cumple":true,"evidencia":"a"},
                       {"n":2,"cumple":false,"evidencia":"b"}"#;
    assert!(Verdict::parse(&body("true", one_false, ""), &spec()).is_err());
    let verdict = Verdict::parse(&body("false", one_false, ""), &spec()).expect("coherente");
    assert!(!verdict.clean());
}

#[test]
fn severities_must_be_known() {
    let finding = r#"{"severidad":"grave","criterio":1,"problema":"x","evidencia":"y"}"#;
    assert!(Verdict::parse(&body("true", criteria_both_true(), finding), &spec()).is_err());
    let major = r#"{"severidad":"mayor","criterio":null,"problema":"x","evidencia":"y"}"#;
    let verdict = Verdict::parse(&body("true", criteria_both_true(), major), &spec()).expect("ok");
    assert!(!verdict.clean());
    assert_eq!(verdict.findings[0].severity, Severity::Major);
}

#[test]
fn broken_json_is_rejected() {
    assert!(Verdict::parse("{ roto", &spec()).is_err());
    assert!(Verdict::parse("[]", &spec()).is_err());
}

#[test]
fn churn_uses_normalized_similarity_and_the_whole_history() {
    assert!(
        similarity(
            "la prueba del criterio 2 falla",
            "La prueba del criterio 2 falla."
        ) >= 0.85
    );
    assert!(similarity("no compila", "falta documentación") < 0.85);
    let corrected = vec!["la prueba del criterio 2 falla".to_string()];
    assert!(is_churn(
        &["La prueba del criterio 2 falla.".to_string()],
        &corrected
    ));
    assert!(!is_churn(&["falta documentación".to_string()], &corrected));
}

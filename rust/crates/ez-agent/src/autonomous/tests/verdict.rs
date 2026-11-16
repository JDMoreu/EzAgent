//! Esquema del veredicto y detección de churn (9.3 y 9.5).

use crate::autonomous::verdict::{is_churn, similarity, Severity, Verdict, CHURN_SIMILARITY};

const VALID: &str = r#"{
  "cumple": false,
  "criterios": [
    {"n": 1, "cumple": true, "evidencia": "cargo test"},
    {"n": 2, "cumple": false, "evidencia": "error E0308"}
  ],
  "hallazgos": [
    {"severidad": "bloqueante", "criterio": 2, "problema": "no compila", "evidencia": "error E0308"},
    {"severidad": "mayor", "criterio": null, "problema": "falta la prueba", "evidencia": ""},
    {"severidad": "menor", "criterio": 1, "problema": "estilo", "evidencia": "clippy"}
  ]
}"#;

#[test]
fn parses_the_verdict_of_the_design() {
    let verdict = Verdict::parse(VALID).expect("veredicto válido");
    assert!(!verdict.cumple);
    assert_eq!(verdict.criteria.len(), 2);
    assert_eq!(verdict.criteria[1].n, 2);
    assert!(!verdict.criteria[1].cumple);
    assert_eq!(verdict.findings.len(), 3);
    assert_eq!(verdict.findings[0].severity, Severity::Blocking);
    assert_eq!(verdict.findings[1].criterion, None);
    assert!(!verdict.clean());
    assert_eq!(
        verdict.problems(),
        vec!["no compila".to_string(), "falta la prueba".to_string()]
    );
}

#[test]
fn rejects_invalid_schemas() {
    assert!(Verdict::parse("no es json").is_err());
    assert!(Verdict::parse("[]").is_err());
    assert!(Verdict::parse(r#"{"criterios": [], "hallazgos": []}"#).is_err());
    assert!(Verdict::parse(r#"{"cumple": "sí", "criterios": [], "hallazgos": []}"#).is_err());
    assert!(Verdict::parse(r#"{"cumple": true, "criterios": {}, "hallazgos": []}"#).is_err());
    assert!(Verdict::parse(
        r#"{"cumple": true, "criterios": [], "hallazgos": [{"severidad": "grave", "criterio": null, "problema": "x", "evidencia": ""}]}"#
    )
    .is_err());
    assert!(Verdict::parse(
        r#"{"cumple": true, "criterios": [], "hallazgos": [{"severidad": "menor", "criterio": "1", "problema": "x", "evidencia": ""}]}"#
    )
    .is_err());
}

#[test]
fn clean_needs_cumple_and_no_blocking_findings() {
    let clean = Verdict::parse(
        r#"{"cumple": true, "criterios": [{"n": 1, "cumple": true, "evidencia": "ok"}],
            "hallazgos": [{"severidad": "menor", "criterio": null, "problema": "estilo", "evidencia": ""}]}"#,
    )
    .expect("válido");
    assert!(clean.clean());
    let not_cumple =
        Verdict::parse(r#"{"cumple": false, "criterios": [], "hallazgos": []}"#).expect("válido");
    assert!(!not_cumple.clean());
}

#[test]
fn similarity_normalizes_accents_and_punctuation() {
    assert_eq!(
        similarity(
            "La prueba del criterio 2 falla",
            "la prueba del criterio 2 falla."
        ),
        1.0
    );
    assert!(similarity("Falta compilar el módulo", "Falta compilar el modulo") >= CHURN_SIMILARITY);
    assert!(similarity("no compila", "el estilo del código") < CHURN_SIMILARITY);
    assert_eq!(similarity("", ""), 1.0);
    assert_eq!(similarity("algo", ""), 0.0);
}

#[test]
fn churn_compares_with_the_corrected_findings() {
    let previous = vec!["La prueba del criterio 2 falla".to_string()];
    assert!(is_churn(
        &["la prueba del criterio 2 falla.".to_string()],
        &previous
    ));
    assert!(!is_churn(&["falta documentar".to_string()], &previous));
    assert!(!is_churn(&previous, &[]));
}

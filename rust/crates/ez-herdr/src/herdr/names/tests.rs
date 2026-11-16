//! Pruebas del resolvedor de nombres dictados: casos reales del log
//! («TES», «TESS», «TETS» → «TEST», «OpenCobe» → «OpenCode») y candidatos
//! parecidos que deben preguntar.

use std::path::PathBuf;

use ez_core::config::VoiceConfig;
use serde_json::json;

use super::super::runtime::SpawnTask;
use super::super::{Completed, HerdrRuntime, RunError};
use super::*;

fn candidates(pairs: &[(&str, &str)]) -> Vec<NameCandidate> {
    pairs
        .iter()
        .map(|(value, label)| NameCandidate::new(*value, *label))
        .collect()
}

fn spaces(names: &[&str]) -> Vec<NameCandidate> {
    names
        .iter()
        .map(|name| NameCandidate::new(*name, *name))
        .collect()
}

#[test]
fn normaliza_minusculas_sin_tildes_ni_signos() {
    assert_eq!(normalize("  ¡TEST!! "), "test");
    assert_eq!(normalize("Open Code"), "open code");
    assert_eq!(normalize("Peñarol_2"), "penarol 2");
    assert_eq!(normalize("   "), "");
}

#[test]
fn clave_fonetica_junta_sonidos() {
    assert_eq!(phonetic("TEST"), "test");
    assert_eq!(phonetic("TES"), phonetic("TESS"));
    assert_eq!(phonetic("Vaca"), phonetic("Baca"));
    assert_eq!(phonetic("Zeta"), phonetic("Seta"));
    assert_eq!(phonetic("Quasar"), phonetic("Kasar"));
    assert_eq!(phonetic("Clod"), phonetic("Clot"));
}

#[test]
fn tes_tess_y_tets_quedan_en_duda() {
    let test = spaces(&["TEST"]);
    for spoken in ["TES", "TESS", "TETS"] {
        assert_eq!(
            resolve_name(spoken, &test),
            NameMatch::Doubtful(NameHit {
                value: "TEST".to_string(),
                label: "TEST".to_string(),
            }),
            "{spoken}"
        );
    }
    assert_eq!(
        resolve_name("test", &test),
        NameMatch::Exact(NameHit {
            value: "TEST".to_string(),
            label: "TEST".to_string(),
        })
    );
}

#[test]
fn opencobe_es_opencode_sin_preguntar() {
    let herdr = Herdr::new(VoiceConfig::new());
    let agents = herdr.name_candidates(NameKind::Agent, false);
    assert!(agents.len() >= 7);
    match resolve_name("OpenCobe", &agents) {
        NameMatch::Confident(found) => {
            assert_eq!(found.value, "opencode");
            assert_eq!(found.label, "OpenCode");
        }
        other => panic!("se esperaba una coincidencia muy probable: {other:?}"),
    }
}

#[test]
fn los_aliases_de_los_perfiles_cuentan() {
    let herdr = Herdr::new(VoiceConfig::new());
    let agents = herdr.name_candidates(NameKind::Agent, false);
    match resolve_name("cloud code", &agents) {
        NameMatch::Exact(found) => assert_eq!(found.value, "claude"),
        other => panic!("se esperaba una coincidencia exacta: {other:?}"),
    }
}

#[test]
fn nombres_muy_distintos_no_coinciden() {
    let test = spaces(&["TEST"]);
    match resolve_name("zzz", &test) {
        NameMatch::Unknown { available } => assert_eq!(available, vec!["TEST".to_string()]),
        other => panic!("se esperaba sin coincidencia: {other:?}"),
    }
    assert!(matches!(
        resolve_name("TES", &[]),
        NameMatch::Unknown { available } if available.is_empty()
    ));
}

#[test]
fn dos_candidatos_parecidos_obligan_a_preguntar() {
    let both = spaces(&["TEST", "TESTO"]);
    assert!(matches!(
        resolve_name("TES", &both),
        NameMatch::Doubtful(found) if found.value == "TEST" || found.value == "TESTO"
    ));
}

#[test]
fn dos_coincidencias_exactas_son_duda() {
    let repeated = candidates(&[("t1", "planner"), ("t2", "planner")]);
    assert!(matches!(
        resolve_name("planner", &repeated),
        NameMatch::Doubtful(_)
    ));
}

#[test]
fn la_pregunta_y_la_nota_las_escribe_el_codigo() {
    assert_eq!(
        NameKind::Space.question("TEST"),
        "¿Te refieres al espacio «TEST»?"
    );
    assert_eq!(
        NameKind::Terminal.question("planner"),
        "¿Te refieres a la terminal «planner»?"
    );
    assert_eq!(
        NameKind::Agent.question("OpenCode"),
        "¿Te refieres al agente «OpenCode»?"
    );
    assert_eq!(NameKind::Space.note("TEST"), "usé el espacio «TEST»");
    assert_eq!(
        NameKind::Terminal.note("planner"),
        "usé la terminal «planner»"
    );
    assert_eq!(
        NameKind::Space.unknown("TES", &["TEST".to_string(), "EzAgent".to_string()]),
        "No encontré el espacio «TES». Espacios abiertos: «TEST», «EzAgent»."
    );
}

#[test]
fn las_etiquetas_no_se_repiten_y_van_ordenadas() {
    let list = candidates(&[("t2", "planner"), ("t1", "Claude Code"), ("t3", "planner")]);
    assert_eq!(
        candidate_labels(&list),
        vec!["Claude Code".to_string(), "planner".to_string()]
    );
}

struct FakeAgents(Vec<serde_json::Map<String, Value>>);

impl HerdrRuntime for FakeAgents {
    fn run(
        &self,
        _argv: &[String],
        _timeout: f64,
        _anonymous: bool,
    ) -> Result<Completed, RunError> {
        Err(RunError::NotFound)
    }

    fn which(&self, _program: &str) -> Option<PathBuf> {
        None
    }

    fn monotonic(&self) -> f64 {
        0.0
    }

    fn next_id(&self) -> String {
        "0000".to_string()
    }

    fn spawn(&self, _name: &str, _task: SpawnTask) -> bool {
        false
    }

    fn agents(&self) -> Option<Vec<serde_json::Map<String, Value>>> {
        Some(self.0.clone())
    }
}

fn herdr_with(agents: Vec<Value>) -> Herdr {
    let rows = agents
        .iter()
        .filter_map(Value::as_object)
        .cloned()
        .collect::<Vec<_>>();
    Herdr::with_runtime(VoiceConfig::new(), std::sync::Arc::new(FakeAgents(rows)))
}

fn live() -> Herdr {
    herdr_with(vec![
        json!({"address": "t1", "where": "EzAgent / planner", "name": "planner", "label": "planner de EzAgent"}),
        json!({"address": "t2", "where": "EzAgent / 2", "name": "2", "label": "pestaña 2 de EzAgent"}),
        json!({"address": "t3", "where": "Otro / TEST", "name": "TEST", "label": "TEST de Otro"}),
    ])
}

#[test]
fn los_candidatos_salen_de_lo_abierto_en_herdr() {
    let herdr = live();
    let names: Vec<String> = herdr
        .name_candidates(NameKind::Space, true)
        .into_iter()
        .map(|candidate| candidate.value)
        .collect();
    assert_eq!(names, vec!["EzAgent".to_string(), "Otro".to_string()]);
    let terminals = herdr.name_candidates(NameKind::Terminal, true);
    assert_eq!(terminals.len(), 3);
    assert_eq!(terminals[0].value, "t1");
    assert!(terminals[0].names.contains(&"planner".to_string()));
    assert!(terminals[2].names.contains(&"TEST".to_string()));
}

#[test]
fn resolve_safely_solo_usa_exactas_o_muy_probables() {
    let herdr = live();
    assert_eq!(
        herdr.resolve_safely(NameKind::Space, "ez agent"),
        Some("EzAgent".to_string())
    );
    assert_eq!(herdr.resolve_safely(NameKind::Space, "TES"), None);
    assert_eq!(
        herdr.resolve_safely(NameKind::Terminal, "planner"),
        Some("t1".to_string())
    );
    assert_eq!(herdr.resolve_safely(NameKind::Terminal, "zzz"), None);
    let empty = herdr_with(Vec::new());
    assert_eq!(
        empty.resolve_safely(NameKind::Space, "  TEST "),
        Some("TEST".to_string())
    );
}

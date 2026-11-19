//! R8 (verificación con tope) y R9 (cierre de ronda y paradas).

use std::fs;

use ez_arbiter::{Caller, Command, Verdict};
use ez_team::Role;

use super::support::{git, Fixture};
use super::task::{clean_round, new_task};

fn setup_with(f: &Fixture, body: &str) -> String {
    let outcome = new_task(f, f.criteria(body)).expect("proponer tarea");
    let task = outcome.data["task"].as_str().expect("id").to_string();
    f.arbiter.approve_task("s1").expect("aprobar tarea");
    f.team(Fixture::tasks());
    f.arbiter.approve_team("s1").expect("aprobar equipo");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("abrir ronda");
    f.launch(Role::Planner);
    f.report("plan-r1", Verdict::Pass, None, None)
        .expect("informe del planificador");
    task
}

fn verify(f: &Fixture) -> serde_json::Value {
    let outcome = f
        .arbiter
        .execute(&Fixture::elephant(), Command::Verify)
        .expect("verificar");
    outcome.data
}

fn passes(body: &str) -> String {
    format!("[[criterio]]\nid = \"C1\"\ndescripcion = \"Git\"\n{body}\n")
}

#[test]
fn r8_passes_when_code_and_text_match() {
    let f = Fixture::new("r8-pasa");
    let task = setup_with(
        &f,
        &passes("comando = [\"git\", \"--version\"]\ncontiene = \"git version\""),
    );
    let data = verify(&f);
    assert_eq!(data["criterios"][0]["ok"], true);
    assert_eq!(data["criterios"][0]["codigo"], 0);
    assert!(data["criterios"][0]["salida"]
        .as_str()
        .expect("salida")
        .contains("git version"));
    let verificacion = f
        .folder
        .join(".ez")
        .join("tareas")
        .join(&task)
        .join("ronda-1")
        .join("verificacion.json");
    assert!(verificacion.is_file());
}

#[test]
fn r8_fails_by_exit_code_and_by_expected_text() {
    let code = Fixture::new("r8-codigo");
    setup_with(
        &code,
        &passes("comando = [\"git\", \"--version\"]\ncodigo = 7\n"),
    );
    let data = verify(&code);
    assert_eq!(data["criterios"][0]["ok"], false);
    assert!(data["criterios"][0]["motivo"]
        .as_str()
        .expect("motivo")
        .contains("código"));

    let text = Fixture::new("r8-texto");
    setup_with(
        &text,
        &passes("comando = [\"git\", \"--version\"]\ncontiene = \"nunca aparece\"\n"),
    );
    let data = verify(&text);
    assert_eq!(data["criterios"][0]["ok"], false);
    assert!(data["criterios"][0]["motivo"]
        .as_str()
        .expect("motivo")
        .contains("no contiene"));
}

#[test]
fn r8_timeout_kills_the_process() {
    let f = Fixture::new("r8-tope");
    setup_with(
        &f,
        &passes("comando = [\"ping\", \"-n\", \"6\", \"127.0.0.1\"]\nsegundos = 1\n"),
    );
    let data = verify(&f);
    assert_eq!(data["criterios"][0]["ok"], false);
    let reason = data["criterios"][0]["motivo"].as_str().expect("motivo");
    assert!(reason.contains("tope"), "{reason}");
}

#[test]
fn r9_clean_round_accumulates_streak() {
    let f = Fixture::new("r9-limpia");
    let task = f.setup();
    clean_round(&f, 1);
    let state = f.state_json(&task);
    assert_eq!(state["clean_streak"], 1);
    assert_eq!(state["rounds"][0]["clean"], true);
    assert_eq!(f.snapshot()["clean_streak"], 1);
}

/// Informes necesarios para cerrar una ronda sucia por una causa elegida.
fn dirty_reports(f: &Fixture, qa_fails: bool, tried: Option<u32>) {
    f.launch(Role::Developer);
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    f.launch(Role::Qa);
    if qa_fails {
        let (key, report) = f.fish_info("qa-r1");
        fs::write(&report, "falló").expect("informe del QA");
        f.arbiter
            .execute(
                &Caller::Fish(key),
                Command::Report {
                    verdict: Verdict::Fail,
                    file: report,
                    blocking: 1,
                    major: 0,
                    minor: 0,
                    mutations_tried: None,
                    mutations_caught: None,
                },
            )
            .expect("informe del QA");
    } else {
        f.report("qa-r1", Verdict::Pass, None, None)
            .expect("informe del QA");
    }
    f.launch(Role::QaValidator);
    f.report("valqa-r1", Verdict::Pass, tried, tried)
        .expect("informe del validador");
}

fn assert_dirty(f: &Fixture, expected: &str) {
    let outcome = f.close_round().expect("cerrar ronda sucia");
    assert!(outcome.message.contains("sucia"), "{}", outcome.message);
    assert!(outcome.message.contains(expected), "{}", outcome.message);
    assert_eq!(f.snapshot()["clean_streak"], 0);
}

#[test]
fn r9_dirty_round_by_failing_criterion() {
    let f = Fixture::new("r9-criterio");
    setup_with(
        &f,
        "[[criterio]]\nid = \"C9\"\ndescripcion = \"Falla\"\n\
         comando = [\"git\", \"--version\"]\ncodigo = 7\n",
    );
    dirty_reports(&f, false, Some(3));
    assert_dirty(&f, "C9");
}

#[test]
fn r9_dirty_round_by_qa() {
    let f = Fixture::new("r9-qa");
    setup_with(
        &f,
        "[[criterio]]\nid = \"C1\"\ndescripcion = \"Git\"\ncomando = [\"git\", \"--version\"]\n",
    );
    dirty_reports(&f, true, Some(3));
    assert_dirty(&f, "QA");
}

#[test]
fn r9_dirty_round_by_validator() {
    let f = Fixture::new("r9-validador");
    setup_with(
        &f,
        "[[criterio]]\nid = \"C1\"\ndescripcion = \"Git\"\ncomando = [\"git\", \"--version\"]\n",
    );
    dirty_reports(&f, false, Some(2));
    assert_dirty(&f, "validador");
}

#[test]
fn r9_protected_file_touched_by_developer_dirties_the_round() {
    let f = Fixture::new("r9-protegido");
    let task = f.setup();
    f.launch(Role::Developer);
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    f.launch(Role::Qa);
    f.write("protegido.txt", "lo escribió QA\n");
    git(&f.folder, &["add", "protegido.txt"]);
    git(&f.folder, &["commit", "-q", "-m", "archivo del QA"]);
    f.report("qa-r1", Verdict::Pass, None, None)
        .expect("informe del QA");
    f.launch(Role::QaValidator);
    f.report("valqa-r1", Verdict::Pass, Some(3), Some(3))
        .expect("informe del validador");
    f.close_round().expect("ronda 1 limpia");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("ronda 2");
    f.launch(Role::Developer);
    f.write("protegido.txt", "lo sobrescribió el desarrollador\n");
    f.report("dev-r2-1", Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    f.launch(Role::Qa);
    f.report("qa-r2", Verdict::Pass, None, None)
        .expect("informe del QA");
    f.launch(Role::QaValidator);
    f.report("valqa-r2", Verdict::Pass, Some(3), Some(3))
        .expect("informe del validador");
    let outcome = f.close_round().expect("cerrar ronda 2");
    assert!(outcome.message.contains("protegido"), "{}", outcome.message);
    assert_eq!(f.state_json(&task)["clean_streak"], 0);
}

#[test]
fn r9_pauses_at_max_rounds() {
    let f = Fixture::new("r9-tope-rondas");
    let task = setup_with(
        &f,
        "[validacion]\nrondas_maximas = 2\n\
         [[criterio]]\nid = \"C1\"\ndescripcion = \"Git\"\ncomando = [\"git\", \"--version\"]\n",
    );
    clean_round(&f, 1);
    assert_eq!(f.state_json(&task)["status"], "between_rounds");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("ronda 2");
    clean_round(&f, 2);
    let state = f.state_json(&task);
    assert_eq!(state["status"], "paused");
    assert_eq!(state["pause_reason"], "tope_rondas");
}

#[test]
fn r9_pauses_on_repeated_error() {
    let f = Fixture::new("r9-mismo-error");
    let task = setup_with(
        &f,
        "[[criterio]]\nid = \"C1\"\ndescripcion = \"Git\"\n\
         comando = [\"git\", \"--version\"]\ncodigo = 7\n",
    );
    clean_round(&f, 1);
    assert_eq!(f.state_json(&task)["status"], "between_rounds");
    f.arbiter
        .execute(&Fixture::elephant(), Command::RoundStart)
        .expect("ronda 2");
    f.launch(Role::Developer);
    f.report("dev-r2-1", Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    f.launch(Role::Qa);
    f.report("qa-r2", Verdict::Pass, None, None)
        .expect("informe del QA");
    f.launch(Role::QaValidator);
    f.report("valqa-r2", Verdict::Pass, Some(3), Some(3))
        .expect("informe del validador");
    f.close_round().expect("cerrar ronda 2");
    let state = f.state_json(&task);
    assert_eq!(state["status"], "paused");
    assert_eq!(state["pause_reason"], "mismo_error");
}

#[test]
fn r9_pauses_at_time_limit_with_a_fake_clock() {
    let f = Fixture::new("r9-tiempo");
    let task = f.setup();
    f.clock.advance(7300.0);
    clean_round(&f, 1);
    let state = f.state_json(&task);
    assert_eq!(state["status"], "paused");
    assert_eq!(state["pause_reason"], "tope_tiempo");
}

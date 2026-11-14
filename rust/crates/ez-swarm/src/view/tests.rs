//! Tests of the board views.

use crate::db::test_support::{seed, temp_board};

fn setup() -> crate::db::test_support::TempBoard {
    let board = temp_board(0.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    board
}

#[test]
fn board_shows_members_tasks_parts_and_messages() {
    let board = setup();
    let parent = board
        .db
        .add_task("s1", "Portar el núcleo", "dev", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    let part = board
        .db
        .add_task("s1", "Lexer", "dev", "", &[], parent.id)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .add_task("s1", "Parser", "dev", "", &[], parent.id)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .claim("s1", "bo", None, false)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .finish("s1", "bo", part.id, "hecho", true, "")
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .post("s1", "ana", "aviso", "*", "info", "")
        .unwrap_or_else(|error| panic!("{error}"));
    let text =
        super::board_recent(&board.db, "s1", "", 4).unwrap_or_else(|error| panic!("{error}"));
    assert!(text.starts_with("Enjambre s1 · activo · Migrar\nMiembros: ana libre · bo libre"));
    assert!(text.contains("Tareas 1/2:"));
    assert!(text.contains(&format!("T{} · Portar el núcleo [dev] 1/2", parent.id)));
    assert!(text.contains(&format!("  T{} ✓ Lexer @bo → hecho", part.id)));
    assert!(text.contains("Últimos:"));
    assert!(text.contains("#1 ana→todos info: aviso"));
    assert_eq!(
        super::board_recent(&board.db, "s1", "bo", 4)
            .unwrap_or_else(|error| panic!("{error}"))
            .lines()
            .last()
            .map(str::to_string),
        Some("Sin leer para bo: 1".to_string())
    );
    assert_eq!(
        super::board_recent(&board.db, "nope", "", 4)
            .unwrap_err()
            .to_string(),
        "No existe el enjambre 'nope'."
    );
}

#[test]
fn health_reports_empty_done_and_unclaimed() {
    let board = setup();
    let empty = super::health(&board.db, "s1", 0.0).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(empty.state, "empty");
    assert_eq!(empty.spoken(), "El enjambre todavía no tiene tareas.");

    board
        .db
        .add_task("s1", "Portar", "dev", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    let waiting = super::health(&board.db, "s1", 500.0).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(waiting.state, "attention");
    assert_eq!(
        waiting.spoken(),
        "Van 0 de 1 tareas. Atención: T1 lleva 8 min lista y bo no la toma."
    );
    assert_eq!(
        super::summary(&board.db, "s1", 500.0).unwrap_or_else(|error| panic!("{error}")),
        "Van 0 de 1 tareas. Atención: T1 lleva 8 min lista y bo no la toma."
    );

    board
        .db
        .claim("s1", "bo", None, false)
        .unwrap_or_else(|error| panic!("{error}"));
    let working = super::health(&board.db, "s1", 0.0).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(working.working, vec!["bo en T1"]);
    assert_eq!(
        working.spoken(),
        "Van 0 de 1 tareas. Trabajando: bo en T1. Todo en orden."
    );

    board
        .db
        .finish("s1", "bo", 1, "ok", true, "")
        .unwrap_or_else(|error| panic!("{error}"));
    let done = super::health(&board.db, "s1", 0.0).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(done.state, "done");
    assert_eq!(done.spoken(), "El enjambre terminó: 1 de 1 tareas hechas.");
    assert_eq!(
        super::summary(&board.db, "s1", 0.0).unwrap_or_else(|error| panic!("{error}")),
        "El enjambre terminó: 1 de 1 tareas hechas. Desarrollar: ok."
    );
}

#[test]
fn health_detects_failed_dependencies_and_orphan_roles() {
    let board = setup();
    board
        .db
        .add_task("s1", "Lexer", "dev", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .claim("s1", "bo", None, false)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .finish("s1", "bo", 1, "roto", false, "")
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .add_task("s1", "Parser", "dev", "", &[1], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    let text = super::summary(&board.db, "s1", 0.0).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        text,
        "Van 0 de 2 tareas. Atención: T1 falló: roto Y 2 avisos más. Desarrollar: roto."
    );

    let orphan_board = setup();
    orphan_board
        .db
        .add_task("s1", "Desplegar", "ops", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        super::summary(&orphan_board.db, "s1", 0.0).unwrap_or_else(|error| panic!("{error}")),
        "Van 0 de 1 tareas. Atención: T1 es para el rol «ops» y nadie lo tiene."
    );
}

#[test]
fn health_reports_paused_and_unanswered_questions() {
    let board = setup();
    board
        .db
        .add_task("s1", "Portar", "dev", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .set_status("s1", "paused")
        .unwrap_or_else(|error| panic!("{error}"));
    let paused = super::health(&board.db, "s1", 500.0).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(paused.found[0].key, "paused");
    assert_eq!(
        paused.spoken(),
        "Van 0 de 1 tareas. Atención: El enjambre está en pausa. Y 1 aviso más."
    );

    let ask_board = setup();
    ask_board
        .db
        .add_task("s1", "Portar", "dev", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    ask_board
        .db
        .post("s1", "ana", "¿qué API?", "bo", "ask", "")
        .unwrap_or_else(|error| panic!("{error}"));
    let asked = super::health(&ask_board.db, "s1", 700.0).unwrap_or_else(|error| panic!("{error}"));
    assert!(asked
        .found
        .iter()
        .any(|issue| issue.text == "bo no respondió a ana en 12 min."));
}

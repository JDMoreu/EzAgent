//! Tests of the board store.

mod gaps;

use crate::db::test_support::{seed, temp_board, TempBoard};
use crate::models::{Member, Task, BROADCAST, SYSTEM};

fn member(board: &TempBoard, name: &str) -> Member {
    board
        .db
        .read(|db| board.db.require_member(db, "s1", name))
        .unwrap_or_else(|error| panic!("{error}"))
}

fn load_task(board: &TempBoard, id: i64) -> Task {
    board
        .db
        .read(|db| board.db.task_row(db, "s1", id))
        .unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn creates_and_lists_swarms() {
    let board = temp_board(10.0);
    let swarm = board
        .db
        .create("Migrar", "C:/x", "active")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(swarm.len(), 8);
    let info = board
        .db
        .get(&swarm)
        .unwrap_or_else(|error| panic!("{error}"))
        .unwrap_or_else(|| panic!("falta el enjambre"));
    assert_eq!(info.goal, "Migrar");
    assert_eq!(info.cwd, "C:/x");
    assert_eq!(info.created, 10.0);
    assert_eq!(
        board.db.latest().unwrap_or_else(|error| panic!("{error}")),
        swarm
    );
    assert_eq!(
        board
            .db
            .active()
            .unwrap_or_else(|error| panic!("{error}"))
            .len(),
        1
    );
    board
        .db
        .set_status(&swarm, "paused")
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(board
        .db
        .active()
        .unwrap_or_else(|error| panic!("{error}"))
        .is_empty());
    assert_eq!(
        board
            .db
            .live()
            .unwrap_or_else(|error| panic!("{error}"))
            .len(),
        1
    );
    assert_eq!(
        board.db.set_status(&swarm, "raro").unwrap_err().to_string(),
        "Estado de enjambre desconocido: 'raro'."
    );
    assert_eq!(
        board.db.create("", "", "active").unwrap_err().to_string(),
        "Falta el objetivo."
    );
}

#[test]
fn joins_updates_and_validates_members() {
    let board = temp_board(5.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    let member = board
        .db
        .join("s1", "  Cy  ", "QA", "codex", "addr", "opus", "high")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(member.name, "cy");
    assert_eq!(member.role, "qa");
    assert_eq!(member.state, "idle");
    assert_eq!(member.seen, 5.0);
    assert_eq!(
        board
            .db
            .members("s1")
            .unwrap_or_else(|error| panic!("{error}"))
            .len(),
        3
    );
    let again = board
        .db
        .join("s1", "cy", "plan", "", "", "", "")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(again.role, "plan");
    assert_eq!(again.agent, "");
    assert_eq!(
        board
            .db
            .join("s1", "ez", "", "", "", "", "")
            .unwrap_err()
            .to_string(),
        "«ez» es un nombre reservado."
    );
    assert_eq!(
        board
            .db
            .join("s1", "a b", "", "", "", "", "")
            .unwrap_err()
            .to_string(),
        "El nombre 'a b' no es válido: usa a-z, 0-9, «-» o «_»."
    );
    assert!(board.db.set_member_state("s1", "ana", "working").is_ok());
    assert_eq!(
        board
            .db
            .set_member_state("s1", "ana", "raro")
            .unwrap_err()
            .to_string(),
        "Estado de miembro desconocido: 'raro'."
    );
}

#[test]
fn tasks_ready_deps_and_parts() {
    let board = temp_board(0.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    let first = board
        .db
        .add_task("s1", "Lexer", "dev", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    let second = board
        .db
        .add_task("s1", "Probar", "qa", "", &[first.id], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(second.deps, vec![first.id]);
    assert_eq!(
        board
            .db
            .ready("s1", "bo")
            .unwrap_or_else(|error| panic!("{error}"))
            .len(),
        1
    );
    assert!(board
        .db
        .ready("s1", "ana")
        .unwrap_or_else(|error| panic!("{error}"))
        .is_empty());
    assert_eq!(
        board
            .db
            .add_task("s1", "X", "", "", &[9], 0)
            .unwrap_err()
            .to_string(),
        "No existe la tarea T9."
    );
    assert_eq!(
        board
            .db
            .add_task("s1", "", "", "", &[], 0)
            .unwrap_err()
            .to_string(),
        "Falta el título de la tarea."
    );

    let part = board
        .db
        .add_task("s1", "Parte", "", "", &[], first.id)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(part.parent, first.id);
    assert_eq!(part.role, "dev");
    assert_eq!(
        board
            .db
            .add_task("s1", "Nieta", "", "", &[], part.id)
            .unwrap_err()
            .to_string(),
        "Las partes solo tienen un nivel."
    );
}

#[test]
fn claim_finish_and_rollup() {
    let board = temp_board(1.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    let parent = board
        .db
        .add_task("s1", "Madre", "dev", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    let one = board
        .db
        .add_task("s1", "Uno", "", "", &[], parent.id)
        .unwrap_or_else(|error| panic!("{error}"));
    let two = board
        .db
        .add_task("s1", "Dos", "", "", &[], parent.id)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        board
            .db
            .claim("s1", "bo", None, true)
            .unwrap_or_else(|error| panic!("{error}"))
            .len(),
        2
    );
    assert_eq!(member(&board, "bo").state, "working");
    let done = board
        .db
        .finish("s1", "bo", one.id, "ok", true, "")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(done.state, "done");
    board
        .db
        .finish("s1", "bo", two.id, "ok", true, "")
        .unwrap_or_else(|error| panic!("{error}"));
    let rolled = load_task(&board, parent.id);
    assert_eq!(rolled.state, "done");
    assert_eq!(rolled.result, "2 partes hechas");
    let messages = board
        .db
        .history("s1", 0, 10)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        messages.last().map(|message| message.body.as_str()),
        Some("T1 hecha: 2 partes hechas")
    );
}

#[test]
fn finish_retry_and_foreign_task_rules() {
    let board = temp_board(0.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    let task = board
        .db
        .add_task("s1", "Portar", "dev", "bo", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .claim("s1", "bo", None, false)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .finish("s1", "bo", task.id, "ok", true, "")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        board
            .db
            .finish("s1", "bo", task.id, "otra vez", true, "")
            .unwrap_or_else(|error| panic!("{error}"))
            .result,
        "ok"
    );
    let other = board
        .db
        .add_task("s1", "Probar", "qa", "bo", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        board
            .db
            .finish("s1", "ana", other.id, "ok", true, "")
            .unwrap_err()
            .to_string(),
        format!("T{} no está en curso a tu nombre.", other.id)
    );
}

#[test]
fn reopen_counts_rounds_and_resets_dependents() {
    let board = temp_board(0.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    let first = board
        .db
        .add_task("s1", "Portar", "dev", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    let second = board
        .db
        .add_task("s1", "Probar", "qa", "", &[first.id], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .claim("s1", "bo", None, false)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .finish("s1", "bo", first.id, "ok", true, "")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        board
            .db
            .claim("s1", "ana", None, false)
            .unwrap_or_else(|error| panic!("{error}"))
            .len(),
        0
    );
    let (task, exhausted) = board
        .db
        .reopen("s1", first.id, "ana", "falta", "")
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!exhausted);
    assert_eq!(task.state, "pending");
    assert_eq!(task.result, "falta");
    assert_eq!(task.rounds, 1);
    assert_eq!(load_task(&board, second.id).state, "pending");
    let messages = board
        .db
        .history("s1", 0, 10)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(messages[1].kind, "fix");
    assert_eq!(messages[1].recipient, "bo");
    assert_eq!(messages[1].body, "T1 reabierta (1/3): falta");
    assert_eq!(
        board
            .db
            .reopen("s1", first.id, "ana", "", "")
            .unwrap_err()
            .to_string(),
        "T1 no está terminada; escríbele a quien la lleva."
    );
}

#[test]
fn release_tasks_returns_work_to_the_role() {
    let board = temp_board(0.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    let task = board
        .db
        .add_task("s1", "Portar", "dev", "bo", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .claim("s1", "bo", None, false)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        board
            .db
            .release_tasks("s1", "bo")
            .unwrap_or_else(|error| panic!("{error}")),
        vec![task.id]
    );
    let released = load_task(&board, task.id);
    assert_eq!(released.state, "pending");
    assert_eq!(released.owner, "");
}

#[test]
fn pending_ignores_own_messages_and_system_kind() {
    let board = temp_board(0.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .post("s1", "ana", "para bo", "bo", "info", "")
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .post("s1", "ana", "aviso", BROADCAST, "sys", "")
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .post("s1", "bo", "propio", BROADCAST, "info", "")
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .add_task("s1", "Portar", "dev", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        board
            .db
            .pending("s1", "bo")
            .unwrap_or_else(|error| panic!("{error}")),
        (1, 1, vec![1])
    );
    assert_eq!(
        board
            .db
            .pending("s1", "ana")
            .unwrap_or_else(|error| panic!("{error}")),
        (0, 0, vec![])
    );
}

#[test]
fn tokens_briefing_and_address() {
    let board = temp_board(0.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .set_token("s1", "bo", "abc")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        board
            .db
            .by_token("abc")
            .unwrap_or_else(|error| panic!("{error}"))
            .map(|(swarm, member)| (swarm, member.name)),
        Some(("s1".to_string(), "bo".to_string()))
    );
    assert_eq!(
        board
            .db
            .by_token("")
            .unwrap_or_else(|error| panic!("{error}")),
        None
    );
    assert!(board
        .db
        .mark_briefed("s1", "bo")
        .unwrap_or_else(|error| panic!("{error}")));
    assert!(!board
        .db
        .mark_briefed("s1", "bo")
        .unwrap_or_else(|error| panic!("{error}")));
    board
        .db
        .set_token("s1", "bo", "def")
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(board
        .db
        .mark_briefed("s1", "bo")
        .unwrap_or_else(|error| panic!("{error}")));
    board
        .db
        .set_address("s1", "bo", &"x".repeat(200))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(member(&board, "bo").address.chars().count(), 96);
}

#[test]
fn finish_by_system_does_not_need_the_owner() {
    let board = temp_board(0.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    let task = board
        .db
        .add_task("s1", "Portar", "dev", "bo", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    let done = board
        .db
        .finish("s1", SYSTEM, task.id, "cerrada", true, "")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(done.state, "done");
    assert_eq!(
        board
            .db
            .finish("s1", "bo", 99, "x", true, "")
            .unwrap_err()
            .to_string(),
        "No existe la tarea T99."
    );
}

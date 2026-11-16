//! Store behaviours from `test_swarm_store.py` that the board fixtures did not
//! exercise (TMAP-1): concurrent claim, silent part rollup and reopen with parts.

use std::sync::{Arc, Mutex};

use crate::db::test_support::{seed, temp_board};
use crate::db::Db;

fn qa(db: &Db) {
    db.join("s1", "qa", "qa", "codex", "", "", "")
        .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn concurrent_claims_hand_out_each_task_once() {
    let board = temp_board(100.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    let db = Arc::new(
        Db::new(board.path.clone(), Arc::new(|| 100.0)).unwrap_or_else(|error| panic!("{error}")),
    );
    for index in 0..6 {
        db.add_task("s1", &format!("Parte {index}"), "", "", &[], 0)
            .unwrap_or_else(|error| panic!("{error}"));
    }
    for name in ["w1", "w2", "w3"] {
        db.join("s1", name, "", "", "", "", "")
            .unwrap_or_else(|error| panic!("{error}"));
    }
    let taken: Arc<Mutex<Vec<i64>>> = Arc::default();
    let workers: Vec<_> = ["w1", "w2", "w3"]
        .into_iter()
        .map(|name| {
            let db = Arc::clone(&db);
            let taken = Arc::clone(&taken);
            std::thread::spawn(move || {
                while let Ok(found) = db.claim("s1", name, None, false) {
                    let Some(task) = found.first() else { break };
                    taken.lock().expect("ids").push(task.id);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("hilo");
    }
    let mut got = taken.lock().expect("ids").clone();
    got.sort_unstable();
    let all: Vec<i64> = db
        .tasks("s1")
        .unwrap_or_else(|error| panic!("{error}"))
        .iter()
        .map(|task| task.id)
        .collect();
    assert_eq!(got, all);
}

#[test]
fn parts_close_silently_and_release_the_leases() {
    let board = temp_board(100.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    qa(&board.db);
    let build = board
        .db
        .add_task("s1", "Desarrollar", "dev", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    let first = board
        .db
        .add_task("s1", "API", "", "", &[], build.id)
        .unwrap_or_else(|error| panic!("{error}"));
    let second = board
        .db
        .add_task("s1", "Pruebas", "", "", &[], build.id)
        .unwrap_or_else(|error| panic!("{error}"));
    let claimed = board
        .db
        .claim("s1", "bo", None, true)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(claimed.len(), 2);
    let paths = vec!["src/api".to_string()];
    board
        .db
        .reserve("s1", "bo", &paths, 600.0)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .finish("s1", "bo", first.id, "ok", true, "")
        .unwrap_or_else(|error| panic!("{error}"));
    let inbox = board
        .db
        .inbox("s1", "qa", true, 20)
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(inbox.is_empty());
    assert_eq!(
        board
            .db
            .leases("s1")
            .unwrap_or_else(|error| panic!("{error}"))
            .len(),
        1
    );
    board
        .db
        .finish("s1", "bo", second.id, "ok", true, "")
        .unwrap_or_else(|error| panic!("{error}"));
    let notice = board
        .db
        .inbox("s1", "qa", true, 20)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        notice.iter().map(|m| m.body.as_str()).collect::<Vec<_>>(),
        vec![format!("T{} hecha: 2 partes hechas", build.id)]
    );
    assert!(board
        .db
        .inbox("s1", "bo", true, 20)
        .unwrap_or_else(|error| panic!("{error}"))
        .is_empty());
    assert!(board
        .db
        .leases("s1")
        .unwrap_or_else(|error| panic!("{error}"))
        .is_empty());
}

#[test]
fn reopening_a_task_with_parts_creates_one_correction() {
    let board = temp_board(100.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    qa(&board.db);
    let build = board
        .db
        .add_task("s1", "Desarrollar", "dev", "", &[], 0)
        .unwrap_or_else(|error| panic!("{error}"));
    let part = board
        .db
        .add_task("s1", "API", "", "", &[], build.id)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .claim("s1", "bo", None, false)
        .unwrap_or_else(|error| panic!("{error}"));
    board
        .db
        .finish("s1", "bo", part.id, "ok", true, "")
        .unwrap_or_else(|error| panic!("{error}"));
    let (reopened, exhausted) = board
        .db
        .reopen("s1", build.id, "qa", "Sin pruebas", "")
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!exhausted && reopened.state == "pending");
    let tasks = board
        .db
        .tasks("s1")
        .unwrap_or_else(|error| panic!("{error}"));
    let extra: Vec<_> = tasks
        .iter()
        .filter(|task| task.parent == build.id && task.id != part.id)
        .collect();
    assert_eq!(extra.len(), 1);
    assert_eq!(extra[0].title, "Corregir: Sin pruebas");
    assert_eq!(extra[0].role, "dev");
    assert_eq!(
        board
            .db
            .claim("s1", "bo", None, false)
            .unwrap_or_else(|error| panic!("{error}"))
            .iter()
            .map(|task| task.id)
            .collect::<Vec<_>>(),
        vec![extra[0].id]
    );
}

#[test]
fn reopening_the_board_keeps_its_members() {
    let board = temp_board(100.0);
    seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
    let reopened =
        Db::new(board.path.clone(), Arc::new(|| 100.0)).unwrap_or_else(|error| panic!("{error}"));
    let names: Vec<String> = reopened
        .members("s1")
        .unwrap_or_else(|error| panic!("{error}"))
        .into_iter()
        .map(|member| member.name)
        .collect();
    assert_eq!(names, vec!["ana".to_string(), "bo".to_string()]);
}

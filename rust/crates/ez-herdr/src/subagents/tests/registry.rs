//! Pruebas de archivos: creación, transiciones, candados y concurrencia.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{change, rec, TempRoot};
use crate::subagents::{Registry, RegistryError, TaskState, Validation};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("reloj del sistema")
        .as_millis() as u64
}

#[test]
fn create_rejects_duplicates_bad_state_and_cleans_lock() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    let record = rec("dev1-opencode", "t6h", "t12", TaskState::InProgress);

    registry.create(&record).expect("crear registro");
    assert!(matches!(
        registry.create(&record),
        Err(RegistryError::Exists(_))
    ));
    let lock = tmp.path().join("t6h").join("dev1-opencode.lock");
    assert!(!lock.exists(), "el candado se limpia aunque falle create");

    let done = rec("qa1-opencode", "t6h", "t13", TaskState::Done);
    assert!(matches!(
        registry.create(&done),
        Err(RegistryError::Invalid(_))
    ));
}

#[test]
fn list_and_find_by_terminal_across_two_parents() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    registry
        .create(&rec("dev1-opencode", "t6h", "t11", TaskState::InProgress))
        .expect("crear en t6h");
    registry
        .create(&rec("qa1-opencode", "t6h", "t12", TaskState::InProgress))
        .expect("crear en t6h");
    registry
        .create(&rec(
            "dev1-opencode",
            "maquina/t7",
            "t13",
            TaskState::InProgress,
        ))
        .expect("crear en maquina/t7");

    assert_eq!(registry.list("t6h").expect("listar t6h").len(), 2);
    let nested = registry.list("maquina/t7").expect("listar maquina/t7");
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].name, "dev1-opencode");
    assert_eq!(nested[0].terminal, "t13");

    let found = registry
        .find_by_terminal("t13")
        .expect("buscar terminal")
        .expect("terminal encontrada");
    assert_eq!(found.parent, "maquina/t7");
    assert_eq!(registry.find_by_terminal("t99").expect("buscar"), None);

    assert_eq!(registry.root(), tmp.path());
    assert!(registry.list("sin-padre").expect("listar vacío").is_empty());
    let one = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(one.terminal, "t11");
    assert!(registry.get("t6h", "sin-registro").expect("leer").is_none());
}

#[test]
fn invalid_parent_folder_and_name_are_rejected() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    let long = "x".repeat(33);
    for parent in ["", "..", "../../etc", "bad\\path", "con espacio", "t6h;rm"] {
        assert!(
            matches!(registry.list(parent), Err(RegistryError::Invalid(_))),
            "padre {parent} debió ser rechazado"
        );
    }
    assert!(registry.list("maquina/t7").is_ok());
    assert!(registry.list("a.b-c_d").is_ok());

    for name in ["", "t7", "T7", "Bad Name", "name/other", long.as_str()] {
        assert!(
            matches!(registry.get("t6h", name), Err(RegistryError::Invalid(_))),
            "nombre {name} debió ser rechazado"
        );
    }
    assert!(registry.get("t6h", "a").is_ok());
    assert!(registry.get("t6h", "dev1-opencode").is_ok());
}

#[test]
fn transition_applies_edit_history_and_persists() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    let mut record = rec("dev1-opencode", "t6h", "t12", TaskState::InProgress);
    record
        .history
        .push(change(TaskState::InProgress, 1, "t6h", "encargo entregado"));
    registry.create(&record).expect("crear");

    let done = registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::Done,
            "dev1-opencode",
            "terminé",
            |record| {
                record.summary = Some("todo listo".to_string());
                record.report = Some("informe.md".to_string());
            },
        )
        .expect("pasar a terminado");
    assert_eq!(done.state, TaskState::Done);
    assert_eq!(done.summary.as_deref(), Some("todo listo"));
    assert_eq!(done.report.as_deref(), Some("informe.md"));
    assert_eq!(done.since_ms, done.history[1].at_ms);
    assert_eq!(done.history.len(), 2);
    assert_eq!(done.history[0].detail, "encargo entregado");
    assert_eq!(done.history[1].state, TaskState::Done);
    assert_eq!(done.history[1].by, "dev1-opencode");

    let stored = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(stored, done);
}

#[test]
fn validated_transition_is_final() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    registry
        .create(&rec("dev1-opencode", "t6h", "t12", TaskState::InProgress))
        .expect("crear");
    registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::Done,
            "dev1-opencode",
            "fin",
            |_| {},
        )
        .expect("pasar a terminado");

    let validated = registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::Validated,
            "t7",
            "validado",
            |record| {
                record.validation = Some(Validation {
                    by: "t7".to_string(),
                    verdict: true,
                    summary: "revisado".to_string(),
                    file: Some("evidencia.txt".to_string()),
                    at_ms: 2,
                });
            },
        )
        .expect("pasar a validado");
    assert_eq!(validated.state, TaskState::Validated);
    assert!(validated.validation.is_some());
    assert_eq!(validated.history.len(), 2);
    assert!(matches!(
        registry.transition("t6h", "dev1-opencode", TaskState::Failed, "t7", "x", |_| {}),
        Err(RegistryError::Invalid(_))
    ));
}

#[test]
fn allowed_transitions_to_failed() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());

    registry
        .create(&rec("dev1-opencode", "t6h", "t11", TaskState::InProgress))
        .expect("crear");
    let failed = registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::Failed,
            "t6h",
            "no entregué",
            |_| {},
        )
        .expect("pasar a fallido");
    assert_eq!(failed.state, TaskState::Failed);
    let error = registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::InProgress,
            "t6h",
            "x",
            |_| {},
        )
        .expect_err("fallido es final");
    assert_eq!(
        error.to_string(),
        "dev1-opencode está «fallido»; no puede pasar a «en_proceso»."
    );

    registry
        .create(&rec("qa1-opencode", "t6h", "t12", TaskState::InProgress))
        .expect("crear");
    registry
        .transition(
            "t6h",
            "qa1-opencode",
            TaskState::Done,
            "qa1-opencode",
            "fin",
            |_| {},
        )
        .expect("pasar a terminado");
    let failed_validation = registry
        .transition(
            "t6h",
            "qa1-opencode",
            TaskState::Failed,
            "t7",
            "no pasó",
            |_| {},
        )
        .expect("validación fallida");
    assert_eq!(failed_validation.state, TaskState::Failed);
}

#[test]
fn forbidden_transitions_and_missing_record() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    registry
        .create(&rec("plan1-opencode", "t6h", "t13", TaskState::InProgress))
        .expect("crear");
    assert!(matches!(
        registry.transition(
            "t6h",
            "plan1-opencode",
            TaskState::InProgress,
            "x",
            "x",
            |_| {}
        ),
        Err(RegistryError::Invalid(_))
    ));
    assert!(matches!(
        registry.transition(
            "t6h",
            "plan1-opencode",
            TaskState::Validated,
            "x",
            "x",
            |_| {}
        ),
        Err(RegistryError::Invalid(_))
    ));
    assert!(matches!(
        registry.transition("t6h", "sin-registro", TaskState::Done, "x", "x", |_| {}),
        Err(RegistryError::NotFound(_))
    ));
}

#[test]
fn mark_notified_records_last_announced_state() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    registry
        .create(&rec("dev1-opencode", "t6h", "t12", TaskState::InProgress))
        .expect("crear");
    registry
        .mark_notified("t6h", "dev1-opencode", TaskState::InProgress)
        .expect("marcar aviso");
    let stored = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(stored.notified, Some(TaskState::InProgress));
}

#[test]
fn concurrent_transitions_never_lose_history() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    let mut record = rec("dev1-opencode", "t6h", "t12", TaskState::InProgress);
    record
        .history
        .push(change(TaskState::InProgress, 1, "t6h", "encargo entregado"));
    registry.create(&record).expect("crear");

    let barrier = Arc::new(Barrier::new(2));
    let launch = |who: &'static str| {
        let worker = Registry::at(tmp.path());
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            worker.transition(
                "t6h",
                "dev1-opencode",
                TaskState::Done,
                who,
                "terminé",
                |_| {},
            )
        })
    };
    let first = launch("dev1-opencode");
    let second = launch("qa1-opencode");
    let first = first.join().expect("hilo 1");
    let second = second.join().expect("hilo 2");
    assert_eq!(
        [first.is_ok(), second.is_ok()]
            .iter()
            .filter(|won| **won)
            .count(),
        1,
        "solo una transición puede ganar"
    );
    for result in [&first, &second] {
        if let Err(error) = result {
            assert!(
                matches!(error, RegistryError::Invalid(_) | RegistryError::Busy(_)),
                "el perdedor vio: {error}"
            );
        }
    }

    let stored = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(stored.state, TaskState::Done);
    assert_eq!(stored.history.len(), 2, "no se perdió historial");
    assert_eq!(stored.history[0].detail, "encargo entregado");
    assert_eq!(stored.history[1].state, TaskState::Done);
}

#[test]
fn atomic_write_is_visible_whole_to_concurrent_readers() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    let mut record = rec("qa1-opencode", "t6h", "t12", TaskState::InProgress);
    record.summary = Some("x".repeat(20_000));
    registry.create(&record).expect("crear");

    let stop = Arc::new(AtomicBool::new(false));
    let writer_stop = Arc::clone(&stop);
    let writer = thread::spawn(move || {
        let worker = Registry::at(tmp.path());
        let mut written = 0u64;
        let mut turn = 0u64;
        while !writer_stop.load(Ordering::Relaxed) {
            let notified = if turn.is_multiple_of(2) {
                TaskState::Done
            } else {
                TaskState::Failed
            };
            if worker
                .mark_notified("t6h", "qa1-opencode", notified)
                .is_ok()
            {
                written += 1;
            }
            turn += 1;
        }
        written
    });

    for _ in 0..400 {
        match registry.get("t6h", "qa1-opencode") {
            Ok(Some(stored)) => {
                assert_eq!(stored.name, "qa1-opencode");
                assert_eq!(stored.summary.as_deref().expect("resumen").len(), 20_000);
            }
            other => panic!("lectura a medias: {other:?}"),
        }
    }
    stop.store(true, Ordering::Relaxed);
    assert!(
        writer.join().expect("hilo escritor") > 0,
        "no se escribió nada"
    );
}

#[test]
fn abandoned_lock_is_replaced_and_cleaned() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    registry
        .create(&rec("dev1-opencode", "t6h", "t12", TaskState::InProgress))
        .expect("crear");

    let lock = tmp.path().join("t6h").join("dev1-opencode.lock");
    let old = now_ms() - 120_000;
    std::fs::write(&lock, format!("{old}:0")).expect("dejar candado abandonado");

    let done = registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::Done,
            "dev1-opencode",
            "fin",
            |_| {},
        )
        .expect("el candado viejo se reemplaza");
    assert_eq!(done.state, TaskState::Done);
    assert!(!lock.exists(), "se limpia al terminar");
}

#[test]
fn fresh_lock_reports_busy() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    registry
        .create(&rec("dev1-opencode", "t6h", "t12", TaskState::InProgress))
        .expect("crear");

    let lock = tmp.path().join("t6h").join("dev1-opencode.lock");
    std::fs::write(&lock, format!("{}:0", now_ms())).expect("dejar candado fresco");

    assert!(matches!(
        registry.transition("t6h", "dev1-opencode", TaskState::Done, "x", "fin", |_| {}),
        Err(RegistryError::Busy(_))
    ));
}

const ALL_STATES: [TaskState; 5] = [
    TaskState::InProgress,
    TaskState::Blocked,
    TaskState::Done,
    TaskState::Failed,
    TaskState::Validated,
];

/// Tabla de transiciones permitidas por el diseño (§13 y novedades §2).
fn spec_allows(from: TaskState, to: TaskState) -> bool {
    matches!(
        (from, to),
        (TaskState::InProgress, TaskState::Blocked)
            | (TaskState::Blocked, TaskState::InProgress)
            | (TaskState::Blocked, TaskState::Done)
            | (TaskState::Blocked, TaskState::Failed)
            | (TaskState::InProgress, TaskState::Done)
            | (TaskState::InProgress, TaskState::Failed)
            | (TaskState::Done, TaskState::Validated)
            | (TaskState::Done, TaskState::Failed)
    )
}

/// Deja un registro en cualquier estado para la prueba (`create` admite
/// `en_proceso` y `fallido`; los otros se alcanzan por transiciones válidas).
fn seed(registry: &Registry, name: &str, parent: &str, state: TaskState) {
    match state {
        TaskState::InProgress | TaskState::Failed => {
            registry
                .create(&rec(name, parent, "t12", state))
                .expect("crear en estado inicial");
        }
        TaskState::Blocked => {
            registry
                .create(&rec(name, parent, "t12", TaskState::InProgress))
                .expect("crear");
            registry
                .transition(
                    parent,
                    name,
                    TaskState::Blocked,
                    "vigilante",
                    "prueba",
                    |_| {},
                )
                .expect("pasar a bloqueado");
        }
        TaskState::Done => {
            registry
                .create(&rec(name, parent, "t12", TaskState::InProgress))
                .expect("crear");
            registry
                .transition(parent, name, TaskState::Done, name, "fin", |_| {})
                .expect("pasar a terminado");
        }
        TaskState::Validated => {
            registry
                .create(&rec(name, parent, "t12", TaskState::InProgress))
                .expect("crear");
            registry
                .transition(parent, name, TaskState::Done, name, "fin", |_| {})
                .expect("pasar a terminado");
            registry
                .transition(parent, name, TaskState::Validated, "t7", "validado", |_| {})
                .expect("pasar a validado");
        }
    }
}

#[test]
fn transition_table_all_twenty_five_pairs() {
    for from in ALL_STATES {
        for to in ALL_STATES {
            let tmp = TempRoot::new();
            let registry = Registry::at(tmp.path());
            let name = "dev1-opencode";
            seed(&registry, name, "t6h", from);

            let path = tmp.path().join("t6h").join(format!("{name}.json"));
            let before = std::fs::read_to_string(&path).expect("leer antes");
            let result = registry.transition("t6h", name, to, "t7", "intento", |_| {});

            if spec_allows(from, to) {
                assert_eq!(result.expect("transición permitida").state, to);
            } else {
                let error = result.expect_err("transición prohibida");
                assert_eq!(
                    error.to_string(),
                    format!(
                        "{name} está «{}»; no puede pasar a «{}».",
                        from.as_str(),
                        to.as_str()
                    )
                );
                assert_eq!(
                    std::fs::read_to_string(&path).expect("leer después"),
                    before,
                    "el JSON no debe cambiar en {from} → {to}"
                );
            }
        }
    }
}

#[test]
fn create_rejects_terminal_initial_states() {
    let tmp = TempRoot::new();
    let registry = Registry::at(tmp.path());
    for (state, label, name) in [
        (TaskState::Done, "terminado", "dev1-opencode"),
        (TaskState::Validated, "validado", "qa1-opencode"),
    ] {
        let error = registry
            .create(&rec(name, "t6h", "t12", state))
            .expect_err("create debe rechazar un estado final");
        assert!(matches!(error, RegistryError::Invalid(_)));
        assert_eq!(
            error.to_string(),
            format!("{name} no puede crearse en estado «{label}»; solo «en_proceso» o «fallido».")
        );
        assert!(
            !tmp.path().join("t6h").join(format!("{name}.json")).exists(),
            "no debe crearse el registro en {label}"
        );
    }
}

//! Tests of the swarm service.

use std::sync::{Arc, Mutex};

use crate::db::test_support::{temp_board, TempBoard};
use crate::db::Db;

use super::*;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn swarm_service() -> (SwarmService, TempBoard, String, Vec<Seat>) {
    let board = temp_board(0.0);
    let db =
        Db::new(board.path.clone(), Arc::new(|| 0.0)).unwrap_or_else(|error| panic!("{error}"));
    let service = SwarmService::new(db, "ezs", Arc::new(|| 0.0));
    let members = [
        MemberSpec::new("ana", "plan", "claude"),
        MemberSpec::new("bo", "dev", "opencode"),
        MemberSpec::new("cy", "qa", "codex"),
    ];
    let tasks = [
        ("Portar".to_string(), "dev".to_string()),
        ("Probar".to_string(), "qa".to_string()),
    ];
    let (swarm, seats) = service
        .start("Migrar", &members, "", Some(&tasks))
        .unwrap_or_else(|error| panic!("{error}"));
    (service, board, swarm, seats)
}

fn token(seats: &[Seat], name: &str) -> String {
    seats.iter().find(|seat| seat.name == name).map_or_else(
        || panic!("falta el asiento de {name}"),
        |seat| seat.token.clone(),
    )
}

#[test]
fn service_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SwarmService>();
}

#[test]
fn start_creates_the_swarm_chain_and_seats() {
    let (service, board, swarm, seats) = swarm_service();
    assert_eq!(seats.len(), 3);
    assert_eq!(seats[0].token.len(), 43);
    assert!(seats[1].briefing.starts_with("Eres «bo»"));
    let tasks = service
        .store
        .tasks(&swarm)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].title, "Portar");
    assert_eq!(tasks[1].deps, vec![tasks[0].id]);
    assert_eq!(
        board
            .db
            .get(&swarm)
            .unwrap_or_else(|error| panic!("{error}"))
            .unwrap_or_else(|| panic!("falta el enjambre"))
            .status,
        "active"
    );
    assert_eq!(
        service.latest().unwrap_or_else(|error| panic!("{error}")),
        swarm
    );
    assert_eq!(
        service.start("X", &[], "", None).unwrap_err().to_string(),
        "Un enjambre necesita al menos un miembro."
    );
}

#[test]
fn next_claims_a_task_and_done_returns_the_next_state() {
    let (service, _board, swarm, seats) = swarm_service();
    let bo = token(&seats, "bo");
    let unknown = service
        .handle("badtoken", &args(&["next"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!unknown.ok);
    assert_eq!(unknown.text, protocol::NO_SEAT);
    let command = service
        .handle(&bo, &args(&["nope"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(command.text, "Comando desconocido: nope. Mira: ezs help");

    let first = service
        .handle(&bo, &args(&["next"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(first.ok);
    assert!(first.text.contains("Eres «bo» (rol: desarrollar)"));
    assert!(first
        .text
        .ends_with("Tu tarea: T1 Portar · después sigue validar con T2 (no la hagas tú)"));
    let again = service
        .handle(&bo, &args(&["next"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        again.text,
        "En curso: T1 Portar · después sigue validar con T2 (no la hagas tú)"
    );

    let missing = service
        .handle(&bo, &args(&["done"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        missing.text,
        "Falta el resumen: ezs done \"qué hiciste en una línea\""
    );
    let done = service
        .handle(&bo, &args(&["done", "listo en una línea"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(done.ok);
    assert_eq!(
        done.text,
        "T1 hecha.\nSin tareas listas para ti. Termina tu turno; EzAgent te escribe cuando haya algo."
    );
    let messages = service
        .store
        .history(&swarm, 0, 10)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(messages[0].body, "T1 hecha: listo en una línea");
}

#[test]
fn say_and_inbox_follow_the_python_texts() {
    let (service, _board, _swarm, seats) = swarm_service();
    let bo = token(&seats, "bo");
    let cy = token(&seats, "cy");
    let said = service
        .handle(&bo, &args(&["say", "all", "empezamos"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(said.text, "#1 enviado a todos.");
    let inbox = service
        .handle(&cy, &args(&["inbox"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(inbox.text, "#1 bo→todos info: empezamos");
    assert_eq!(
        service
            .handle(&cy, &args(&["inbox"]))
            .unwrap_or_else(|error| panic!("{error}"))
            .text,
        "Sin mensajes nuevos."
    );
    assert_eq!(
        service
            .handle(&bo, &args(&["say"]))
            .unwrap_or_else(|error| panic!("{error}"))
            .text,
        "Uso: ezs say <nombre|all> \"texto\""
    );
}

#[test]
fn locks_and_add_follow_the_python_texts() {
    let (service, _board, _swarm, seats) = swarm_service();
    let bo = token(&seats, "bo");
    let cy = token(&seats, "cy");
    let locked = service
        .handle(&bo, &args(&["lock", "src/a.rs", "src/b.rs"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(locked.text, "Reservado: src/a.rs, src/b.rs.");
    let conflict = service
        .handle(&cy, &args(&["lock", "src/a.rs"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        conflict.text,
        "Ocupado: src/a.rs lo tiene bo. Elige otros archivos o coordínalo con say."
    );
    let unlocked = service
        .handle(&bo, &args(&["unlock", "src/a.rs"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(unlocked.text, "Reservas liberadas: 1.");

    let added = service
        .handle(&bo, &args(&["add", "Publicar", "--after", "T1,T2"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(added.text, "Creada: T3 · Publicar tras T1,T2");
    assert_eq!(
        service
            .handle(&bo, &args(&["add", "X", "--role", "ROL!"]))
            .unwrap_or_else(|error| panic!("{error}"))
            .text,
        "El rol 'rol!' no es válido: usa a-z, 0-9, «-» o «_»."
    );
    assert_eq!(
        service
            .handle(&bo, &args(&["say", "all", "x", "--zzz"]))
            .unwrap_or_else(|error| panic!("{error}"))
            .text,
        "Opción desconocida: --zzz."
    );
}

#[test]
fn fix_reopens_and_reports_the_round() {
    let (service, _board, _swarm, seats) = swarm_service();
    let bo = token(&seats, "bo");
    let cy = token(&seats, "cy");
    service
        .handle(&bo, &args(&["next"]))
        .unwrap_or_else(|error| panic!("{error}"));
    service
        .handle(&bo, &args(&["done", "ok"]))
        .unwrap_or_else(|error| panic!("{error}"));
    let fixed = service
        .handle(&cy, &args(&["fix", "T1", "falta la prueba"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(fixed.text, "T1 reabierta (ronda 1/3).");
    assert_eq!(
        service
            .handle(&cy, &args(&["fix"]))
            .unwrap_or_else(|error| panic!("{error}"))
            .text,
        "Uso: ezs fix <T> \"qué falta\" [--ref informe]"
    );
}

#[test]
fn pause_resume_stop_and_wait_short_circuit() {
    let (service, _board, swarm, seats) = swarm_service();
    let bo = token(&seats, "bo");
    service
        .pause(&swarm)
        .unwrap_or_else(|error| panic!("{error}"));
    let paused = service
        .handle(&bo, &args(&["next"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(paused.ok);
    assert_eq!(paused.text, protocol::PAUSED);
    service
        .resume(&swarm)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        service.resume(&swarm).unwrap_err().to_string(),
        "Ese enjambre no está en pausa."
    );
    service
        .stop(&swarm)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        service
            .handle(&bo, &args(&["next"]))
            .unwrap_or_else(|error| panic!("{error}"))
            .text,
        protocol::STOPPED
    );

    let (service, _board, _swarm, seats) = swarm_service();
    let bo = token(&seats, "bo");
    service.close();
    let woken = service
        .handle(&bo, &args(&["wait", "5"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(woken.text.starts_with("Eres «bo»"));
    service.open();
}

#[test]
fn listeners_and_activity_are_isolated_from_panics() {
    let (service, _board, swarm, seats) = swarm_service();
    let bo = token(&seats, "bo");
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    service.subscribe(move |swarm, member, op| {
        sink.lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(format!("{swarm}:{member}:{op}"));
    });
    let activity = Arc::new(Mutex::new(0usize));
    let counter = Arc::clone(&activity);
    service.on_activity(move |_swarm, _member| {
        *counter.lock().unwrap_or_else(|error| error.into_inner()) += 1;
    });
    service.subscribe(|_swarm, _member, _op| panic!("oyente roto"));
    service
        .handle(&bo, &args(&["next"]))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        *activity.lock().unwrap_or_else(|error| error.into_inner()),
        1
    );
    assert_eq!(
        *events.lock().unwrap_or_else(|error| error.into_inner()),
        vec![format!("{swarm}:bo:next")]
    );
}

#[test]
fn stages_share_a_stage_only_when_parallel_and_without_task() {
    let specs = [
        MemberSpec::new("ana", "plan", "claude"),
        MemberSpec {
            task: "Backend".to_string(),
            ..MemberSpec::new("bo", "dev", "opencode")
        },
        MemberSpec {
            parallel: true,
            ..MemberSpec::new("cy", "dev", "codex")
        },
    ];
    let stages = stages_for(&specs);
    assert_eq!(stages.len(), 2);
    assert_eq!(
        stages[0],
        vec![(
            protocol::role_task("plan"),
            "plan".to_string(),
            String::new()
        )]
    );
    assert_eq!(stages[1].len(), 2);
    assert_eq!(
        stages[1][0],
        ("Backend".to_string(), "dev".to_string(), "bo".to_string())
    );
    assert_eq!(stages[1][1].1, "dev");
    assert_eq!(stages[1][1].2, "");
}

#[test]
fn wait_wakes_up_when_a_message_arrives() {
    let (service, _board, swarm, seats) = swarm_service();
    let bo = token(&seats, "bo");
    service
        .handle(&bo, &args(&["next"]))
        .unwrap_or_else(|error| panic!("{error}"));
    let service = Arc::new(service);
    let sender = Arc::clone(&service);
    let target = swarm.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        sender
            .tell(&target, "hola", "bo")
            .unwrap_or_else(|error| panic!("{error}"));
    });
    let reply = service
        .handle(&bo, &args(&["wait", "2"]))
        .unwrap_or_else(|error| panic!("{error}"));
    thread.join().unwrap_or_else(|error| panic!("{error:?}"));
    assert!(reply.text.contains("Mensajes (1):"), "{}", reply.text);
    assert!(reply.text.contains("#1 ez→bo info: hola"), "{}", reply.text);
}

#[test]
fn options_and_tokens_match_the_python_contract() {
    let (service, _board, swarm, _seats) = swarm_service();
    assert_eq!(
        parse(&args(&["--ref", "a.md", "--all", "x", "--ref=b", "--"]))
            .unwrap_or_else(|error| panic!("{error}")),
        (
            vec!["x".to_string(), "--".to_string()],
            BTreeMap::from([
                ("all".to_string(), "1".to_string()),
                ("ref".to_string(), "b".to_string())
            ])
        )
    );
    assert_eq!(
        parse(&args(&["--to"]))
            .unwrap_or_else(|error| panic!("{error}"))
            .1,
        BTreeMap::from([("to".to_string(), String::new())])
    );
    assert_eq!(
        parse(&args(&["--zzz"])).unwrap_err().to_string(),
        "Opción desconocida: --zzz."
    );
    let first = service
        .issue_token(&swarm, "bo")
        .unwrap_or_else(|error| panic!("{error}"));
    let second = service
        .issue_token(&swarm, "bo")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(first.len(), 43);
    assert_ne!(first, second);
    assert!(service
        .authenticate(&first)
        .unwrap_or_else(|error| panic!("{error}"))
        .is_none());
    assert_eq!(
        service
            .authenticate(&second)
            .unwrap_or_else(|error| panic!("{error}"))
            .map(|(_, member)| member.name),
        Some("bo".to_string())
    );
    assert!(service
        .authenticate("")
        .unwrap_or_else(|error| panic!("{error}"))
        .is_none());
}

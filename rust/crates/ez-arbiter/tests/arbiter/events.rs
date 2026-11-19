//! R12: traza, eventos del elefante, suscriptores y recarga.

use std::sync::Arc;
use std::time::Duration;

use ez_arbiter::{Arbiter, Command, Verdict};
use ez_team::Role;

use super::support::{kind, live_kind, FakeWriter, Fixture};

fn next(f: &Fixture) -> String {
    f.arbiter
        .wait_for_elephant("s1", Duration::from_millis(100))
        .expect("evento para el elefante")
        .kind
}

#[test]
fn r12_bitacora_has_one_event_per_action_and_rejections() {
    let f = Fixture::new("r12-bitacora");
    let task = f.setup();
    let events = f.bitacora(&task);
    let kinds: Vec<&str> = events.iter().map(kind).collect();
    assert_eq!(
        kinds,
        vec![
            "task_proposed",
            "card_opened",
            "task_approved",
            "card_closed",
            "team_proposed",
            "card_opened",
            "team_approved",
            "card_closed",
            "round_started",
            "fish_launched",
            "fish_reported",
        ]
    );
    let seqs: Vec<u64> = events
        .iter()
        .map(|event| event["seq"].as_u64().expect("seq"))
        .collect();
    assert_eq!(seqs, (1..=11).collect::<Vec<u64>>());
    let error = f
        .arbiter
        .execute(&Fixture::elephant(), Command::TaskClose)
        .expect_err("sin rondas limpias");
    assert_eq!(error.rule, "R10");
    let events = f.bitacora(&task);
    let last = events.last().expect("último evento");
    assert_eq!(kind(last), "rejected");
    assert_eq!(last["kind"]["rule"], "R10");
}

#[test]
fn r12_elephant_events_arrive_in_order_and_timeout() {
    let f = Fixture::new("r12-cola");
    let _ = f.setup();
    assert_eq!(next(&f), "task_approved");
    assert_eq!(next(&f), "team_approved");
    assert_eq!(next(&f), "fish_reported");
    f.launch(Role::Developer);
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    let event = f
        .arbiter
        .wait_for_elephant("s1", Duration::from_millis(100))
        .expect("informe");
    assert_eq!(event.kind, "fish_reported");
    assert_eq!(event.data["fish"], "dev-r1-1");
    assert!(event.message.contains("dev-r1-1"));
    f.arbiter
        .execute(
            &Fixture::elephant(),
            Command::Pause {
                reason: "espera".to_string(),
            },
        )
        .expect("pausar");
    assert_eq!(next(&f), "paused");
    assert!(f
        .arbiter
        .wait_for_elephant("s1", Duration::from_millis(10))
        .is_none());
}

#[test]
fn r12_subscribers_see_fish_launch_and_signals() {
    let f = Fixture::new("r12-suscriptores");
    let receiver = f.arbiter.subscribe();
    let task = f.setup();
    let fish = f.launch(Role::Developer);
    f.report(&fish, Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    let key = f.fish_key(&fish);
    f.arbiter.fish_blocked(&key, Some("working"));
    f.arbiter.fish_exited(&key);
    let kinds: Vec<String> = receiver.try_iter().map(|event| live_kind(&event)).collect();
    assert!(kinds.contains(&"task_proposed".to_string()));
    assert!(kinds.contains(&"fish_launched".to_string()));
    assert!(kinds.contains(&"fish_blocked".to_string()));
    assert!(kinds.contains(&"fish_exited".to_string()));
    assert!(f
        .bitacora(&task)
        .iter()
        .any(|event| kind(event) == "fish_blocked"));
}

#[test]
fn r12_reload_keeps_unfinished_task_and_trace() {
    let f = Fixture::new("r12-recarga");
    let task = f.setup();
    f.launch(Role::Developer);
    f.report("dev-r1-1", Verdict::Pass, None, None)
        .expect("informe del desarrollador");
    let reopened = Arbiter::new(
        f.data_dir.clone(),
        f.launcher.clone(),
        Arc::new(FakeWriter),
        f.clock.clone(),
    )
    .expect("reabrir el árbitro");
    reopened.register_space(Fixture::space(&f.folder));
    let snapshot = reopened.snapshot("s1").expect("tarea recargada");
    assert_eq!(snapshot["task"], task);
    // §14.5: al reiniciar, las tareas no finales quedan pausadas.
    assert_eq!(snapshot["status"], "paused");
    assert_eq!(snapshot["pause_reason"], "reinicio");
    assert_eq!(snapshot["round"], 1);
    assert_eq!(snapshot["fish"][0]["id"], "plan-r1");
    assert_eq!(snapshot["fish"][1]["id"], "dev-r1-1");
    assert_eq!(snapshot["fish"][1]["attempt"], 1);
    // `seq` es monotónico por proceso: el reinicio emitió dos eventos.
    assert_eq!(snapshot["last_seq"].as_u64().expect("last_seq"), 2);
    assert_eq!(f.bitacora(&task).len(), 15);
}

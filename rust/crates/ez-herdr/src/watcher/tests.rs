use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

fn watcher(announce: bool) -> Watcher {
    let on_event: EventCallback = Arc::new(|_, _, _, _| {});
    let should_announce: AnnounceCallback = Arc::new(move |_| announce);
    Watcher::new("hc", on_event, should_announce, None, None)
}

/// Eventos publicados en la cola (el hilo de entrega no corre en la prueba).
fn events(watcher: &Watcher) -> Vec<Event> {
    let receiver = lock(&watcher.receiver);
    let mut out = Vec::new();
    while let Ok(Some(event)) = receiver.try_recv() {
        out.push(event);
    }
    out
}

fn terminal(address: &str, status: Value) -> Value {
    json!({"address": address, "status": status})
}

fn dict(state: &str) -> Value {
    json!({"state": state, "app": "claude"})
}

#[test]
fn la_primera_consulta_solo_prepara_el_estado() {
    let watcher = watcher(true);
    watcher.compare(&[terminal("t1", dict("working"))]);
    assert!(events(&watcher).is_empty());
    assert_eq!(watcher.state_of("t1").as_deref(), Some("working"));
    assert_eq!(watcher.state_of("t2"), None);
}

#[test]
fn working_a_done_avisa_done() {
    let watcher = watcher(true);
    watcher.compare(&[terminal("t1", dict("working"))]);
    watcher.compare(&[terminal("t1", dict("done"))]);
    assert_eq!(
        events(&watcher),
        vec![(
            "t1".to_string(),
            "claude".to_string(),
            "done".to_string(),
            "done".to_string()
        )]
    );
    assert_eq!(watcher.state_of("t1").as_deref(), Some("done"));
}

#[test]
fn working_a_idle_tambien_es_done() {
    let watcher = watcher(true);
    watcher.compare(&[terminal("t1", dict("working"))]);
    watcher.compare(&[terminal("t1", dict("idle"))]);
    let events = events(&watcher);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].3, "done");
}

#[test]
fn un_estado_quieto_no_repite_el_aviso() {
    let watcher = watcher(true);
    watcher.compare(&[terminal("t1", dict("working"))]);
    watcher.compare(&[terminal("t1", dict("done"))]);
    watcher.compare(&[terminal("t1", dict("done"))]);
    watcher.compare(&[terminal("t1", dict("idle"))]);
    assert_eq!(events(&watcher).len(), 1);
}

#[test]
fn blocked_y_error_se_avisan() {
    let watcher = watcher(true);
    watcher.compare(&[terminal("t1", dict("working"))]);
    watcher.compare(&[terminal("t1", dict("blocked"))]);
    watcher.compare(&[terminal("t1", dict("error"))]);
    let events = events(&watcher);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].2, "blocked");
    assert_eq!(events[0].3, "blocked");
    assert_eq!(events[1].2, "error");
    assert_eq!(events[1].3, "error");
}

#[test]
fn desaparecer_es_perdido_salvo_si_ya_habia_terminado() {
    let watcher = watcher(true);
    watcher.compare(&[
        terminal("t1", dict("working")),
        terminal("t2", dict("done")),
        terminal("t3", dict("blocked")),
    ]);
    watcher.compare(&[]);
    let events = events(&watcher);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].0, "t1");
    assert_eq!(events[0].3, "lost");
    assert_eq!(events[1].0, "t3");
    assert_eq!(events[1].3, "lost");
    assert!(watcher.state_of("t1").is_none());
}

#[test]
fn una_terminal_nueva_no_avisa() {
    let watcher = watcher(true);
    watcher.compare(&[terminal("t1", dict("working"))]);
    watcher.compare(&[
        terminal("t1", dict("working")),
        terminal("t2", dict("done")),
    ]);
    assert!(events(&watcher).is_empty());
}

#[test]
fn sin_anunciar_no_salen_avisos_pero_si_se_guarda_el_estado() {
    let watcher = watcher(false);
    watcher.compare(&[terminal("t1", dict("working"))]);
    watcher.compare(&[terminal("t1", dict("blocked"))]);
    watcher.compare(&[]);
    assert!(events(&watcher).is_empty());
    assert_eq!(watcher.state_of("t1"), None);
}

#[test]
fn el_estado_puede_venir_como_texto() {
    let watcher = watcher(true);
    watcher.compare(&[json!({"address": "t9", "agent": "opencode", "status": "working"})]);
    watcher.compare(&[json!({"address": "t9", "agent": "opencode", "status": "done"})]);
    assert_eq!(
        events(&watcher),
        vec![(
            "t9".to_string(),
            "opencode".to_string(),
            "done".to_string(),
            "done".to_string()
        )]
    );
}

#[test]
fn valores_por_defecto_del_estado() {
    let watcher = watcher(true);
    watcher.compare(&[
        json!({"address": "t1", "status": {"app": "codex"}}),
        json!({"address": "t2", "status": {"state": "working"}}),
        json!({"address": "t3"}),
        json!({"status": "working"}),
        json!({"address": "t4", "status": 3}),
    ]);
    assert_eq!(watcher.state_of("t1").as_deref(), Some("idle"));
    assert_eq!(watcher.state_of("t2").as_deref(), Some("working"));
    assert_eq!(watcher.state_of("t3"), None);
    assert_eq!(watcher.state_of("t4"), None);
    watcher.compare(&[json!({"address": "t2", "status": {"state": "done"}})]);
    assert_eq!(events(&watcher)[0].1, "agente");
}

#[test]
fn una_direccion_repetida_conserva_el_orden_y_gana_la_ultima() {
    let watcher = watcher(true);
    watcher.compare(&[
        terminal("t1", dict("working")),
        terminal("t2", dict("working")),
        terminal("t1", dict("blocked")),
    ]);
    assert_eq!(watcher.state_of("t1").as_deref(), Some("blocked"));
    watcher.compare(&[]);
    let events = events(&watcher);
    assert_eq!(events[0].0, "t1");
    assert_eq!(events[0].2, "lost");
    assert_eq!(events[1].0, "t2");
}

#[test]
fn el_latido_solo_avisa_cuando_cambia() {
    let health: Arc<Mutex<Vec<bool>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&health);
    let on_health: HealthCallback = Arc::new(move |healthy| {
        lock(&recorder).push(healthy.unwrap_or(false));
    });
    let on_event: EventCallback = Arc::new(|_, _, _, _| {});
    let should_announce: AnnounceCallback = Arc::new(|_| true);
    let watcher = Watcher::new("hc", on_event, should_announce, Some(on_health), None);
    assert!(watcher.healthy());
    watcher.set_health(true);
    watcher.set_health(false);
    watcher.set_health(false);
    watcher.set_health(true);
    assert_eq!(lock(&health).clone(), vec![false, true]);
    assert!(watcher.healthy());
}

#[test]
fn el_sondeo_falla_sin_hc() {
    let on_event: EventCallback = Arc::new(|_, _, _, _| {});
    let should_announce: AnnounceCallback = Arc::new(|_| true);
    let watcher = Watcher::new(
        "hc-que-no-existe-ezagent",
        on_event,
        should_announce,
        None,
        None,
    );
    assert!(!watcher.poll_once());
}

#[test]
fn arrancar_y_parar_no_se_cuelga() {
    let on_event: EventCallback = Arc::new(|_, _, _, _| {});
    let should_announce: AnnounceCallback = Arc::new(|_| true);
    let watcher = Arc::new(Watcher::new(
        "hc-que-no-existe-ezagent",
        on_event,
        should_announce,
        None,
        None,
    ));
    watcher.start();
    watcher.start();
    watcher.stop();
    watcher.stop();
    watcher.start();
    watcher.stop();
}

#[test]
fn el_sondeo_rapido_depende_de_los_objetivos() {
    let on_event: EventCallback = Arc::new(|_, _, _, _| {});
    let should_announce: AnnounceCallback = Arc::new(|_| true);
    let targets = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&targets);
    let has_targets: TargetsCallback = Arc::new(move || flag.load(Ordering::SeqCst));
    let watcher = Watcher::new("hc", on_event, should_announce, None, Some(has_targets));
    assert!(!watcher.wants_fast_poll());
    targets.store(true, Ordering::SeqCst);
    assert!(watcher.wants_fast_poll());
}

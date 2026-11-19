//! Pruebas de la conexión de eventos contra un `hc` falso en memoria.

mod support;

use std::time::Duration;

use ez_term::{EventStream, TermEvent};
use serde_json::json;

fn next(stream: &EventStream) -> TermEvent {
    stream
        .events()
        .recv_timeout(Duration::from_secs(2))
        .expect("evento en la conexión")
}

fn script_created_status_finished(fake: support::Fake) {
    fake.hello();
    let subscribe = fake.request();
    fake.respond(
        &subscribe,
        json!({"channel": null, "seq": 10, "terminals": []}),
    );
    fake.notify(json!({
        "seq": 11,
        "terminal": "t1",
        "type": "created",
        "data": support::terminal("t2", "dos"),
    }));
    fake.notify(json!({
        "seq": 12,
        "terminal": "t2",
        "type": "status",
        "data": {"state": "working", "kind": null, "msg": "pensando", "title": "", "app": "opencode", "progress": 40},
    }));
    fake.notify(json!({
        "seq": 13,
        "terminal": "t2",
        "type": "status",
        "data": {"msg": "sin palabra de estado"},
    }));
    fake.notify(json!({
        "seq": 14,
        "terminal": "t2",
        "type": "finished",
        "data": {"exit_code": 0, "finished_ms": 7, "reason": "exited", "signal": null},
    }));
}

fn script_removed_heartbeat_missed(fake: support::Fake) {
    fake.hello();
    let subscribe = fake.request();
    fake.respond(
        &subscribe,
        json!({"channel": null, "seq": 10, "terminals": []}),
    );
    fake.notify(json!({"seq": 15, "terminal": "t2", "type": "removed", "data": {}}));
    fake.notify(json!({
        "seq": 16,
        "terminal": "t2",
        "type": "foreground",
        "data": {"name": "PING.EXE", "pid": 10004},
    }));
    fake.raw(json!({"jsonrpc": "2.0", "method": "heartbeat", "params": {}}));
    fake.raw(json!({
        "jsonrpc": "2.0",
        "method": "missed",
        "params": {"seq": 20, "terminals": [support::terminal("t3", "tres")]},
    }));
    // Sin foto en el aviso: se entrega vacía (DECISION en `events.rs`).
    fake.raw(json!({"jsonrpc": "2.0", "method": "missed", "params": {"seq": 21}}));
}

#[test]
fn subscribe_sends_labels_and_keeps_snapshot() {
    let (stream, script) = support::events_scripted(|fake| {
        fake.hello();
        let subscribe = fake.request();
        assert_eq!(subscribe["method"], "events.subscribe");
        assert_eq!(subscribe["params"]["labels"]["ez"], "1");
        fake.respond(
            &subscribe,
            json!({
                "channel": null,
                "seq": 10,
                "terminals": [support::terminal("t1", "uno")],
            }),
        );
    });
    assert_eq!(stream.snapshot().len(), 1);
    assert_eq!(stream.snapshot()[0].address, "t1");
    assert_eq!(stream.snapshot()[0].name.as_deref(), Some("uno"));
    assert_eq!(stream.snapshot_seq(), Some(10));
    assert_eq!(next(&stream), TermEvent::Closed);
    script.join().expect("guion");
}

#[test]
fn created_status_and_finished_events() {
    let (stream, script) = support::events_scripted(script_created_status_finished);
    match next(&stream) {
        TermEvent::Created { terminal, seq } => {
            assert_eq!(terminal.name.as_deref(), Some("dos"));
            assert_eq!(seq, 11);
        }
        other => panic!("se esperaba Created: {other:?}"),
    }
    match next(&stream) {
        TermEvent::Status {
            terminal,
            status,
            seq,
        } => {
            assert_eq!(terminal, "t2");
            assert_eq!(status.state, "working");
            assert_eq!(status.progress, Some(40));
            assert_eq!(seq, 12);
        }
        other => panic!("se esperaba Status: {other:?}"),
    }
    match next(&stream) {
        TermEvent::Other { terminal, kind, .. } => {
            assert_eq!(terminal, "t2");
            assert_eq!(kind, "status");
        }
        other => panic!("se esperaba Other: {other:?}"),
    }
    match next(&stream) {
        TermEvent::Finished {
            terminal,
            exit_code,
            reason,
            seq,
        } => {
            assert_eq!(terminal, "t2");
            assert_eq!(exit_code, Some(0));
            assert_eq!(reason.as_deref(), Some("exited"));
            assert_eq!(seq, 14);
        }
        other => panic!("se esperaba Finished: {other:?}"),
    }
    assert_eq!(next(&stream), TermEvent::Closed);
    script.join().expect("guion");
}

#[test]
fn removed_heartbeat_and_missed_events() {
    let (stream, script) = support::events_scripted(script_removed_heartbeat_missed);
    assert_eq!(
        next(&stream),
        TermEvent::Removed {
            terminal: "t2".to_string(),
            seq: 15,
        }
    );
    match next(&stream) {
        TermEvent::Other {
            terminal,
            kind,
            data,
            ..
        } => {
            assert_eq!(terminal, "t2");
            assert_eq!(kind, "foreground");
            assert_eq!(data["name"], "PING.EXE");
        }
        other => panic!("se esperaba Other: {other:?}"),
    }
    assert_eq!(next(&stream), TermEvent::Heartbeat);
    match next(&stream) {
        TermEvent::Missed { snapshot, seq } => {
            let snapshot = snapshot.expect("foto del missed");
            assert_eq!(snapshot.len(), 1);
            assert_eq!(snapshot[0].address, "t3");
            assert_eq!(seq, Some(20));
        }
        other => panic!("se esperaba Missed: {other:?}"),
    }
    assert_eq!(
        next(&stream),
        TermEvent::Missed {
            snapshot: None,
            seq: None,
        }
    );
    assert_eq!(next(&stream), TermEvent::Closed);
    assert!(stream
        .events()
        .recv_timeout(Duration::from_secs(1))
        .is_err());
    script.join().expect("guion");
}

#[test]
fn missed_discards_old_events_and_keeps_new_ones() {
    let (stream, script) = support::events_scripted(|fake| {
        fake.hello();
        let subscribe = fake.request();
        fake.respond(
            &subscribe,
            json!({"channel": null, "seq": 10, "terminals": []}),
        );
        fake.notify(json!({"seq": 11, "terminal": "t1", "type": "removed", "data": {}}));
        // Llega un `missed` con foto en la secuencia 12.
        fake.raw(json!({
            "jsonrpc": "2.0",
            "method": "missed",
            "params": {"seq": 12, "terminals": [support::terminal("t1", "uno")]},
        }));
        // 11 y 12 ya están en la foto: se descartan; 13 es nuevo: pasa.
        fake.notify(json!({"seq": 11, "terminal": "t1", "type": "lost", "data": {}}));
        fake.notify(json!({"seq": 12, "terminal": "t1", "type": "lost", "data": {}}));
        fake.notify(json!({"seq": 13, "terminal": "t1", "type": "restarted", "data": {}}));
    });
    assert_eq!(
        next(&stream),
        TermEvent::Removed {
            terminal: "t1".to_string(),
            seq: 11,
        }
    );
    match next(&stream) {
        TermEvent::Missed { snapshot, seq } => {
            assert_eq!(snapshot.expect("foto")[0].address, "t1");
            assert_eq!(seq, Some(12));
        }
        other => panic!("se esperaba Missed: {other:?}"),
    }
    assert_eq!(
        next(&stream),
        TermEvent::Restarted {
            terminal: "t1".to_string(),
            seq: 13,
        }
    );
    assert_eq!(next(&stream), TermEvent::Closed);
    script.join().expect("guion");
}

#[test]
fn unknown_notifications_are_ignored() {
    let (stream, script) = support::events_scripted(|fake| {
        fake.hello();
        let subscribe = fake.request();
        fake.respond(&subscribe, json!({"seq": 1, "terminals": []}));
        fake.raw(json!({"jsonrpc": "2.0", "method": "algo-nuevo", "params": {}}));
        fake.raw(json!({"jsonrpc": "2.0", "method": "heartbeat", "params": {}}));
    });
    assert_eq!(next(&stream), TermEvent::Heartbeat);
    script.join().expect("guion");
}

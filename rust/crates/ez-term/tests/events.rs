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
    assert_eq!(next(&stream), TermEvent::Closed);
    script.join().expect("guion");
}

#[test]
fn events_follow_the_script_and_end_with_closed() {
    let (stream, script) = support::events_scripted(|fake| {
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
    });

    match next(&stream) {
        TermEvent::Created(terminal) => assert_eq!(terminal.name.as_deref(), Some("dos")),
        other => panic!("se esperaba Created: {other:?}"),
    }
    match next(&stream) {
        TermEvent::Status { terminal, status } => {
            assert_eq!(terminal, "t2");
            assert_eq!(status.state, "working");
            assert_eq!(status.progress, Some(40));
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
        } => {
            assert_eq!(terminal, "t2");
            assert_eq!(exit_code, Some(0));
            assert_eq!(reason.as_deref(), Some("exited"));
        }
        other => panic!("se esperaba Finished: {other:?}"),
    }
    assert_eq!(
        next(&stream),
        TermEvent::Removed {
            terminal: "t2".to_string()
        }
    );
    match next(&stream) {
        TermEvent::Other {
            terminal,
            kind,
            data,
        } => {
            assert_eq!(terminal, "t2");
            assert_eq!(kind, "foreground");
            assert_eq!(data["name"], "PING.EXE");
        }
        other => panic!("se esperaba Other: {other:?}"),
    }
    assert_eq!(next(&stream), TermEvent::Heartbeat);
    match next(&stream) {
        TermEvent::Missed { snapshot } => {
            assert_eq!(snapshot.len(), 1);
            assert_eq!(snapshot[0].address, "t3");
        }
        other => panic!("se esperaba Missed: {other:?}"),
    }
    assert_eq!(
        next(&stream),
        TermEvent::Missed {
            snapshot: Vec::new()
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

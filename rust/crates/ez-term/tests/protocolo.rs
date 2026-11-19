//! Deserialización de los ejemplos reales de `docs/herdr/hc-protocolo.md`.

mod support;

use std::time::Duration;

use ez_term::{Status, TermEvent, Terminal};
use serde_json::{json, Value};

#[test]
fn hello_result_from_doc() {
    let raw = r#"{"id":1,"jsonrpc":"2.0","result":{"core":{"id":"d5e9f0","name":"desktop-ig8chfg"},"features":["framed","attach","events","status","shell","resize_notifications","automatic_input","restart_environment"],"framed":false,"generation":1,"methods":["hello","server.status","terminal.run"],"version":"0.1.21"}}"#;
    let value: Value = serde_json::from_str(raw).expect("ejemplo del protocolo");
    assert_eq!(value["result"]["version"], "0.1.21");
}

#[test]
fn run_result_from_doc() {
    let raw = r#"{"address":"t72","argv":["cmd.exe","/d","/c","echo hola-ez"],"cols":200,"rows":120,"created_ms":1795049775431,"cwd":"C:\\Users\\JDMoreu","exit_code":null,"finished_ms":null,"identity":"ghostty","keep":false,"labels":{"ezprobe":"1"},"name":"ezprobe2","notify":true,"parent":null,"pid":5860,"reason":null,"state":"live","status":null,"status_ms":null,"template":null,"title":"","turn":0}"#;
    let terminal: Terminal = serde_json::from_str(raw).expect("terminal viva");
    assert_eq!(terminal.address, "t72");
    assert_eq!(terminal.name.as_deref(), Some("ezprobe2"));
    assert_eq!(
        terminal.labels.get("ezprobe").map(String::as_str),
        Some("1")
    );
    assert_eq!(terminal.state, "live");
    assert_eq!(terminal.exit_code, None);
    assert_eq!(terminal.cols, 200);
    assert_eq!(terminal.rows, 120);
    assert_eq!(terminal.turn, 0);
    assert_eq!(terminal.pid, Some(5860));
}

#[test]
fn get_result_finished_from_doc() {
    let raw = r#"{"address":"t72","argv":["cmd.exe","/d","/c","echo hola-ez"],"cols":200,"rows":120,"created_ms":1795049775431,"cwd":"C:\\Users\\JDMoreu","exit_code":0,"finished_ms":1795049775475,"labels":{"ezprobe":"1"},"name":"ezprobe2","reason":"exited","state":"finished","status":null,"turn":1}"#;
    let terminal: Terminal = serde_json::from_str(raw).expect("terminal terminada");
    assert_eq!(terminal.state, "finished");
    assert_eq!(terminal.exit_code, Some(0));
    assert_eq!(terminal.reason.as_deref(), Some("exited"));
}

#[test]
fn missing_fields_take_defaults() {
    let terminal: Terminal = serde_json::from_str(r#"{"address":"t1"}"#).expect("mínimo");
    assert_eq!(terminal.address, "t1");
    assert_eq!(terminal.name, None);
    assert!(terminal.labels.is_empty());
}

#[test]
fn status_from_schema() {
    let raw = r#"{"state":"blocked","kind":"permission","msg":"¿Permito el comando?","title":"editar","app":"claude","progress":null}"#;
    let status: Status = serde_json::from_str(raw).expect("objeto Status");
    assert_eq!(status.state, "blocked");
    assert_eq!(status.kind.as_deref(), Some("permission"));
    assert_eq!(status.progress, None);
}

#[test]
fn created_event_from_doc() {
    let (stream, _script) = support::events_scripted(|fake| {
        fake.hello();
        let subscribe = fake.request();
        fake.respond(
            &subscribe,
            json!({"channel": null, "seq": 156, "terminals": []}),
        );
        fake.raw(json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "data": {
                    "address": "t73",
                    "argv": ["cmd.exe", "/d", "/c", "ping -n 2 127.0.0.1"],
                    "cols": 200,
                    "created_ms": 1795049775431u64,
                    "cwd": "C:\\Users\\JDMoreu",
                    "labels": {"ezprobe": "3"},
                    "name": "ezprobe3",
                    "rows": 120,
                    "screen": "exact",
                    "state": "live",
                    "turn": 0,
                },
                "seq": 157,
                "terminal": "t73",
                "type": "created",
            },
        }));
    });
    match stream
        .events()
        .recv_timeout(Duration::from_secs(2))
        .expect("evento created")
    {
        TermEvent::Created { terminal, seq } => {
            assert_eq!(seq, 157);
            assert_eq!(terminal.address, "t73");
            assert_eq!(terminal.name.as_deref(), Some("ezprobe3"));
            assert_eq!(
                terminal.labels.get("ezprobe").map(String::as_str),
                Some("3")
            );
        }
        other => panic!("se esperaba Created: {other:?}"),
    }
}

#[test]
fn finished_event_from_doc() {
    let (stream, _script) = support::events_scripted(|fake| {
        fake.hello();
        let subscribe = fake.request();
        fake.respond(
            &subscribe,
            json!({"channel": null, "seq": 159, "terminals": []}),
        );
        fake.raw(json!({
            "jsonrpc": "2.0",
            "method": "event",
            "params": {
                "data": {"exit_code": 0, "finished_ms": 1795049796350u64, "reason": "exited", "signal": null},
                "seq": 160,
                "terminal": "t73",
                "type": "finished",
            },
        }));
    });
    match stream
        .events()
        .recv_timeout(Duration::from_secs(2))
        .expect("evento finished")
    {
        TermEvent::Finished {
            terminal,
            exit_code,
            reason,
            seq,
        } => {
            assert_eq!(terminal, "t73");
            assert_eq!(exit_code, Some(0));
            assert_eq!(reason.as_deref(), Some("exited"));
            assert_eq!(seq, 160);
        }
        other => panic!("se esperaba Finished: {other:?}"),
    }
}

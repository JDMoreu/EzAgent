//! Pruebas de `launch_and_deliver` contra un `hc` falso en memoria (§14.2).

mod support;

use std::thread::JoinHandle;
use std::time::Duration;

use ez_term::{Delivery, HcClient, RunSpec, TermError};
use serde_json::{json, Value};

const FIRST_LINE: &str = "Eres un pez dorado de EzAgent.";

fn client<F>(script: F) -> (HcClient, JoinHandle<()>)
where
    F: FnOnce(support::Fake) + Send + 'static,
{
    let (client, handle) = support::client_scripted(script);
    let client = client.with_timing(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(50),
        Duration::from_millis(1),
    );
    (client, handle)
}

fn spec() -> RunSpec {
    RunSpec {
        argv: vec!["opencode".to_string()],
        name: Some("pez".to_string()),
        ..RunSpec::default()
    }
}

fn reply_run(fake: &support::Fake) {
    let request = fake.request();
    assert_eq!(request["method"], "terminal.run");
    assert_eq!(request["params"]["argv"][0], "opencode");
    fake.respond(&request, support::terminal("t1", "pez"));
}

fn reply_wait(fake: &support::Fake) {
    let request = fake.request();
    assert_eq!(request["method"], "terminal.wait");
    assert_eq!(request["params"]["terminal"], "t1");
    fake.respond(
        &request,
        json!({"ended": true, "reason": "exit", "turn": 0}),
    );
}

fn reply_send(fake: &support::Fake, started: Value) {
    let request = fake.request();
    assert_eq!(request["method"], "terminal.send");
    assert_eq!(request["params"]["terminal"], "t1");
    assert_eq!(request["params"]["text"], FIRST_LINE);
    fake.respond(
        &request,
        json!({"turn": 4, "started": started, "status": "idle"}),
    );
}

fn reply_read(fake: &support::Fake, text: &str) {
    let request = fake.request();
    assert_eq!(request["method"], "terminal.read");
    assert_eq!(request["params"]["lines"], 50);
    fake.respond(&request, json!({"text": text, "complete": true}));
}

#[test]
fn delivered_when_first_send_starts_the_fish() {
    let (client, script) = client(|fake| {
        fake.hello();
        reply_run(&fake);
        reply_wait(&fake);
        reply_send(&fake, json!(true));
    });
    let (terminal, delivery) = client
        .launch_and_deliver(&spec(), FIRST_LINE, Duration::from_secs(2))
        .expect("lanzamiento");
    assert_eq!(terminal.address, "t1");
    assert_eq!(delivery, Delivery::Delivered);
    script.join().expect("guion");
}

#[test]
fn unconfirmed_when_started_is_null() {
    let (client, script) = client(|fake| {
        fake.hello();
        reply_run(&fake);
        reply_wait(&fake);
        reply_send(&fake, Value::Null);
    });
    let (_terminal, delivery) = client
        .launch_and_deliver(&spec(), FIRST_LINE, Duration::from_secs(2))
        .expect("lanzamiento");
    assert_eq!(delivery, Delivery::Unconfirmed);
    script.join().expect("guion");
}

#[test]
fn retry_delivers_when_second_send_starts_the_fish() {
    let (client, script) = client(|fake| {
        fake.hello();
        reply_run(&fake);
        reply_wait(&fake);
        reply_send(&fake, json!(false));
        reply_read(&fake, "pantalla del pez");
        reply_send(&fake, json!(true));
    });
    let (_terminal, delivery) = client
        .launch_and_deliver(&spec(), FIRST_LINE, Duration::from_secs(2))
        .expect("lanzamiento");
    assert_eq!(delivery, Delivery::Delivered);
    script.join().expect("guion");
}

#[test]
fn retry_fails_with_the_screen_snippet() {
    let (client, script) = client(|fake| {
        fake.hello();
        reply_run(&fake);
        reply_wait(&fake);
        reply_send(&fake, json!(false));
        reply_read(&fake, "pantalla del pez");
        reply_send(&fake, json!(false));
    });
    let (_terminal, delivery) = client
        .launch_and_deliver(&spec(), FIRST_LINE, Duration::from_secs(2))
        .expect("lanzamiento");
    match delivery {
        Delivery::Failed(reason) => assert!(reason.contains("pantalla del pez")),
        other => panic!("se esperaba Failed: {other:?}"),
    }
    script.join().expect("guion");
}

#[test]
fn run_failure_returns_the_error() {
    let (client, script) = client(|fake| {
        fake.hello();
        let request = fake.request();
        assert_eq!(request["method"], "terminal.run");
        fake.respond_error(&request, -32000, "name_taken", "ya existe");
    });
    let error = client
        .launch_and_deliver(&spec(), FIRST_LINE, Duration::from_secs(2))
        .expect_err("run falló");
    assert!(matches!(error, TermError::Rpc { .. }));
    script.join().expect("guion");
}

#[test]
fn later_failure_keeps_the_created_terminal() {
    let (client, script) = client(|fake| {
        fake.hello();
        reply_run(&fake);
        reply_wait(&fake);
        let request = fake.request();
        assert_eq!(request["method"], "terminal.send");
        fake.respond_error(&request, -32000, "not_found", "sin terminal");
    });
    let (terminal, delivery) = client
        .launch_and_deliver(&spec(), FIRST_LINE, Duration::from_secs(2))
        .expect("resultado con terminal");
    assert_eq!(terminal.address, "t1");
    match delivery {
        Delivery::Failed(reason) => assert!(reason.contains("envío inicial")),
        other => panic!("se esperaba Failed: {other:?}"),
    }
    script.join().expect("guion");
}

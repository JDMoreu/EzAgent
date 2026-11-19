//! Pruebas del cliente de peticiones contra un `hc` falso en memoria.

mod support;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use ez_term::{HcClient, RunSpec, SendOutcome, TermError};
use serde_json::json;

fn labels() -> BTreeMap<String, String> {
    BTreeMap::from([("ez".to_string(), "1".to_string())])
}

fn live_terminal() -> serde_json::Value {
    json!({
        "address": "t1",
        "name": "ezprobe",
        "argv": ["hostname"],
        "cwd": "C:\\tmp",
        "labels": {"ez": "1"},
        "state": "live",
        "cols": 100,
        "rows": 40,
        "turn": 0,
        "created_ms": 1,
        "screen": "exact",
        "pid": 4242,
    })
}

fn finished_terminal() -> serde_json::Value {
    json!({
        "address": "t1",
        "name": "ezprobe",
        "argv": ["hostname"],
        "cwd": "C:\\tmp",
        "labels": {"ez": "1"},
        "state": "finished",
        "exit_code": 0,
        "reason": "exited",
        "cols": 100,
        "rows": 40,
        "created_ms": 1,
        "screen": "exact",
    })
}

#[test]
fn hello_sets_core_version() {
    let (client, script) = support::client_scripted(|fake| fake.hello());
    assert_eq!(client.core_version(), "0.1.21");
    script.join().expect("guion");
}

#[test]
fn run_get_and_rm() {
    let (client, script) = support::client_scripted(|fake| {
        fake.hello();
        let request = fake.request();
        assert_eq!(request["method"], "terminal.run");
        assert_eq!(request["params"]["argv"][0], "hostname");
        assert_eq!(request["params"]["cwd"], "C:\\tmp");
        assert_eq!(request["params"]["name"], "ezprobe");
        assert_eq!(request["params"]["env"]["EZ_PROBE"], "1");
        assert_eq!(request["params"]["size"]["cols"], 100);
        assert_eq!(request["params"]["size"]["rows"], 40);
        assert_eq!(request["params"]["no_parent"], true);
        assert_eq!(request["params"]["no_notify"], true);
        fake.respond(&request, live_terminal());

        let request = fake.request();
        assert_eq!(request["method"], "terminal.get");
        assert_eq!(request["params"]["terminal"], "t1");
        fake.respond(&request, finished_terminal());

        let request = fake.request();
        assert_eq!(request["method"], "terminal.rm");
        assert_eq!(request["params"]["terminal"], "t1");
        fake.respond(&request, json!({ "removed": "t1" }));
    });

    let spec = RunSpec {
        argv: vec!["hostname".to_string()],
        cwd: Some(PathBuf::from("C:\\tmp")),
        name: Some("ezprobe".to_string()),
        labels: labels(),
        env: BTreeMap::from([("EZ_PROBE".to_string(), "1".to_string())]),
        size: Some((100, 40)),
        ..RunSpec::default()
    };
    let terminal = client.run(&spec).expect("terminal.run");
    assert_eq!(terminal.address, "t1");
    assert_eq!(terminal.name.as_deref(), Some("ezprobe"));
    assert_eq!(terminal.pid, Some(4242));

    let finished = client.get("t1").expect("terminal.get");
    assert_eq!(finished.state, "finished");
    assert_eq!(finished.exit_code, Some(0));
    assert_eq!(finished.reason.as_deref(), Some("exited"));

    client.rm("t1").expect("terminal.rm");
    script.join().expect("guion");
}

#[test]
fn list_sends_labels_and_parses_terminals() {
    let (client, script) = support::client_scripted(|fake| {
        fake.hello();
        let request = fake.request();
        assert_eq!(request["method"], "terminal.list");
        assert_eq!(request["params"]["labels"]["ez"], "1");
        fake.respond(
            &request,
            json!({
                "terminals": [
                    support::terminal("t1", "uno"),
                    support::terminal("t2", "dos"),
                ],
                "hidden_finished": 0,
                "machines": [],
            }),
        );
    });
    let terminals = client.list(&labels()).expect("terminal.list");
    assert_eq!(terminals.len(), 2);
    assert_eq!(terminals[1].name.as_deref(), Some("dos"));
    script.join().expect("guion");
}

#[test]
fn responses_match_by_id_in_any_order() {
    let (client, script) = support::client_scripted(|fake| {
        fake.hello();
        let request_a = fake.request();
        let request_b = fake.request();
        let terminal_a = request_a["params"]["terminal"]
            .as_str()
            .expect("terminal del primer pedido")
            .to_string();
        let terminal_b = request_b["params"]["terminal"]
            .as_str()
            .expect("terminal del segundo pedido")
            .to_string();
        // Responder en orden inverso al de llegada.
        fake.respond(
            &request_b,
            json!({"text": format!("eco-{terminal_b}"), "complete": true, "how": "screen"}),
        );
        fake.respond(
            &request_a,
            json!({"text": format!("eco-{terminal_a}"), "complete": true, "how": "screen"}),
        );
    });

    let first = client.clone();
    let second = client.clone();
    let a = thread::spawn(move || first.read("a", 1).map(|text| text.text));
    let b = thread::spawn(move || second.read("b", 1).map(|text| text.text));
    assert_eq!(a.join().expect("hilo a"), Ok("eco-a".to_string()));
    assert_eq!(b.join().expect("hilo b"), Ok("eco-b".to_string()));
    script.join().expect("guion");
}

#[test]
fn rpc_error_carries_kind() {
    let (client, script) = support::client_scripted(|fake| {
        fake.hello();
        let request = fake.request();
        assert_eq!(request["method"], "terminal.get");
        fake.respond_error(&request, -32000, "not_found", "not_found");
    });
    let error = client
        .call(
            "terminal.get",
            json!({"terminal": "t9"}),
            Duration::from_secs(2),
        )
        .expect_err("error JSON-RPC");
    assert_eq!(
        error,
        TermError::Rpc {
            code: -32000,
            kind: Some("not_found".to_string()),
            message: "not_found".to_string(),
        }
    );
    assert!(error.to_string().contains("hc rechazó"));
    script.join().expect("guion");
}

#[test]
fn timeout_reports_method() {
    let (client, script) = support::client_scripted(|fake| {
        fake.hello();
        let first = fake.request();
        assert_eq!(first["method"], "terminal.get");
        // Sin respuesta: vence el tope de esa llamada.
        let second = fake.request();
        assert_eq!(second["method"], "terminal.get");
        fake.respond(&second, support::terminal("t9", "nueve"));
    });
    let error = client
        .call(
            "terminal.get",
            json!({"terminal": "t9"}),
            Duration::from_millis(120),
        )
        .expect_err("debe vencer el tope");
    assert_eq!(
        error,
        TermError::Timeout {
            method: "terminal.get".to_string()
        }
    );
    // La conexión sigue usable después de un tope.
    let terminal = client.get("t9").expect("terminal.get");
    assert_eq!(terminal.name.as_deref(), Some("nueve"));
    script.join().expect("guion");
}

#[test]
fn eof_mid_call_disconnects() {
    let (client, script) = support::client_scripted(|fake| {
        fake.hello();
        let _request = fake.request();
        // Al terminar el guion se cierra el flujo con una llamada pendiente.
    });
    let handle = {
        let client = client.clone();
        thread::spawn(move || {
            client.call(
                "terminal.get",
                json!({"terminal": "t9"}),
                Duration::from_secs(10),
            )
        })
    };
    assert_eq!(
        handle.join().expect("hilo de la llamada"),
        Err(TermError::Disconnected)
    );
    assert_eq!(
        client.call("terminal.get", json!({}), Duration::from_secs(1)),
        Err(TermError::Disconnected)
    );
    script.join().expect("guion");
}

#[test]
fn send_and_wait_use_operation_tops() {
    let (client, script) = support::client_scripted(|fake| {
        fake.hello();
        let request = fake.request();
        assert_eq!(request["method"], "terminal.send");
        assert_eq!(request["params"]["terminal"], "t1");
        assert_eq!(request["params"]["text"], "hola");
        fake.respond(
            &request,
            json!({"turn": 4, "started": true, "status": "idle"}),
        );
        let request = fake.request();
        assert_eq!(request["method"], "terminal.wait");
        assert_eq!(request["params"]["terminal"], "t1");
        assert_eq!(request["params"]["timeout"], 1.5);
        fake.respond(
            &request,
            json!({
                "terminal": "t1",
                "ended": true,
                "reason": "exit",
                "turn": 5,
                "children": [],
                "exit_code": 0,
                "signal": null,
                "status": null,
                "from": null,
            }),
        );
    });

    let outcome = client.send("t1", "hola").expect("terminal.send");
    assert_eq!(
        outcome,
        SendOutcome {
            turn: 4,
            started: Some(true),
            status: Some("idle".to_string()),
        }
    );
    let value = client.wait("t1", 1.5).expect("terminal.wait");
    assert_eq!(value["reason"], "exit");
    script.join().expect("guion");
}

#[test]
fn keys_interrupt_and_kill_wrap_their_methods() {
    let (client, script) = support::client_scripted(|fake| {
        fake.hello();
        let keys = fake.request();
        assert_eq!(keys["method"], "terminal.keys");
        assert_eq!(keys["params"]["keys"][0], "enter");
        assert_eq!(keys["params"]["keys"][1], "ctrl+c");
        fake.respond(&keys, json!({}));
        let interrupt = fake.request();
        assert_eq!(interrupt["method"], "terminal.interrupt");
        assert_eq!(interrupt["params"]["terminal"], "t1");
        fake.respond(&interrupt, json!({}));
        let kill = fake.request();
        assert_eq!(kill["method"], "terminal.kill");
        assert_eq!(kill["params"]["terminal"], "t1");
        fake.respond(&kill, json!({}));
    });
    client
        .keys("t1", &["enter".to_string(), "ctrl+c".to_string()])
        .expect("terminal.keys");
    client.interrupt("t1").expect("terminal.interrupt");
    client.kill("t1").expect("terminal.kill");
    script.join().expect("guion");
}

#[test]
fn simple_calls_use_the_simple_timeout() {
    let (client, script) = support::client_scripted(|fake| {
        fake.hello();
        let request = fake.request();
        assert_eq!(request["method"], "terminal.get");
        // Sin respuesta: vence el tope de las llamadas simples; se mantiene
        // abierto el canal para que no parezca un fin de flujo.
        thread::sleep(Duration::from_millis(300));
    });
    let client = client.with_timing(
        Duration::from_millis(120),
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(1),
    );
    assert_eq!(
        client.get("t1"),
        Err(TermError::Timeout {
            method: "terminal.get".to_string()
        })
    );
    script.join().expect("guion");
}

#[test]
fn send_uses_its_own_timeout() {
    let (client, script) = support::client_scripted(|fake| {
        fake.hello();
        let request = fake.request();
        assert_eq!(request["method"], "terminal.send");
        // Sin respuesta: vence el tope propio de `send`.
        thread::sleep(Duration::from_millis(300));
    });
    let client = client.with_timing(
        Duration::from_secs(1),
        Duration::from_millis(120),
        Duration::from_secs(1),
        Duration::from_millis(1),
    );
    assert_eq!(
        client.send("t1", "hola"),
        Err(TermError::Timeout {
            method: "terminal.send".to_string()
        })
    );
    script.join().expect("guion");
}

#[test]
fn wait_adds_its_margin_to_the_caller_timeout() {
    let (client, script) = support::client_scripted(|fake| {
        fake.hello();
        let request = fake.request();
        assert_eq!(request["method"], "terminal.wait");
        assert_eq!(request["params"]["timeout"], 0.1);
        // Sin respuesta: vence `timeout_s` más el margen inyectado.
        thread::sleep(Duration::from_millis(300));
    });
    let client = client.with_timing(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(120),
        Duration::from_millis(1),
    );
    assert_eq!(
        client.wait("t1", 0.1),
        Err(TermError::Timeout {
            method: "terminal.wait".to_string()
        })
    );
    script.join().expect("guion");
}

#[test]
fn client_is_clone_send_and_sync() {
    fn assert_traits<T: Clone + Send + Sync>() {}
    assert_traits::<HcClient>();
}

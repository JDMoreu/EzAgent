//! Hilos de wait: 32 esperas retenidas, desbordamiento 503 y parada rápida.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use ez_ctl::{Caller, CtlServer, Handler, Reply, Request, Tokens, MAX_WAITS};
use serde_json::json;
use support::{SPACE, TASK, TOKEN};

const WAIT_BODY: &str = r#"{"command":"wait","args":{"timeout":170}}"#;
const STATUS_BODY: &str = r#"{"command":"status","args":{}}"#;
const REPORT_BODY: &str = r#"{"command":"report","args":{"verdict":"pass"}}"#;
const LIMIT: Duration = Duration::from_secs(8);

/// Un `Handler` que retiene los `wait` hasta que se abre la compuerta.
struct HoldingHandler {
    entered: AtomicUsize,
    released: Mutex<bool>,
    wake: Condvar,
}

impl HoldingHandler {
    fn new() -> Arc<HoldingHandler> {
        Arc::new(HoldingHandler {
            entered: AtomicUsize::new(0),
            released: Mutex::new(false),
            wake: Condvar::new(),
        })
    }

    fn wait_until(&self, count: usize) {
        let start = Instant::now();
        loop {
            let entered = self.entered.load(Ordering::SeqCst);
            if entered >= count {
                return;
            }
            assert!(start.elapsed() < LIMIT, "solo entraron {entered} esperas");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn release(&self) {
        let mut released = self.released.lock().expect("compuerta");
        *released = true;
        self.wake.notify_all();
    }
}

impl Handler for HoldingHandler {
    fn handle(&self, _caller: &Caller, request: &Request) -> Reply {
        if request.command == "wait" {
            self.entered.fetch_add(1, Ordering::SeqCst);
            let mut released = self.released.lock().expect("compuerta");
            while !*released {
                released = self.wake.wait(released).expect("compuerta");
            }
        }
        Reply::ok("hecho", json!({}))
    }
}

fn start(handler: &Arc<HoldingHandler>) -> CtlServer {
    let tokens = Arc::new(Tokens::new());
    tokens.register(
        TOKEN,
        Caller::Fish {
            space: SPACE.to_string(),
            task: TASK.to_string(),
            fish: "dev-r1-1".to_string(),
            attempt: 1,
        },
    );
    let dynamic: Arc<dyn Handler> = handler.clone();
    CtlServer::start(dynamic, tokens).expect("servidor")
}

fn post(url: &str, body: &str) -> (u16, String) {
    let response = reqwest::blocking::Client::new()
        .post(format!("{url}/v1/command"))
        .bearer_auth(TOKEN)
        .body(body.to_string())
        .send()
        .expect("conexión");
    let status = response.status().as_u16();
    let text = response.text().expect("cuerpo");
    (status, text)
}

#[test]
fn thirty_two_waits_do_not_block_report_and_the_next_wait_is_503() {
    let handler = HoldingHandler::new();
    let server = start(&handler);
    let url = server.url();
    let mut clients = Vec::with_capacity(MAX_WAITS);
    for _ in 0..MAX_WAITS {
        let url = url.clone();
        clients.push(thread::spawn(move || post(&url, WAIT_BODY)));
    }
    handler.wait_until(MAX_WAITS);

    for body in [REPORT_BODY, STATUS_BODY] {
        let start = Instant::now();
        let (status, text) = post(&url, body);
        assert_eq!(status, 200, "{text}");
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "el comando tardó {:?} con 32 esperas en curso",
            start.elapsed()
        );
    }

    let (status, text) = post(&url, WAIT_BODY);
    assert_eq!(status, 503, "{text}");
    assert_eq!(
        text,
        r#"{"ok":false,"message":"Demasiadas esperas.","data":null}"#
    );

    handler.release();
    for client in clients {
        let (status, text) = client.join().expect("hilo de wait");
        assert_eq!(status, 200, "{text}");
    }
    server.stop();
}

#[test]
fn stop_does_not_wait_for_held_waits() {
    let handler = HoldingHandler::new();
    let server = start(&handler);
    let url = server.url();
    let client = {
        let url = url.clone();
        thread::spawn(move || post(&url, WAIT_BODY))
    };
    handler.wait_until(1);

    let start = Instant::now();
    server.stop();
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "stop tardó {:?} con un wait retenido",
        start.elapsed()
    );

    handler.release();
    let (status, text) = client.join().expect("hilo de wait");
    assert_eq!(status, 200, "{text}");
}

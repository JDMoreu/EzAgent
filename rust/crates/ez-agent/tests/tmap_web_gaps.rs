//! Conductas de `tools_web` que los fixtures no cubrían (TMAP-1).
#![allow(dead_code)] // reutiliza todos los dobles de `tools_web_parity/fakes.rs`

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ez_agent::tools_web::{
    run_fetch_page_json, validate_public_target_with, FetchHooks, WebRequest, WebResponse,
};
use ez_core::agent_config::AgentConfig;
use serde_json::json;

#[path = "tools_web_parity/fakes.rs"]
mod fakes;
use fakes::{page, Dns, FakeClock, FakeResolver, FakeTransport};

const PUBLIC: &str = "93.184.216.34";

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ez-web-tmap-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("carpeta temporal");
    dir
}

fn config(dir: &std::path::Path) -> AgentConfig {
    let (mut config, _) = AgentConfig::load(Some(&dir.join("permissions.toml")));
    config.web.max_fetch_chars = 8000;
    config.web.block_private_ips = true;
    config
}

fn public() -> Dns {
    Dns::Addresses(vec![PUBLIC.parse().expect("IPv4 pública")])
}

#[test]
fn decimal_hex_and_octal_hosts_are_never_loopback_literals() {
    let dir = temp("hosts");
    let config = config(&dir);
    let resolver = FakeResolver(BTreeMap::new());
    for host in ["2130706433", "0x7f000001", "0177.0.0.1"] {
        let outcome = validate_public_target_with(&format!("http://{host}/"), &config, &resolver);
        assert!(outcome.is_err(), "{host} no debe resolverse como loopback");
    }
}

#[test]
fn a_mixed_dns_answer_with_a_private_address_is_blocked() {
    let dir = temp("rebind");
    let config = config(&dir);
    let mut table = BTreeMap::new();
    table.insert(
        "rebind.test".to_string(),
        Dns::Addresses(vec![
            PUBLIC.parse().expect("IPv4 pública"),
            "10.0.0.5".parse().expect("IPv4 privada"),
        ]),
    );
    let error = validate_public_target_with("http://rebind.test/", &config, &FakeResolver(table))
        .expect_err("un solo registro privado basta");
    assert!(error.message.contains("bloqueado"), "{}", error.message);
}

#[test]
fn a_redirect_to_credentials_is_blocked_before_the_second_hop() {
    let dir = temp("redirect");
    let config = config(&dir);
    let mut table = BTreeMap::new();
    table.insert("example.test".to_string(), public());
    table.insert("other.test".to_string(), public());
    let calls: Arc<Mutex<Vec<WebRequest>>> = Arc::default();
    let seen = Arc::clone(&calls);
    let transport = Arc::new(FakeTransport {
        calls: Mutex::new(Vec::new()),
        handler: Box::new(move |request: &WebRequest| {
            seen.lock().expect("llamadas").push(request.clone());
            if request.url.contains("example.test") {
                Ok(WebResponse {
                    status: 302,
                    headers: vec![(
                        "location".to_string(),
                        "http://user:pw@other.test/".to_string(),
                    )],
                    body: Vec::new(),
                })
            } else {
                Ok(page(200, b"SECRETO INTERNO", "text/html; charset=utf-8"))
            }
        }),
    });
    let hooks = FetchHooks {
        transport: Some(transport),
        resolver: Some(Arc::new(FakeResolver(table))),
        clock: Some(Arc::new(FakeClock::new(&[1000.0; 4]))),
        pin_requests: false,
    };
    let result = run_fetch_page_json(
        &config,
        &json!("http://example.test/"),
        &json!(8000),
        &hooks,
    );
    assert!(!result.ok);
    assert!(calls.lock().expect("llamadas").len() == 1);
}

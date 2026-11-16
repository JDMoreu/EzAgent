use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use super::*;

fn serve(responses: Vec<(&'static str, &'static str)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("puerto libre");
    let address = listener.local_addr().expect("dirección");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&seen);
    std::thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buffer = [0_u8; 8192];
            let read = stream.read(&mut buffer).unwrap_or(0);
            captured
                .lock()
                .expect("peticiones")
                .push(String::from_utf8_lossy(&buffer[..read]).into_owned());
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body.as_bytes());
        }
    });
    (format!("http://{address}"), seen)
}

#[test]
fn aliases_and_styles() {
    assert_eq!(normalize_provider(Some(" CLAUDE ")), "anthropic");
    assert_eq!(normalize_provider(None), "opencode");
    assert_eq!(normalize_api_style(Some(" Anthropic ")), "anthropic");
    assert_eq!(normalize_api_style(Some("otro")), "openai");
}

#[test]
fn model_ids_skip_non_chat_and_prefix() {
    let payload = serde_json::json!({"data": [
        {"id": "models/gemini-2.5-flash"}, {"id": "whisper-1"}, "b", {"id": "a"}, {"id": "b"}
    ]});
    assert_eq!(
        chat_model_ids(&payload, 80),
        vec!["a", "b", "gemini-2.5-flash"]
    );
    assert_eq!(chat_model_ids(&payload, 1), vec!["a"]);
    let flat = serde_json::json!(["m0", "m1", "m2"]);
    assert_eq!(chat_model_ids(&flat, 2), vec!["m0", "m1"]);
}

#[test]
fn fetch_models_sends_python_headers_and_reports_errors() {
    let (base, seen) = serve(vec![
        (
            "200 OK",
            r#"{"data": [{"id": "models/gemini-2.5-flash"}, {"id": "whisper-1"}, {"id": "gpt-4o"}]}"#,
        ),
        ("200 OK", r#"{"data": []}"#),
        ("200 OK", r#"{"data": []}"#),
        ("401 Unauthorized", "{}"),
        ("200 OK", "<html>"),
    ]);
    let timeout = Duration::from_secs(5);
    let ids =
        fetch_models(&format!("{base}/v1"), "clave-1", "openai", timeout, 80).expect("modelos");
    assert_eq!(ids, vec!["gemini-2.5-flash", "gpt-4o"]);
    fetch_models(&base, "clave-2", "anthropic", timeout, 80).expect("modelos");
    fetch_models(&format!("{base}/v1"), "", "openai", timeout, 80).expect("modelos");
    let error = fetch_models(&base, "CLAVE-PRIVADA", "openai", timeout, 80).unwrap_err();
    assert_eq!(error, "La lista de modelos respondió 401.");
    assert!(!error.contains("CLAVE-PRIVADA"));
    let error = fetch_models(&base, "CLAVE-PRIVADA", "openai", timeout, 80).unwrap_err();
    assert_eq!(error, "La lista de modelos no es JSON.");
    assert!(!error.contains("CLAVE-PRIVADA"));

    let requests = seen.lock().expect("peticiones");
    assert!(requests[0].starts_with("GET /v1/models HTTP/1.1"));
    assert!(requests[0].contains("authorization: Bearer clave-1"));
    assert!(!requests[0].to_lowercase().contains("x-api-key"));
    assert!(requests[1].starts_with("GET /v1/models?limit=1000 HTTP/1.1"));
    assert!(requests[1].contains("x-api-key: clave-2"));
    assert!(requests[1].contains("anthropic-version: 2023-06-01"));
    assert!(!requests[1].to_lowercase().contains("authorization:"));
    assert!(!requests[2].to_lowercase().contains("authorization:"));
}

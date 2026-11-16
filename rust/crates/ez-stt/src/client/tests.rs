//! Pruebas de `client.rs`: servidor HTTP falso y modelos en carpetas temporales.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::models::{available_in, model_path_in, pick_home};
use super::*;

struct Recorded {
    path: String,
    body: Vec<u8>,
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn read_request(stream: &mut std::net::TcpStream) -> Recorded {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    let header_end = loop {
        if let Some(index) = find_bytes(&buffer, b"\r\n\r\n") {
            break index + 4;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break buffer.len(),
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
    };
    let header = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
    let length = header
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = buffer[header_end..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => body.extend_from_slice(&chunk[..read]),
        }
    }
    body.truncate(length);
    let path = header
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("")
        .to_string();
    Recorded { path, body }
}

fn fake_server_delayed(
    responses: Vec<(u16, String)>,
    delay: Duration,
) -> (String, Arc<Mutex<Vec<Recorded>>>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener falso");
    let port = listener.local_addr().expect("puerto local").port();
    let records = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&records);
    let handle = std::thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let request = read_request(&mut stream);
            captured
                .lock()
                .expect("registro del servidor")
                .push(request);
            std::thread::sleep(delay);
            let reason = if status == 200 { "OK" } else { "Error" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://127.0.0.1:{port}"), records, handle)
}

fn fake_server(
    responses: Vec<(u16, String)>,
) -> (String, Arc<Mutex<Vec<Recorded>>>, JoinHandle<()>) {
    fake_server_delayed(responses, Duration::ZERO)
}

fn json_response(body: &str) -> (u16, String) {
    (200, body.to_string())
}

fn config_for(url: &str) -> VoiceConfig {
    let mut config = VoiceConfig::new();
    config.set("whisper_url", json!(format!("{url}/inference")));
    config.set("stt_timeout_secs", json!(5.0));
    config
}

fn multipart_field(body: &[u8], name: &str) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    let marker = format!("name=\"{name}\"\r\n\r\n");
    let start = text.find(&marker)? + marker.len();
    let end = text[start..].find("\r\n--")? + start;
    Some(text[start..end].to_string())
}

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let path = std::env::temp_dir().join(format!("ez-stt-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&path).expect("carpeta temporal");
    path
}

fn cleanup(path: &Path) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let file = entry.path();
            if file.is_file() {
                let _ = std::fs::remove_file(file);
            }
        }
    }
    let _ = std::fs::remove_dir(path);
}

#[test]
fn transcribe_posts_the_form_and_returns_clean_text() {
    let (url, records, handle) =
        fake_server(vec![json_response(r#"{"text": "  abre claude code \n"}"#)]);
    let mut config = config_for(&url);
    config.set("stt_prompt", json!("EzAgent, OpenCode"));
    let text = transcribe(&config, b"RIFFwav", None).expect("transcripción");
    handle.join().expect("servidor falso");
    assert_eq!(text, "abre claude code");
    let requests = records.lock().expect("registro");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/inference");
    let body = &requests[0].body;
    assert_eq!(
        multipart_field(body, "model").as_deref(),
        Some("ggml-large-v3-turbo-q8_0")
    );
    assert_eq!(multipart_field(body, "language").as_deref(), Some("es"));
    assert_eq!(
        multipart_field(body, "response_format").as_deref(),
        Some("json")
    );
    assert_eq!(
        multipart_field(body, "temperature").as_deref(),
        Some("0.00")
    );
    assert_eq!(multipart_field(body, "beam_size").as_deref(), Some("5"));
    assert_eq!(
        multipart_field(body, "prompt").as_deref(),
        Some("EzAgent, OpenCode")
    );
    let text = String::from_utf8_lossy(body);
    assert!(text.contains("filename=\"utterance.wav\""));
    assert!(text.contains("Content-Type: audio/wav"));
    assert!(text.contains("RIFFwav"));
    let model_at = text.find("name=\"model\"").expect("modelo");
    let prompt_at = text.find("name=\"prompt\"").expect("prompt");
    assert!(model_at < prompt_at);
}

#[test]
fn vocabulary_hints_and_beam_size_follow_the_config() {
    let (url, records, handle) = fake_server(vec![json_response(r#"{"text": "hola"}"#)]);
    let mut config = config_for(&url);
    config.set("stt_vocabulary_hints", json!(false));
    config.set("stt_beam_size", json!(1));
    assert_eq!(transcribe(&config, b"x", None).expect("texto"), "hola");
    handle.join().expect("servidor falso");
    let requests = records.lock().expect("registro");
    let body = &requests[0].body;
    assert_eq!(multipart_field(body, "prompt"), None);
    assert_eq!(multipart_field(body, "beam_size"), None);
}

#[test]
fn beam_size_is_clamped_and_invalid_values_fall_back_to_five() {
    for (value, expected) in [
        (json!(3), Some("3")),
        (json!(50), Some("8")),
        (json!("raro"), Some("5")),
        (json!(-4), None),
    ] {
        let (url, records, handle) = fake_server(vec![json_response(r#"{"text": "ok"}"#)]);
        let mut config = config_for(&url);
        config.set("stt_beam_size", value);
        assert_eq!(transcribe(&config, b"x", None).expect("texto"), "ok");
        handle.join().expect("servidor falso");
        let requests = records.lock().expect("registro");
        assert_eq!(
            multipart_field(&requests[0].body, "beam_size").as_deref(),
            expected
        );
    }
}

#[test]
fn dictionary_corrects_the_result_and_primes_the_prompt() {
    let (url, records, handle) = fake_server(vec![json_response(r#"{"text": "abre open code"}"#)]);
    let mut config = config_for(&url);
    config.set("stt_dictionary", json!("open code → OpenCode"));
    assert_eq!(
        transcribe(&config, b"wav", None).expect("texto"),
        "abre OpenCode"
    );
    handle.join().expect("servidor falso");
    let requests = records.lock().expect("registro");
    let prompt = multipart_field(&requests[0].body, "prompt").expect("prompt");
    assert!(prompt.contains("OpenCode"));
}

#[test]
fn a_404_tries_the_openai_route() {
    let (url, records, handle) = fake_server(vec![
        (404, "no".to_string()),
        json_response(r#"{"text": "ok"}"#),
    ]);
    let config = config_for(&url);
    assert_eq!(transcribe(&config, b"x", None).expect("texto"), "ok");
    handle.join().expect("servidor falso");
    let requests = records.lock().expect("registro");
    let paths: Vec<&str> = requests
        .iter()
        .map(|request| request.path.as_str())
        .collect();
    assert_eq!(paths, ["/inference", "/v1/audio/transcriptions"]);
}

#[test]
fn a_plain_body_is_used_as_text() {
    let (url, _records, handle) = fake_server(vec![(200, " texto plano ".to_string())]);
    let config = config_for(&url);
    assert_eq!(
        transcribe(&config, b"x", None).expect("texto"),
        "texto plano"
    );
    handle.join().expect("servidor falso");
}

#[test]
fn invalid_dictionary_is_reported_with_the_python_text() {
    let mut config = VoiceConfig::new();
    config.set("stt_dictionary", json!("sin flecha"));
    let error = form(&config).expect_err("diccionario inválido");
    assert_eq!(
        error.message(),
        "Diccionario de transcripción inválido: Usa origen → corrección; separa las \
         reglas con punto y coma."
    );
}

#[test]
fn json_without_text_is_empty() {
    let (url, _records, handle) = fake_server(vec![json_response(r#"{"otra": 1}"#)]);
    let config = config_for(&url);
    assert_eq!(transcribe(&config, b"x", None).expect("texto"), "");
    handle.join().expect("servidor falso");
}

#[test]
fn server_errors_are_not_retried_and_show_the_body() {
    let (url, records, handle) = fake_server(vec![(503, "modelo no cargado".to_string())]);
    let config = config_for(&url);
    let error = transcribe(&config, b"x", None).expect_err("error del servidor");
    handle.join().expect("servidor falso");
    assert!(error.message().contains("respondió 503"));
    assert!(error.message().contains("modelo no cargado"));
    assert_eq!(records.lock().expect("registro").len(), 1);
}

#[test]
fn without_a_server_the_error_explains_how_to_start_it() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("puerto libre");
    let port = listener.local_addr().expect("puerto").port();
    drop(listener);
    let mut config = VoiceConfig::new();
    config.set(
        "whisper_url",
        json!(format!("http://127.0.0.1:{port}/inference")),
    );
    let error = transcribe(&config, b"x", None).expect_err("sin servidor");
    assert!(error.message().contains("No puedo conectar"));
    assert!(error.message().contains("start-whisper.ps1"));
}

#[test]
fn a_read_timeout_is_reported_and_not_retried() {
    let (url, records, _handle) = fake_server_delayed(
        vec![json_response(r#"{"text": "tarde"}"#)],
        Duration::from_millis(600),
    );
    let mut config = config_for(&url);
    config.set("stt_timeout_secs", json!(0.05));
    let error = transcribe(&config, b"x", None).expect_err("timeout");
    assert!(error.message().contains("no respondió a tiempo"));
    assert!(error.message().contains("/inference"));
    let _ = records;
}

#[test]
fn a_url_without_alternative_is_tried_once() {
    let (url, records, handle) = fake_server(vec![(404, "no".to_string())]);
    let mut config = VoiceConfig::new();
    config.set("whisper_url", json!(format!("{url}/custom")));
    let error = transcribe(&config, b"x", None).expect_err("404 sin alternativa");
    handle.join().expect("servidor falso");
    assert!(error.message().contains("No puedo conectar"));
    assert_eq!(records.lock().expect("registro").len(), 1);
}

#[test]
fn fallback_derivation_matches_the_python_routes() {
    assert_eq!(
        derive_fallback("http://h:1/inference").as_deref(),
        Some("http://h:1/v1/audio/transcriptions")
    );
    assert_eq!(
        derive_fallback("http://h:1/inference/").as_deref(),
        Some("http://h:1/v1/audio/transcriptions")
    );
    assert_eq!(
        derive_fallback("http://h:1/v1/audio/transcriptions").as_deref(),
        Some("http://h:1/inference")
    );
    assert_eq!(
        derive_fallback("http://h:1/api/inference?x=1").as_deref(),
        Some("http://h:1/api/v1/audio/transcriptions?x=1")
    );
    assert_eq!(derive_fallback("http://h:1/otra"), None);
    assert_eq!(derive_fallback("http://h:1"), None);
}

#[test]
fn the_working_endpoint_is_tried_first_and_forgotten() {
    close_client();
    let primary = "http://127.0.0.1:9/inference";
    let fallback = "http://127.0.0.1:9/v1/audio/transcriptions";
    remember_endpoint(primary, fallback);
    assert_eq!(endpoints(primary), [fallback, primary]);
    forget_endpoint(primary, fallback);
    assert_eq!(endpoints(primary), [primary, fallback]);
    close_client();
    assert_eq!(endpoints(primary), [primary, fallback]);
}

#[test]
fn a_connect_error_tries_the_alternative_route() {
    // Un puerto cerrado da error de conexión; el mensaje final menciona la
    // ruta alternativa, prueba de que se intentó después de la primaria.
    let listener = TcpListener::bind("127.0.0.1:0").expect("puerto libre");
    let port = listener.local_addr().expect("puerto").port();
    drop(listener);
    let mut config = VoiceConfig::new();
    config.set(
        "whisper_url",
        json!(format!("http://127.0.0.1:{port}/inference")),
    );
    config.set("stt_timeout_secs", json!(5.0));
    let error = transcribe(&config, b"x", None).expect_err("sin servidor");
    assert!(
        error.message().contains("/v1/audio/transcriptions"),
        "{}",
        error.message()
    );
}

#[test]
fn hallucinated_subtitle_phrases_are_always_discarded() {
    for text in [
        "¡Gracias por ver el vídeo!",
        "GRACIAS POR VER",
        "Suscríbete al canal.",
        "No olvides suscribirte",
        "Subtítulos realizados por la comunidad de Amara.org",
        "amara.org",
    ] {
        assert!(is_hallucination(text, false), "{text}");
        assert!(is_hallucination(text, true), "{text}");
    }
}

#[test]
fn courtesies_are_only_discarded_with_weak_audio() {
    for text in [
        "Gracias.",
        "muchas gracias",
        "Gracias a todos",
        "hasta luego",
        "Chao",
    ] {
        assert!(!is_hallucination(text, false), "{text}");
        assert!(is_hallucination(text, true), "{text}");
    }
}

#[test]
fn real_orders_are_never_discarded() {
    for text in [
        "",
        "   ",
        "...",
        "Gracias, ahora abre OpenCode",
        "Gracias por ver el vídeo que hice y revísalo",
        "Muchas gracias, ahora crea una carpeta nueva",
        "abre Claude Code",
        "dime la hora",
    ] {
        assert!(!is_hallucination(text, true), "{text}");
    }
}

#[test]
fn installed_models_are_sorted_by_quality_with_labels() {
    let folder = temp_dir("models");
    for name in [
        "ggml-large-v3-turbo-q8_0",
        "ggml-tiny-q5_1",
        "mi-modelo-raro",
        "ggml-base-q5_1",
    ] {
        std::fs::write(folder.join(format!("{name}.bin")), b"x").expect("modelo");
    }
    std::fs::write(folder.join("ggml-small-q5_1.txt"), b"no").expect("texto");
    let listed = available_in(Some(&folder), "");
    cleanup(&folder);
    let keys: Vec<&str> = listed.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "ggml-tiny-q5_1",
            "ggml-base-q5_1",
            "ggml-large-v3-turbo-q8_0",
            "mi-modelo-raro"
        ]
    );
    assert!(listed[2].1.ends_with("recomendada"));
    assert_eq!(listed[3].1, "Whisper mi modelo raro");
}

#[test]
fn the_current_model_is_listed_even_when_missing() {
    let listed = available_in(None, "ggml-medium-q5_0");
    assert_eq!(
        listed,
        [(
            "ggml-medium-q5_0".to_string(),
            "Whisper Medium · precisa".to_string()
        )]
    );
}

#[test]
fn model_paths_accept_names_with_or_without_extension() {
    let folder = temp_dir("path");
    let model = folder.join("ggml-base-q5_1.bin");
    std::fs::write(&model, b"x").expect("modelo");
    assert_eq!(
        model_path_in("ggml-base-q5_1", Some(&folder)),
        Some(model.clone())
    );
    assert_eq!(
        model_path_in("ggml-base-q5_1.bin", Some(&folder)),
        Some(model.clone())
    );
    assert_eq!(model_path_in("no-existe", Some(&folder)), None);
    assert_eq!(model_path_in(&model.to_string_lossy(), None), Some(model));
    cleanup(&folder);
    assert_eq!(whisper_model_path(""), None);
}

#[test]
fn home_selection_prefers_explicit_then_configured_then_default() {
    let first = temp_dir("home-a");
    let second = temp_dir("home-b");
    let fallback = temp_dir("home-c");
    assert_eq!(
        pick_home(Some(first.clone()), Some(second.clone()), fallback.clone()),
        Some(first.clone())
    );
    assert_eq!(
        pick_home(
            Some(first.join("no")),
            Some(second.clone()),
            fallback.clone()
        ),
        Some(second.clone())
    );
    assert_eq!(
        pick_home(Some(first.join("no")), None, fallback.clone()),
        Some(fallback.clone())
    );
    cleanup(&second);
    cleanup(&fallback);
    assert_eq!(pick_home(None, None, first.join("no")), None);
}

#[test]
fn url_parts_expose_host_and_port() {
    let parts = UrlParts::split("http://127.0.0.1:8080/inference?x=1");
    assert_eq!(parts.host().as_deref(), Some("127.0.0.1"));
    assert_eq!(parts.port(), Some(8080));
    assert_eq!(parts.path, "/inference");
    assert_eq!(parts.query, "x=1");
    let https = UrlParts::split("https://LOCALHOST/inference");
    assert_eq!(https.host().as_deref(), Some("localhost"));
    assert_eq!(https.default_port(), 443);
    let ipv6 = UrlParts::split("http://[::1]:9000/inference");
    assert_eq!(ipv6.host().as_deref(), Some("::1"));
    assert_eq!(ipv6.port(), Some(9000));
    assert_eq!(ipv6.unsplit(), "http://[::1]:9000/inference");
}

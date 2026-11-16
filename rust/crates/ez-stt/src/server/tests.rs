//! Pruebas de `server.rs`: puertos, sondeo HTTP y argumentos de arranque.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::*;

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "ez-stt-server-{tag}-{}-{nanos}",
        std::process::id()
    ));
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

fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("puerto libre");
    let port = listener.local_addr().expect("puerto").port();
    drop(listener);
    port
}

#[test]
fn resource_root_contains_the_whisper_script() {
    let script = resource_root().join("scripts").join("start-whisper.ps1");
    assert!(script.is_file(), "falta {}", script.display());
}

#[test]
fn resource_root_prefers_the_copy_beside_the_executable() {
    let base = temp_dir("root");
    let scripts = base.join("scripts");
    std::fs::create_dir_all(&scripts).expect("carpeta scripts");
    let script = scripts.join("start-whisper.ps1");
    std::fs::write(&script, "").expect("script de prueba");
    assert_eq!(resource_root_in(Some(&base)), base);
    let empty = temp_dir("root-empty");
    assert_eq!(resource_root_in(Some(&empty)), resource_root_in(None));
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_dir(&scripts);
    cleanup(&base);
    cleanup(&empty);
}

#[test]
fn a_free_local_port_is_available() {
    let port = free_local_port().expect("puerto libre");
    assert!(port > 0);
    let listener = TcpListener::bind(("127.0.0.1", port));
    assert!(listener.is_ok());
}

#[test]
fn only_loopback_hosts_are_auto_started() {
    assert!(is_local_host(Some("127.0.0.1")));
    assert!(is_local_host(Some("localhost")));
    assert!(is_local_host(Some("::1")));
    assert!(!is_local_host(Some("192.168.1.10")));
    assert!(!is_local_host(None));
}

#[test]
fn whisper_ready_reports_whether_the_port_accepts() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
    let port = listener.local_addr().expect("puerto").port();
    let url = format!("http://127.0.0.1:{port}/inference");
    assert!(whisper_ready_timeout(&url, Duration::from_millis(500)));
    drop(listener);
    let url = format!("http://127.0.0.1:{}/inference", closed_port());
    assert!(!whisper_ready_timeout(&url, Duration::from_millis(100)));
    assert!(!whisper_ready_timeout(
        "http:///inference",
        Duration::from_millis(10)
    ));
}

#[test]
fn the_local_url_keeps_the_path_and_query() {
    assert_eq!(
        url_for_local_port("http://host:123/api/inference?x=1", 8080),
        "http://127.0.0.1:8080/api/inference?x=1"
    );
    assert_eq!(
        url_for_local_port("https://host:123/inference", 8080),
        "https://127.0.0.1:8080/inference"
    );
    assert_eq!(
        url_for_local_port("/inference", 8080),
        "http://127.0.0.1:8080/inference"
    );
}

#[test]
fn start_arguments_match_the_powershell_script() {
    let script = Path::new("/repo/scripts/start-whisper.ps1");
    let home = Path::new("/whisper");
    let model = Path::new("/whisper/models/ggml-base-q5_1.bin");
    assert_eq!(
        start_args(script, 8088, "auto", Some(home), Some(model)),
        [
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            "/repo/scripts/start-whisper.ps1",
            "-Port",
            "8088",
            "-Engine",
            "auto",
            "-WhisperHome",
            "/whisper",
            "-Model",
            "/whisper/models/ggml-base-q5_1.bin",
        ]
    );
    assert_eq!(
        start_args(script, 9, "cpu", None, None),
        [
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            "/repo/scripts/start-whisper.ps1",
            "-Port",
            "9",
            "-Engine",
            "cpu",
        ]
    );
}

#[test]
fn a_whisper_home_needs_one_of_the_known_server_folders() {
    let home = temp_dir("server-home");
    assert!(!has_server(None));
    assert!(!has_server(Some(&home)));
    let cpu = home.join("cpu");
    std::fs::create_dir_all(&cpu).expect("carpeta cpu");
    std::fs::write(cpu.join("whisper-server.exe"), b"x").expect("binario");
    assert!(has_server(Some(&home)));
    cleanup(&cpu);
    cleanup(&home);
}

#[test]
fn a_ready_server_is_not_started() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
    let port = listener.local_addr().expect("puerto").port();
    let mut config = VoiceConfig::new();
    config.set(
        "whisper_url",
        json!(format!("http://127.0.0.1:{port}/inference")),
    );
    let root = temp_dir("ready");
    let process = ensure_whisper_at(&mut config, 0.1, &root);
    cleanup(&root);
    assert!(process.is_none());
}

#[test]
fn a_missing_script_does_not_start_anything() {
    let mut config = VoiceConfig::new();
    config.set(
        "whisper_url",
        json!(format!("http://127.0.0.1:{}/inference", closed_port())),
    );
    let root = temp_dir("no-script");
    let process = ensure_whisper_at(&mut config, 0.1, &root);
    cleanup(&root);
    assert!(process.is_none());
}

#[test]
fn auto_start_can_be_disabled() {
    let mut config = VoiceConfig::new();
    config.set("auto_start_whisper", json!(false));
    config.set(
        "whisper_url",
        json!(format!("http://127.0.0.1:{}/inference", closed_port())),
    );
    let root = temp_dir("no-auto");
    let process = ensure_whisper_at(&mut config, 0.1, &root);
    cleanup(&root);
    assert!(process.is_none());
}

#[test]
fn waiting_for_a_ready_server_returns_immediately() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
    let port = listener.local_addr().expect("puerto").port();
    let url = format!("http://127.0.0.1:{port}/inference");
    assert!(wait_until_ready(&url, 1.0));
}

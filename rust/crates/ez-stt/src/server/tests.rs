//! Pruebas de `server.rs`: puertos, sondeo HTTP, estado del servidor y
//! argumentos de arranque.

use std::cell::RefCell;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
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
    let _ = std::fs::remove_dir_all(path);
}

fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("puerto libre");
    let port = listener.local_addr().expect("puerto").port();
    drop(listener);
    port
}

/// Servidor HTTP mínimo que responde a `/health` con el código pedido.
struct HealthServer {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl HealthServer {
    fn start(status: u16) -> Self {
        let server = tiny_http::Server::http(("127.0.0.1", 0)).expect("servidor de salud");
        let port = server.server_addr().to_ip().expect("dirección IP").port();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let handle = std::thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                match server.recv_timeout(Duration::from_millis(20)) {
                    Ok(Some(request)) => {
                        let _ = request.respond(tiny_http::Response::empty(status));
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        });
        Self {
            port,
            stop,
            handle: Some(handle),
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/inference", self.port)
    }
}

impl Drop for HealthServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Sonda falsa: dice qué imagen tiene el PID y anota las bajas pedidas.
struct FakeProbe {
    image: Option<String>,
    terminated: RefCell<Vec<u32>>,
}

impl FakeProbe {
    fn seen(image: Option<&str>) -> Self {
        Self {
            image: image.map(str::to_string),
            terminated: RefCell::new(Vec::new()),
        }
    }
}

impl ProcessProbe for FakeProbe {
    fn image_name(&self, _pid: u32) -> Option<String> {
        self.image.clone()
    }

    fn terminate(&self, pid: u32) -> bool {
        self.terminated.borrow_mut().push(pid);
        true
    }
}

fn saved_state(path: &Path, pid: u32, url: &str, port: u16) {
    let state = ServerState {
        pid,
        port,
        url: url.to_string(),
    };
    write_state(path, &state).expect("estado de prueba");
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
fn the_health_url_is_the_server_root() {
    assert_eq!(
        health_url("http://127.0.0.1:8088/inference?x=1"),
        "http://127.0.0.1:8088/health"
    );
    assert_eq!(
        health_url("https://127.0.0.1:9/v1/audio/transcriptions"),
        "https://127.0.0.1:9/health"
    );
}

#[test]
fn health_uses_the_health_endpoint_and_falls_back_to_tcp() {
    let healthy = HealthServer::start(200);
    assert!(server_healthy(&healthy.url()));
    let old = HealthServer::start(404);
    assert!(server_healthy(&old.url()));
    assert!(!server_healthy(&format!(
        "http://127.0.0.1:{}/inference",
        closed_port()
    )));
}

#[test]
fn the_server_state_round_trips_and_is_removed() {
    let dir = temp_dir("state");
    let path = dir.join(STATE_FILE);
    assert!(read_state(&path).is_none());
    saved_state(&path, 4242, "http://127.0.0.1:9/inference", 9);
    assert_eq!(
        read_state(&path).map(|state| state.pid),
        Some(4242),
        "el estado se lee"
    );
    saved_state(&path, 5252, "http://127.0.0.1:10/inference", 10);
    assert_eq!(
        read_state(&path).map(|state| state.pid),
        Some(5252),
        "se reemplaza el estado anterior"
    );
    remove_state(&path);
    assert!(!path.exists());
    cleanup(&dir);
}

#[test]
fn a_healthy_saved_server_is_reused() {
    let health = HealthServer::start(200);
    let dir = temp_dir("reuse");
    let path = dir.join(STATE_FILE);
    let url = health.url();
    saved_state(&path, 1234, &url, health.port);
    let probe = FakeProbe::seen(Some("whisper-server.exe"));
    let root = temp_dir("reuse-root");
    let mut config = VoiceConfig::new();
    config.set(
        "whisper_url",
        json!(format!("http://127.0.0.1:{}/inference", closed_port())),
    );
    let child = ensure_whisper_in(&mut config, 0.1, &root, &path, None, None, &probe);
    assert!(child.is_none(), "no se lanza otro servidor");
    assert_eq!(config_text(&config, "whisper_url"), url);
    assert!(probe.terminated.borrow().is_empty());
    assert!(path.exists(), "el estado sano se conserva");
    cleanup(&root);
    cleanup(&dir);
}

#[test]
fn a_saved_server_that_does_not_respond_is_closed_and_replaced() {
    let dir = temp_dir("hung");
    let path = dir.join(STATE_FILE);
    let url = format!("http://127.0.0.1:{}/inference", closed_port());
    saved_state(&path, 4321, &url, 9);
    let probe = FakeProbe::seen(Some("whisper-server.exe"));
    let root = temp_dir("hung-root");
    let mut config = VoiceConfig::new();
    config.set("whisper_url", json!(url));
    let child = ensure_whisper_in(&mut config, 0.1, &root, &path, None, None, &probe);
    assert!(child.is_none());
    assert_eq!(*probe.terminated.borrow(), vec![4321]);
    assert!(!path.exists(), "el estado inservible se retira");
    cleanup(&root);
    cleanup(&dir);
}

#[test]
fn a_saved_pid_from_another_program_is_never_terminated() {
    let dir = temp_dir("foreign");
    let path = dir.join(STATE_FILE);
    let url = format!("http://127.0.0.1:{}/inference", closed_port());
    saved_state(&path, 9999, &url, 9);
    let probe = FakeProbe::seen(Some("notepad.exe"));
    let root = temp_dir("foreign-root");
    let mut config = VoiceConfig::new();
    config.set("whisper_url", json!(url));
    let child = ensure_whisper_in(&mut config, 0.1, &root, &path, None, None, &probe);
    assert!(child.is_none());
    assert!(probe.terminated.borrow().is_empty());
    assert!(!path.exists());
    cleanup(&root);
    cleanup(&dir);
}

#[test]
fn the_server_is_searched_in_the_script_order() {
    let home = temp_dir("server-home");
    assert!(resolve_server(Some(&home), "auto").is_none());
    let cpu = home.join("cpu");
    std::fs::create_dir_all(&cpu).expect("carpeta cpu");
    let cpu_exe = cpu.join(SERVER_EXE);
    std::fs::write(&cpu_exe, b"x").expect("binario");
    assert_eq!(resolve_server(Some(&home), "auto"), Some(cpu_exe.clone()));
    let root_exe = home.join(SERVER_EXE);
    std::fs::write(&root_exe, b"x").expect("binario");
    assert_eq!(resolve_server(Some(&home), "auto"), Some(root_exe));
    assert_eq!(resolve_server(Some(&home), "cpu"), Some(cpu_exe));
    cleanup(&home);
}

#[test]
fn the_model_falls_back_to_the_script_defaults() {
    let home = temp_dir("model-home");
    let models = home.join("models");
    std::fs::create_dir_all(&models).expect("carpeta models");
    let q5 = models.join(DEFAULT_MODELS[1]);
    std::fs::write(&q5, b"x").expect("modelo");
    assert_eq!(resolve_model(None, Some(&home)), Some(q5.clone()));
    let explicit = temp_dir("model-explicit");
    let model = explicit.join("modelo.bin");
    std::fs::write(&model, b"x").expect("modelo");
    assert_eq!(resolve_model(Some(&model), Some(&home)), Some(model));
    assert_eq!(resolve_model(None, None), None);
    cleanup(&explicit);
    cleanup(&home);
}

#[test]
fn whisper_server_arguments_follow_the_script() {
    let server = Path::new("/whisper/whisper-server.exe");
    let model = Path::new("/whisper/models/m.bin");
    assert_eq!(
        server_args(9000, "auto", server, model),
        [
            "-m",
            "/whisper/models/m.bin",
            "--host",
            "127.0.0.1",
            "--port",
            "9000",
            "-l",
            "es",
            "-t",
            "6",
        ]
    );
    let cpu_server = Path::new("/whisper/cpu/whisper-server.exe");
    assert!(server_args(1, "auto", cpu_server, model).contains(&"-ng".to_string()));
    let dir = temp_dir("vad");
    let vad_model = dir.join("m.bin");
    std::fs::write(&vad_model, b"x").expect("modelo");
    std::fs::write(dir.join(VAD_MODEL), b"x").expect("vad");
    let args = server_args(1, "auto", server, &vad_model);
    assert!(args.contains(&"--vad".to_string()));
    assert!(args.contains(&dir.join(VAD_MODEL).to_string_lossy().into_owned()));
    cleanup(&dir);
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
    let state = root.join(STATE_FILE);
    let probe = FakeProbe::seen(None);
    let process = ensure_whisper_in(&mut config, 0.1, &root, &state, None, None, &probe);
    cleanup(&root);
    assert!(process.is_none());
}

#[test]
fn a_missing_server_does_not_start_anything() {
    let mut config = VoiceConfig::new();
    config.set(
        "whisper_url",
        json!(format!("http://127.0.0.1:{}/inference", closed_port())),
    );
    let root = temp_dir("no-server");
    let state = root.join(STATE_FILE);
    let probe = FakeProbe::seen(None);
    // Sin carpeta ni modelo inyectados: nunca mira la instalación real.
    let process = ensure_whisper_in(&mut config, 0.1, &root, &state, None, None, &probe);
    cleanup(&root);
    assert!(process.is_none());
    assert!(!state.exists());
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
    let state = root.join(STATE_FILE);
    let probe = FakeProbe::seen(None);
    let process = ensure_whisper_in(&mut config, 0.1, &root, &state, None, None, &probe);
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

#[test]
fn the_missing_whisper_hint_names_a_command_that_exists_there() {
    assert!(super::missing_whisper_hint(true).ends_with("ezagent-voice downloads --all"));
    assert!(super::missing_whisper_hint(false)
        .ends_with("uv run python -m ezagent.core.downloads --all"));
}

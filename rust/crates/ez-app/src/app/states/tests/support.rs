//! Dobles compartidos por las pruebas de F6-04: altavoz que apunta y un
//! cliente del LLM con guion. Ninguna prueba abre micrófono, red ni altavoz.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use ez_llm::errors::LlmError;
use ez_llm::messenger::LlmClient;
use ez_llm::openai::{ChatStream, Chunk};
use parking_lot::Mutex;
use serde_json::{json, Value};

use crate::app::core::{AppEvent, AppParts, SpeakerApi, VoiceApp};

/// Altavoz de prueba: recuerda síntesis y frases dichas.
pub(crate) struct RecordingSpeaker {
    pub synthesized: Mutex<Vec<String>>,
    pub spoken: Mutex<Vec<String>>,
    pub plays: AtomicUsize,
    pub fail: Mutex<Option<String>>,
}

impl RecordingSpeaker {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            synthesized: Mutex::new(Vec::new()),
            spoken: Mutex::new(Vec::new()),
            plays: AtomicUsize::new(0),
            fail: Mutex::new(None),
        })
    }
}

impl SpeakerApi for RecordingSpeaker {
    fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32), String> {
        if let Some(error) = self.fail.lock().clone() {
            return Err(error);
        }
        self.synthesized.lock().push(text.to_string());
        Ok((vec![0.1; 240], 24_000))
    }

    fn play(
        &self,
        _samples: &[f32],
        _sample_rate: u32,
        stop: &AtomicBool,
        mut on_level: Option<&mut dyn FnMut(f64)>,
        mut on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        self.plays.fetch_add(1, Ordering::SeqCst);
        if stop.load(Ordering::SeqCst) {
            return Ok(false);
        }
        if let Some(start) = on_start.as_mut() {
            start();
        }
        if let Some(level) = on_level.as_mut() {
            level(0.25);
        }
        Ok(true)
    }

    fn speak(
        &self,
        text: &str,
        stop: &AtomicBool,
        mut on_level: Option<&mut dyn FnMut(f64)>,
        mut on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        if let Some(error) = self.fail.lock().clone() {
            return Err(error);
        }
        if stop.load(Ordering::SeqCst) {
            return Ok(false);
        }
        self.spoken.lock().push(text.to_string());
        if let Some(start) = on_start.as_mut() {
            start();
        }
        if let Some(level) = on_level.as_mut() {
            level(0.25);
        }
        Ok(true)
    }
}

/// Una respuesta guionada del cliente del LLM.
pub(crate) enum Script {
    Chunks(Vec<Chunk>),
    Fail(LlmError),
}

/// Cliente del LLM con respuestas prefijadas, sin red.
pub(crate) struct ScriptedClient {
    scripts: Mutex<VecDeque<Script>>,
}

impl ScriptedClient {
    pub(crate) fn new(scripts: Vec<Script>) -> Arc<Self> {
        Arc::new(Self {
            scripts: Mutex::new(scripts.into()),
        })
    }
}

impl LlmClient for ScriptedClient {
    fn stream(&self, _request: &Value) -> Result<Box<dyn ChatStream>, LlmError> {
        match self.scripts.lock().pop_front() {
            Some(Script::Chunks(chunks)) => Ok(Box::new(VecStream {
                chunks: chunks.into(),
            })),
            Some(Script::Fail(error)) => Err(error),
            None => Ok(Box::new(VecStream {
                chunks: VecDeque::new(),
            })),
        }
    }

    fn close(&self) {}
}

struct VecStream {
    chunks: VecDeque<Chunk>,
}

impl Iterator for VecStream {
    type Item = Result<Chunk, LlmError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.chunks.pop_front().map(Ok)
    }
}

impl ChatStream for VecStream {
    fn close(&mut self) {
        self.chunks.clear();
    }
}

/// Trozo de texto del asistente.
pub(crate) fn chunk(text: &str) -> Chunk {
    Chunk {
        content: Some(text.to_string()),
        ..Chunk::default()
    }
}

/// App con dobles, chat sin disco y micrófono que nunca abre un dispositivo.
pub(crate) struct Fixture {
    pub app: VoiceApp,
    pub speaker: Arc<RecordingSpeaker>,
    config_file: PathBuf,
    tasks_path: PathBuf,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        Self::with_config(test_config())
    }

    pub(crate) fn with_config(cfg: VoiceConfig) -> Self {
        let mic = Arc::new(Mutex::new(MicCapture::new(None, VadSettings::default())));
        mic.lock().set_device_index(None);
        let speaker = RecordingSpeaker::new();
        let config_file = temp_path("voice.json");
        let tasks_path = temp_path("tasks.json");
        let mut parts = AppParts::new(
            Arc::clone(&speaker) as Arc<dyn SpeakerApi>,
            Arc::clone(&mic),
        );
        parts.chat = Some(Arc::new(ChatModel::new(None, true)));
        parts.config_file = Some(config_file.clone());
        let app = VoiceApp::with_services(cfg, Some(tasks_path.clone()), parts);
        Self {
            app,
            speaker,
            config_file,
            tasks_path,
        }
    }

    pub(crate) fn saved_config(&self) -> String {
        std::fs::read_to_string(&self.config_file).unwrap_or_default()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.tasks_path);
        let _ = std::fs::remove_file(&self.config_file);
    }
}

/// Sustituye el cliente HTTP del mensajero por el guion.
pub(crate) fn install_client(app: &VoiceApp, client: Arc<ScriptedClient>) {
    if let Some(messenger) = app.inner.agents.messenger.lock().as_ref() {
        messenger.set_client(client);
    }
}

/// Vacía la cola de eventos de la app.
pub(crate) fn drain_events(app: &VoiceApp) -> Vec<AppEvent> {
    std::iter::from_fn(|| app.events().get_nowait()).collect()
}

/// Espera hasta `timeout` a que llegue un evento que cumpla `matches`.
pub(crate) fn wait_event(
    app: &VoiceApp,
    mut matches: impl FnMut(&AppEvent) -> bool,
) -> Vec<AppEvent> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        while let Some(event) = app.events().get_nowait() {
            let found = matches(&event);
            seen.push(event);
            if found {
                return seen;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    seen
}

/// Servidor HTTP de un solo uso que responde `{"text": ...}` como whisper.
pub(crate) fn stt_server(text: &str) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener de whisper");
    let address = listener.local_addr().expect("dirección del listener");
    let body = json!({ "text": text }).to_string();
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        read_request(&mut stream);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
    });
    (format!("http://{address}/inference"), handle)
}

fn read_request(stream: &mut std::net::TcpStream) {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        if let Some(index) = find_bytes(&buffer, b"\r\n\r\n") {
            break index + 4;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
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
    let mut body = buffer.len().saturating_sub(header_end);
    while body < length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => body += read,
        }
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn test_config() -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(false));
    cfg.set("smart_turn", json!(false));
    cfg.set("chat_voice", json!(true));
    cfg.set("llm_provider", json!("openai"));
    cfg.set("llm_model", json!("gpt-prueba"));
    cfg
}

fn temp_path(name: &str) -> PathBuf {
    let unique = format!(
        "ezagent-f6-04-{}-{}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    );
    std::env::temp_dir().join(unique)
}

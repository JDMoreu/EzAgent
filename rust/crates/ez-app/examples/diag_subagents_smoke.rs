//! NV-3b: anuncios de los subagentes en la app real, sin micrófono.
//!
//! Monta la app (`VoiceApp`) con dobles de micrófono y altavoz, apunta
//! `APPDATA`, `EZAGENT_LOGS_DIR` y `EZ_SUBAGENTS_DIR` a una carpeta temporal y
//! usa el `fake-hc` de `ez-fakes` con dos hijos falsos registrados bajo `voz`:
//! uno parado con «Free limit reached» y otro muerto sin informe. El vigilante
//! de la app los marca `bloqueado` y `fallido`; nadie escribe esos estados a
//! mano. Los anuncios tienen que aparecer en el chat.
//!
//! Al final comprueba que el registro temporal no tenga ningún `PÁNICO`. No
//! abre micrófono, no habla de verdad, no lanza agentes reales y no toca la
//! carpeta, los registros ni la ventana de Herdr de la persona.
//!
//! ```text
//! cargo build -p ez-fakes --bin fake-hc
//! cargo build -p ez-app --example diag_subagents_smoke
//! ./target/debug/examples/diag_subagents_smoke.exe
//! ```

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_app::app::core::{AppParts, SpeakerApi, SubagentWatch, VoiceApp};
use ez_app::app::live::LiveHooks;
use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use ez_herdr::herdr::system_runtime;
use ez_herdr::subagents::{epoch_ms, Change, HcRuntime, Record, Registry, TaskState};
use serde_json::json;

/// Altavoz mudo: el ejemplo no carga Kokoro.
struct SilentSpeaker;

impl SpeakerApi for SilentSpeaker {
    fn synthesize(&self, _: &str) -> Result<(Vec<f32>, u32), String> {
        Ok((Vec::new(), 16_000))
    }

    fn play(
        &self,
        _: &[f32],
        _: u32,
        _: &AtomicBool,
        _: Option<&mut dyn FnMut(f64)>,
        _: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        Ok(true)
    }

    fn speak(
        &self,
        _: &str,
        _: &AtomicBool,
        _: Option<&mut dyn FnMut(f64)>,
        _: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        Ok(true)
    }
}

fn temp_dir() -> PathBuf {
    let unique = format!(
        "ezagent-nv3-humo-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    );
    std::env::temp_dir().join(unique)
}

fn seed_config(dir: &Path, fake_hc: &Path, fake_state: &Path) -> std::io::Result<()> {
    let config = json!({
        // DECISION: sin barra visible; la prueba mira el chat y la voz, no el HUD.
        "hud_hidden": true,
        "smart_turn": false,
        "chat_voice": false,
        "hc_path": fake_hc.to_string_lossy(),
        "herdr_app_path": fake_hc.to_string_lossy(),
        "default_cwd": dir.to_string_lossy(),
        "fake_hc_state": fake_state.to_string_lossy()
    });
    std::fs::write(
        dir.join("voice.json"),
        serde_json::to_vec_pretty(&config).unwrap_or_default(),
    )
}

/// El `fake-hc` que deja `cargo build -p ez-fakes --bin fake-hc`.
fn fake_hc_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let folder = exe.parent()?;
    [
        folder.join("fake-hc.exe"),
        folder.join("../fake-hc.exe"),
        folder.join("../../fake-hc.exe"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

/// El estado del `fake-hc`: un hijo vivo con el límite en pantalla y otro muerto.
fn seed_fake_hc(path: &Path) -> std::io::Result<()> {
    let state = json!({
        "next": 2,
        "spaces": ["main"],
        "terminals": [
            {
                "address": "t1",
                "tab": "tab1",
                "space": "main",
                "name": "ir-1",
                "title": "ir-1",
                "agent": "opencode",
                "state": "live",
                "cwd": "C:\\humo",
                "where": "main / ir-1",
                "screen": "Free limit reached. Resets at 7:00 p.m.\n"
            },
            {
                "address": "t2",
                "tab": "tab2",
                "space": "main",
                "name": "ir-2",
                "title": "ir-2",
                "agent": "opencode",
                "state": "finished",
                "cwd": "C:\\humo",
                "where": "main / ir-2",
                "screen": "",
                "exit_code": 1
            }
        ]
    });
    std::fs::write(path, serde_json::to_vec_pretty(&state).unwrap_or_default())
}

/// Un registro que la app dejó al lanzar al hijo falso (`by = voz`).
fn fake_record(name: &str, terminal: &str) -> Record {
    let now = epoch_ms();
    Record {
        name: name.to_string(),
        terminal: terminal.to_string(),
        parent: "voz".to_string(),
        agent: "opencode".to_string(),
        label: "OpenCode".to_string(),
        model: Some("glm-5.3".to_string()),
        effort: None,
        role: "voz".to_string(),
        cwd: "C:\\humo".to_string(),
        brief_head: "Revisa la rama work/NV-3.".to_string(),
        brief_path: None,
        retry_of: None,
        state: TaskState::InProgress,
        since_ms: now,
        history: vec![Change {
            state: TaskState::InProgress,
            at_ms: now,
            by: "voz".to_string(),
            detail: "encargo entregado".to_string(),
        }],
        summary: None,
        report: None,
        validation: None,
        notified: None,
    }
}

fn notes(chat: &ChatModel) -> Vec<String> {
    chat.snapshot()
        .1
        .into_iter()
        .map(|message| message.text)
        .collect()
}

/// Late la app hasta que el chat tenga `needle` (o se acabe el tiempo).
fn wait_for_note(app: &VoiceApp, chat: &ChatModel, needle: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        app.activity_tick();
        app.tick();
        if notes(chat).iter().any(|text| text.contains(needle)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    notes(chat).iter().any(|text| text.contains(needle))
}

fn panic_lines(logs: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(logs) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("log")
            && !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("ezagent.log"))
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        found.extend(
            text.lines()
                .filter(|line| line.contains("PÁNICO"))
                .map(str::to_string),
        );
    }
    found
}

fn main() {
    let temp = temp_dir();
    let logs = temp.join("logs");
    let subagents = temp.join("subagentes");
    let _ = std::fs::create_dir_all(&logs);
    let _ = std::fs::create_dir_all(&subagents);
    // La app lee `%APPDATA%\EzAgent`: con una carpeta temporal no se toca la
    // configuración, el historial, la latencia ni el registro de la persona.
    std::env::set_var("APPDATA", &temp);
    std::env::set_var("EZAGENT_LOGS_DIR", &logs);
    std::env::set_var("EZ_SUBAGENTS_DIR", &subagents);
    ez_core::panics::install_hook_in(&logs);
    let Some(fake_hc) = fake_hc_binary() else {
        eprintln!(
            "No encuentro fake-hc.exe; compílalo con `cargo build -p ez-fakes --bin fake-hc`."
        );
        std::process::exit(1);
    };
    let fake_state = temp.join("fake-hc.json");
    if let Err(error) = seed_fake_hc(&fake_state) {
        eprintln!("No pude escribir el estado del fake-hc: {error}");
        std::process::exit(1);
    }
    // `fake-hc` lee y guarda su estado aquí; el vigilante lo hereda como hijo.
    std::env::set_var("FAKE_HC_STATE", &fake_state);
    if let Err(error) = seed_config(&temp, &fake_hc, &fake_state) {
        eprintln!("No pude escribir la configuración sintética: {error}");
        std::process::exit(1);
    }

    let cfg = match VoiceConfig::load(Some(&temp.join("voice.json"))) {
        Ok(cfg) => cfg,
        Err(error) => {
            eprintln!("No pude cargar la configuración: {error}");
            std::process::exit(1);
        }
    };
    let chat = Arc::new(ChatModel::new(None, false));
    let mut parts = AppParts::new(
        Arc::new(SilentSpeaker),
        Arc::new(parking_lot::Mutex::new(MicCapture::silent(
            VadSettings::default(),
        ))),
    );
    // DECISION: sin hilos de fondo, atajos ni agente: el ejemplo solo quiere el
    // vigilante, los anuncios y el chat (mismo estilo que `diag_chat_smoke`).
    parts.background = false;
    parts.build_swarm = false;
    parts.build_agent = false;
    parts.mic_capture = false;
    parts.whisper_ready = true;
    parts.chat = Some(Arc::clone(&chat));
    parts.config_file = Some(temp.join("voice.json"));
    parts.subagent_watch = Some(SubagentWatch {
        runtime: Arc::new(HcRuntime::new(
            system_runtime(),
            fake_hc.to_string_lossy().into_owned(),
        )),
        novelty_file: Some(temp.join("novedades.toml")),
        interval_ms: 100,
        grace_ms: 0,
    });

    let app = VoiceApp::with_services(cfg, None, parts);
    app.set_hooks(Arc::new(LiveHooks::new(&app)));
    eprintln!(
        "app viva; fake-hc {}; registros en {}",
        fake_hc.display(),
        subagents.display()
    );

    let registry = Registry::at(&subagents);
    for record in [fake_record("ir-1", "t1"), fake_record("ir-2", "t2")] {
        if let Err(error) = registry.create(&record) {
            eprintln!(
                "No pude crear el registro de {}: {}",
                record.name,
                error.message()
            );
            std::process::exit(1);
        }
    }
    eprintln!("registros: ir-1 (t1, límite) e ir-2 (t2, muerto) en_proceso");

    // Nadie escribe `bloqueado`/`fallido`: lo hace el vigilante de la app.
    let blocked = wait_for_note(&app, &chat, "está bloqueado");
    if let Some(text) = notes(&chat)
        .into_iter()
        .find(|text| text.contains("está bloqueado"))
    {
        eprintln!("ANUNCIO bloqueado: {text}");
    }
    let failed = wait_for_note(&app, &chat, "falló");
    if let Some(text) = notes(&chat).into_iter().find(|text| text.contains("falló")) {
        eprintln!("ANUNCIO fallido: {text}");
    }
    let state_of = |name: &str| {
        registry
            .get("voz", name)
            .ok()
            .flatten()
            .map(|record| {
                format!(
                    "{} ({})",
                    record.state.as_str(),
                    record
                        .history
                        .last()
                        .map(|change| change.by.clone())
                        .unwrap_or_default()
                )
            })
            .unwrap_or_else(|| "sin registro".to_string())
    };
    eprintln!(
        "estado real: ir-1 = {}; ir-2 = {}",
        state_of("ir-1"),
        state_of("ir-2")
    );
    let offer = app.pending_retry();
    eprintln!("oferta de reintento: {offer:?}");

    let status = app.fast_command("¿cómo va ir-1?");
    eprintln!("atajo «¿cómo va ir-1?»: handled={status}");
    let alive = !app.state().is_empty();
    eprintln!("app viva al final: {alive}");

    app.shutdown();
    let panics = panic_lines(&logs);
    eprintln!("PÁNICO en el registro temporal: {}", panics.len());
    for line in &panics {
        eprintln!("  {line}");
    }
    let ok = blocked
        && failed
        && offer.as_deref() == Some("ir-1")
        && status
        && alive
        && panics.is_empty();
    eprintln!(
        "RESULT bloqueado={blocked} fallido={failed} oferta={} atajo={status} viva={alive} panico={}",
        offer.as_deref().unwrap_or("ninguna"),
        panics.len()
    );
    let _ = std::fs::remove_dir_all(&temp);
    if !ok {
        std::process::exit(1);
    }
}

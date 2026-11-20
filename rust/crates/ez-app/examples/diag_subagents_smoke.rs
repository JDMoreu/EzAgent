//! NV-3: anuncios de los subagentes en la app real, sin micrófono.
//!
//! Monta la app (`VoiceApp`) con dobles de micrófono y altavoz, apunta
//! `APPDATA`, `EZAGENT_LOGS_DIR` y `EZ_SUBAGENTS_DIR` a una carpeta temporal y
//! escribe registros falsos del padre `voz` que cambian de estado:
//! `en_proceso` → `bloqueado` por límite → `terminado`.
//!
//! La app vive, el latido de actividad (`activity_tick`) lee el registro y los
//! anuncios tienen que aparecer en el chat. Al final comprueba que el registro
//! temporal no tenga ningún `PÁNICO`. No abre micrófono, no habla de verdad y
//! no toca la carpeta ni los subagentes de la persona.
//!
//! ```text
//! cargo run -p ez-app --example diag_subagents_smoke
//! ```

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_app::app::core::{AppParts, SpeakerApi, VoiceApp};
use ez_app::app::live::LiveHooks;
use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_core::config::VoiceConfig;
use ez_herdr::subagents::{epoch_ms, Change, Record, Registry, TaskState};
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

fn seed_config(dir: &Path) -> std::io::Result<()> {
    let config = json!({
        // DECISION: sin barra visible; la prueba mira el chat y la voz, no el HUD.
        "hud_hidden": true,
        "smart_turn": false,
        "chat_voice": false
    });
    std::fs::write(
        dir.join("voice.json"),
        serde_json::to_vec_pretty(&config).unwrap_or_default(),
    )
}

fn fake_record() -> Record {
    let now = epoch_ms();
    Record {
        name: "ir-1".to_string(),
        terminal: "t-falso".to_string(),
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
            by: "vigilante".to_string(),
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
    if let Err(error) = seed_config(&temp) {
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
    // camino de los anuncios y el chat (mismo estilo que `diag_chat_smoke`).
    parts.background = false;
    parts.build_swarm = false;
    parts.build_agent = false;
    parts.mic_capture = false;
    parts.whisper_ready = true;
    parts.chat = Some(Arc::clone(&chat));
    parts.config_file = Some(temp.join("voice.json"));

    let app = VoiceApp::with_services(cfg, None, parts);
    app.set_hooks(Arc::new(LiveHooks::new(&app)));
    eprintln!("app viva; registros en {}", subagents.display());

    let registry = Registry::at(&subagents);
    if let Err(error) = registry.create(&fake_record()) {
        eprintln!("No pude crear el registro falso: {}", error.message());
        std::process::exit(1);
    }
    app.activity_tick();
    app.tick();
    eprintln!(
        "estado inicial: ir-1 en_proceso; anuncios={}",
        notes(&chat).len()
    );

    if let Err(error) = registry.transition(
        "voz",
        "ir-1",
        TaskState::Blocked,
        "vigilante",
        "límite de uso del plan gratuito (se reinicia a las 7:00 p.m.)",
        |_| {},
    ) {
        eprintln!("No pude marcar el bloqueo: {}", error.message());
        std::process::exit(1);
    }
    let blocked = wait_for_note(&app, &chat, "está bloqueado");
    if let Some(text) = notes(&chat)
        .into_iter()
        .find(|text| text.contains("está bloqueado"))
    {
        eprintln!("ANUNCIO bloqueado: {text}");
    }

    if let Err(error) = registry.transition(
        "voz",
        "ir-1",
        TaskState::Done,
        "ez report",
        "informe entregado",
        |record| {
            record.summary = Some("Rama NV-3 en verde.".to_string());
            record.report = Some("rust-migration/reports/NV-3-r1.md".to_string());
        },
    ) {
        eprintln!("No pude marcar el término: {}", error.message());
        std::process::exit(1);
    }
    let done = wait_for_note(&app, &chat, "terminó");
    if let Some(text) = notes(&chat)
        .into_iter()
        .find(|text| text.contains("terminó"))
    {
        eprintln!("ANUNCIO terminado: {text}");
    }

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
    let ok = blocked && done && status && alive && panics.is_empty();
    eprintln!(
        "RESULT bloqueado={blocked} terminado={done} atajo={status} viva={alive} panico={}",
        panics.len()
    );
    let _ = std::fs::remove_dir_all(&temp);
    if !ok {
        std::process::exit(1);
    }
}

//! DX-1d: abre el chat de la app real (por el camino del atajo) y comprueba
//! que la app sigue viva.
//!
//! La app se cayó al abrir el chat (Windows: `0xc0000409`, el aborto de Rust).
//! Este ejemplo monta la app **real** (`VoiceApp`, la barra y el panel del
//! chat con su ventana nativa) con un historial **sintético** en una carpeta
//! temporal, abre el chat por el mismo camino que el atajo (`handle_hotkey
//! ("chat")` → `toggle_chat` → `ChatPanel::open`) y espera diez segundos.
//!
//! - No hay micrófono: la captura es la de prueba (`MicCapture::silent`).
//! - No hay altavoz: el `SpeakerApi` es mudo.
//! - No hay atajos globales, ni enjambre, ni agente: `background = false`.
//! - La ventana del chat sí es real (por eso el camino del `WndProc` se
//!   ejercita de verdad).
//!
//! ```text
//! cargo run --release -p ez-app --example diag_chat_smoke
//! ```
//!
//! Si el proceso se cae, el ejemplo muere con él: eso es la reproducción.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_app::app::core::{AppParts, SpeakerApi, VoiceApp};
use ez_app::app::live::LiveHooks;
use ez_audio::capture::{MicCapture, VadSettings};
use ez_chat::session::ChatModel;
use ez_chat::store::ChatStore;
use ez_core::config::VoiceConfig;

/// Segundos que el chat tiene que seguir vivo.
const ALIVE_SECS: u64 = 10;

/// Altavoz mudo: el ejemplo no necesita Kokoro ni la DLL de ONNX.
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

/// Un texto largo sin espacios.
fn long_word() -> String {
    "x".repeat(2_000)
}

/// Un bloque de markdown con todo lo que puede romper la maquetación.
fn rich() -> String {
    format!(
        "## Encabezado\n\n\
         Un párrafo con **negrita**, *cursiva*, `código` y un [enlace](https://example.com).\n\n\
         - Lista de primer nivel\n\
         - Lista de segundo nivel\n\n\
         > Una cita.\n\n\
         ```rust\n\
         fn main() {{\n\
         \x20   println!(\"hola\");\n\
         }}\n\
         ```\n\n\
         | Caso | Antes | Después |\n\
         |---|---|---|\n\
         | 10 | 3 ms | 1 ms |\n\n\
         ![una imagen](C:\\no\\existe\\imagen.png)\n\n\
         Emojis: 🦙 🧑‍💻 🇦🇷 ✅ 🎉\n\n\
         {}\n",
        long_word()
    )
}

/// El historial sintético, en el JSONL que escribe la app.
fn history_jsonl() -> String {
    let mut texts: Vec<(&str, String)> = vec![
        ("user", "Revisa la rama `work/DX-1`.".to_string()),
        ("assistant", rich()),
        ("user", String::new()),
        ("assistant", String::new()),
        ("user", long_word()),
        ("assistant", "Una respuesta corta.".to_string()),
        ("system", "Conversación compactada.".to_string()),
        ("user", "Emojis 🦙 y acentos: ñáéíóú ¿? ¡!".to_string()),
        ("assistant", rich()),
    ];
    // Hasta 35 mensajes, como el historial que tenía la persona.
    let mut index = texts.len();
    while texts.len() < 35 {
        let role = match index % 3 {
            0 => "user",
            1 => "assistant",
            _ => "system",
        };
        let text = match index % 4 {
            0 => rich(),
            1 => format!("Mensaje {index} con un poco de texto normal."),
            2 => String::new(),
            _ => long_word(),
        };
        texts.push((role, text));
        index += 1;
    }
    let mut out = String::new();
    out.push_str("{\"v\":1,\"t\":1000.0,\"event\":\"session\",\"session\":1,\"seq\":1}\n");
    for (number, (role, text)) in texts.iter().enumerate() {
        let seq = number as i64 + 2;
        let escaped = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
        out.push_str(&format!(
            "{{\"v\":1,\"t\":{},\"session\":1,\"role\":\"{role}\",\"source\":\"chat\",\"text\":{escaped},\"seq\":{seq}}}\n",
            1000.0 + seq as f64
        ));
    }
    out
}

/// Escribe el historial sintético en una carpeta temporal y devuelve la ruta.
fn seed_history(dir: &std::path::Path) -> std::io::Result<PathBuf> {
    let history = dir.join("history");
    std::fs::create_dir_all(&history)?;
    let path = history.join("chat.jsonl");
    std::fs::write(&path, history_jsonl())?;
    Ok(path)
}

fn main() {
    let temp = std::env::temp_dir().join(format!("ezagent-dx1d-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp);
    let history = match seed_history(&temp) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("No pude escribir el historial sintético: {error}");
            std::process::exit(1);
        }
    };
    // La app lee `%APPDATA%\EzAgent`: con una carpeta temporal no se toca el
    // historial, la configuración ni el registro de la persona.
    std::env::set_var("APPDATA", &temp);

    let cfg = match VoiceConfig::load(Some(&temp.join("voice.json"))) {
        Ok(cfg) => cfg,
        Err(error) => {
            eprintln!("No pude cargar la configuración: {error}");
            std::process::exit(1);
        }
    };

    let store = Arc::new(ChatStore::new(Some(history)));
    let chat = Arc::new(ChatModel::new(Some(store), false));

    let mut parts = AppParts::new(
        Arc::new(SilentSpeaker),
        Arc::new(parking_lot::Mutex::new(MicCapture::silent(
            VadSettings::default(),
        ))),
    );
    // DECISION: sin hilos de fondo, sin atajos globales, sin enjambre, sin
    // agente y sin captura real: el ejemplo solo quiere la barra y el panel
    // del chat (DX-1d).
    parts.background = false;
    parts.build_swarm = false;
    parts.build_agent = false;
    parts.mic_capture = false;
    parts.whisper_ready = true;
    parts.chat = Some(chat);
    parts.config_file = Some(temp.join("voice.json"));

    let app = VoiceApp::with_services(cfg, None, parts);
    app.set_hooks(Arc::new(LiveHooks::new(&app)));

    // El camino del atajo: `handle_hotkey("chat")` → `toggle_chat` →
    // `ChatPanel::open` (ventana nativa, hilo de pintura y `WndProc`).
    app.handle_hotkey("chat");
    let opened = app.hud().chat_is_open();
    eprintln!("chat abierto: {opened}");

    let deadline = Instant::now() + Duration::from_secs(ALIVE_SECS);
    let mut ticks = 0u64;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        ticks += 1;
        if !app.hud().chat_is_open() {
            eprintln!("el chat se cerró solo tras {ticks} décimas");
            break;
        }
    }
    let alive = app.hud().chat_is_open();
    eprintln!("RESULT chat_open={alive} alive_secs={ALIVE_SECS}");
    app.shutdown();
    if !alive {
        std::process::exit(1);
    }
}

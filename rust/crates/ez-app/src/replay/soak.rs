//! Prueba de 2 h del motor de reproducción (F7-05).
//!
//! Repite los escenarios en **un solo proceso**: una fuga de memoria, de hilos
//! o de handles se acumula y se ve. En paralelo pinta el HUD fuera de pantalla
//! (barra, vista rápida y chat) en el hilo `ezagent-hud-soak`, como haría el
//! hilo de interfaz. El muestreo de RSS, hilos y handles lo hace el arnés de
//! `parity/robustness/run_robustness.py` desde fuera, con las APIs de Windows:
//! así se mide el proceso real y no una estimación.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ez_gfx::Image;
use ez_hud::model::drop::{DropModel, Update};
use ez_hud::paint::chat::render::{render_message, RenderOptions as ChatOptions};
use ez_hud::paint::chat::{Block, ChatHost, FileInfo, Paragraph, Span};
use ez_hud::paint::drop::{render_model, RenderOptions as BarOptions};
use ez_hud::paint::peek::{paint_rows, NoIcons, PeekItem};
use ez_hud::tokens::palette;

use super::run_path;
use super::scenario::Paths;

/// Cada cuánto se pinta una pieza del HUD (barra, vista rápida y chat, por
/// turnos). El pintado real es a 60 fps; aquí basta con ejercitar el camino
/// completo sin robarle la CPU al motor, que es lo que se está midiendo.
const FRAME_SECS: f64 = 1.0;

/// Repite `scenarios` hasta `minutes`; devuelve el código de salida.
pub fn run(paths: &Paths, scenarios: &[PathBuf], minutes: f64) -> Result<u8, String> {
    if scenarios.is_empty() {
        return Err("no hay escenarios para la prueba de fondo".to_string());
    }
    let deadline = Instant::now() + Duration::from_secs_f64((minutes * 60.0).max(0.0));
    let stop = Arc::new(AtomicBool::new(false));
    let painter = spawn_painter(Arc::clone(&stop))?;
    let trace = std::env::temp_dir().join("ezagent-soak.trace.jsonl");
    let mut rounds = 0_u64;
    let mut failures = 0_u64;
    while Instant::now() < deadline {
        for path in scenarios {
            if Instant::now() >= deadline {
                break;
            }
            rounds += 1;
            let started = Instant::now();
            let stem = path
                .file_stem()
                .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
            let outcome = run_path(path, paths, Some(&trace));
            let seconds = started.elapsed().as_secs_f64();
            match outcome {
                Ok(outcome) if outcome.errors.is_empty() => {
                    let _ = writeln!(
                        std::io::stdout(),
                        "soak {rounds}: {stem} ok en {seconds:.1} s"
                    );
                }
                Ok(outcome) => {
                    failures += 1;
                    let _ = writeln!(
                        std::io::stdout(),
                        "soak {rounds}: {stem} FALLOS en {seconds:.1} s: {:?}",
                        outcome.errors
                    );
                }
                Err(error) => {
                    failures += 1;
                    let _ = writeln!(
                        std::io::stdout(),
                        "soak {rounds}: {stem} ERROR en {seconds:.1} s: {error}"
                    );
                }
            }
        }
    }
    stop.store(true, Ordering::SeqCst);
    let _ = painter.join();
    let _ = writeln!(
        std::io::stdout(),
        "soak: fin {rounds} vueltas, {failures} fallos"
    );
    Ok(u8::from(failures > 0))
}

/// Hilo `ezagent-hud-soak`: pinta barra, vista rápida y chat sin ventana.
fn spawn_painter(stop: Arc<AtomicBool>) -> Result<std::thread::JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("ezagent-hud-soak".to_string())
        .spawn(move || paint_loop(&stop))
        .map_err(|error| format!("no pude arrancar ezagent-hud-soak: {error}"))
}

fn paint_loop(stop: &AtomicBool) {
    let mut model = DropModel::new("pill", Some("sides"));
    let items = peek_items();
    let host = SoakHost;
    let pal = palette("light").unwrap_or_default();
    let mut frames = 0_u64;
    let mut now = 0.0;
    while !stop.load(Ordering::SeqCst) {
        // Un fotograma de la barra, uno de la vista rápida y uno del chat, por
        // turnos: los tres caminos se pintan sin encadenar su coste en un tick.
        let pose = ["idle", "listen", "talk"][(frames / 20) as usize % 3];
        let input = Update {
            now,
            dt: FRAME_SECS,
            pose,
            rms: 0.3,
            hover: frames.is_multiple_of(7),
            in_call: true,
            ..Update::default()
        };
        model.update(&input);
        match frames % 3 {
            0 => {
                let options = BarOptions {
                    width: 420,
                    height: 190,
                    scale: 1.0,
                    in_call: true,
                    now: Some(now),
                    hover_item: Some("gear"),
                    orbit: &["done", "working", "needs_you"],
                    ..BarOptions::default()
                };
                if let Err(error) = render_model(&model, &options) {
                    tracing::warn!("HUD de fondo: la barra no pintó: {error}");
                }
            }
            1 => {
                let _ = paint_rows(&items, 0, 240, 150, 1.0, "light", None, now, &NoIcons);
            }
            _ => {
                let text = if frames.is_multiple_of(2) {
                    "Hola **mundo**, listo."
                } else {
                    "Reviso el *estado* de la tarea.\n\nY vuelvo a pintar."
                };
                let rendered = render_message(&host, text, &pal, 300.0, &ChatOptions::default());
                if rendered.height <= 0.0 {
                    tracing::warn!("HUD de fondo: el chat no pintó");
                }
            }
        }
        frames += 1;
        now += FRAME_SECS;
        std::thread::sleep(Duration::from_secs_f64(FRAME_SECS));
    }
    let _ = writeln!(
        std::io::stdout(),
        "soak: HUD fuera de pantalla {frames} fotogramas"
    );
}

/// Filas fijas de la vista rápida.
fn peek_items() -> Vec<PeekItem> {
    ["done", "working", "needs_you"]
        .iter()
        .enumerate()
        .map(|(index, state)| PeekItem {
            key: format!("t{index}"),
            title: "Tarea de prueba".to_string(),
            agent: "opencode".to_string(),
            state: (*state).to_string(),
            since: 1_700_000_000.0 + index as f64,
            unseen: index == 0,
            live: index == 1,
        })
        .collect()
}

/// Host mínimo del chat: dos párrafos, sin archivos ni imágenes.
struct SoakHost;

impl ChatHost for SoakHost {
    fn parse(&self, text: &str) -> Vec<Block> {
        text.split("\n\n")
            .filter(|part| !part.trim().is_empty())
            .map(|part| {
                Block::Paragraph(Paragraph {
                    spans: vec![Span {
                        bold: part.contains("**"),
                        ..Span::plain(part.replace("**", "").replace('*', ""))
                    }],
                })
            })
            .collect()
    }

    fn resolve_target(&self, _target: &str, _base_dirs: &[PathBuf]) -> Option<PathBuf> {
        None
    }

    fn file_info(&self, _path: Option<&Path>, _roots: Option<&[PathBuf]>) -> Option<FileInfo> {
        None
    }

    fn load_image(&self, _info: &FileInfo) -> Option<Image> {
        None
    }

    fn text_preview(&self, _info: &FileInfo) -> Option<(String, bool)> {
        None
    }
}

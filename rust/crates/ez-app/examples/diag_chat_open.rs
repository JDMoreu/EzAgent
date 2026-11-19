//! DX-1d: abre el chat con un historial sintético y busca el pánico.
//!
//! La app se cayó al abrir el chat (Windows: `0xc0000409`, el aborto de Rust).
//! Este ejemplo reproduce el camino real del panel (`ChatView::render` →
//! `compose` → el parche del cursor) con un historial **sintético** (nunca el
//! historial real de la persona) que lleva todo lo que puede romper la
//! maquetación: markdown, bloques de código, listas, enlaces, emojis, texto
//! largo sin espacios, mensajes vacíos, imágenes y 35 mensajes.
//!
//! ```text
//! cargo run --release -p ez-app --example diag_chat_open
//! ```
//!
//! Sale con código 1 si algún caso entra en pánico (y dice cuál).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ez_chat::markdown;
use ez_gfx::raster::Image;
use ez_hud::model::chat::view::{
    ChatMessage, ChatModel, ChatView, ConfirmCard, Move, Role, SessionInfo, TextField, ViewHost,
};
use ez_hud::model::drop::{DropModel, Update, PILL_H, PILL_W};
use ez_hud::paint::chat::{
    Block, ChatHost, Code, Document, FileInfo, Heading, ImageBlock, ListItem, Paragraph, Quote,
    Span, Table, TableAlign,
};
use ez_hud::paint::drop::render_fluid;
use ez_hud::win::chat::compose::{compose_cached, fluid_array, ContentCache, Scene, WindowRect};
use ez_hud::win::chat::{AnimState, ChatUi};
use ndarray::Array3;
use parking_lot::Mutex;

// ---------------------------------------------------------------- host real

fn span(span: markdown::Span) -> Span {
    Span {
        text: span.text,
        bold: span.bold,
        italic: span.italic,
        code: span.code,
        link: span.link,
    }
}

fn spans(spans: Vec<markdown::Span>) -> Vec<Span> {
    spans.into_iter().map(span).collect()
}

fn align(align: markdown::Align) -> TableAlign {
    match align {
        markdown::Align::Left => TableAlign::Left,
        markdown::Align::Center => TableAlign::Center,
        markdown::Align::Right => TableAlign::Right,
    }
}

fn block(block: markdown::Block) -> Block {
    match block {
        markdown::Block::Heading(heading) => Block::Heading(Heading {
            level: i32::try_from(heading.level).unwrap_or(i32::MAX),
            spans: spans(heading.spans),
        }),
        markdown::Block::Paragraph(paragraph) => Block::Paragraph(Paragraph {
            spans: spans(paragraph.spans),
        }),
        markdown::Block::ListItem(item) => Block::ListItem(ListItem {
            level: item.level,
            marker: item.marker,
            spans: spans(item.spans),
            checked: item.checked,
        }),
        markdown::Block::Quote(quote) => Block::Quote(Quote {
            spans: spans(quote.spans),
        }),
        markdown::Block::Code(code) => Block::Code(Code {
            lang: code.lang,
            text: code.text,
        }),
        markdown::Block::Table(table) => Block::Table(Table {
            header: table
                .header
                .into_iter()
                .map(spans)
                .collect::<Vec<Vec<Span>>>(),
            rows: table
                .rows
                .into_iter()
                .map(|row| row.into_iter().map(spans).collect())
                .collect(),
            aligns: table.aligns.into_iter().map(align).collect(),
        }),
        markdown::Block::Rule(_) => Block::Rule,
        markdown::Block::Image(image) => Block::Image(ImageBlock {
            alt: image.alt,
            src: image.src,
        }),
        markdown::Block::Document(document) => Block::Document(Document {
            label: document.label,
            target: document.target,
        }),
    }
}

/// El mismo puente que usa la app (`app/chat_wiring/host.rs`).
struct RealHost;

impl ChatHost for RealHost {
    fn parse(&self, text: &str) -> Vec<Block> {
        markdown::parse(text).into_iter().map(block).collect()
    }

    fn resolve_target(&self, _: &str, _: &[PathBuf]) -> Option<PathBuf> {
        None
    }

    fn file_info(&self, _: Option<&Path>, _: Option<&[PathBuf]>) -> Option<FileInfo> {
        None
    }

    fn load_image(&self, _: &FileInfo) -> Option<Image> {
        None
    }

    fn text_preview(&self, _: &FileInfo) -> Option<(String, bool)> {
        None
    }
}

impl ViewHost for RealHost {
    fn parse_inline(&self, text: &str) -> Vec<Span> {
        spans(markdown::parse_inline(text))
    }
}

// ------------------------------------------------------------ modelo real

struct StoreModel {
    messages: Vec<ChatMessage>,
    snapshots: Arc<AtomicUsize>,
}

impl ChatModel for StoreModel {
    fn snapshot(&self) -> (i64, Vec<ChatMessage>) {
        self.snapshots.fetch_add(1, Ordering::SeqCst);
        (0, self.messages.clone())
    }

    fn version(&self) -> i64 {
        0
    }

    fn sessions(&self) -> Vec<SessionInfo> {
        Vec::new()
    }

    fn busy(&self) -> bool {
        false
    }

    fn confirm(&self) -> Option<ConfirmCard> {
        None
    }

    fn viewing(&self) -> Option<i64> {
        None
    }

    fn current_session(&self) -> Option<i64> {
        None
    }

    fn voice_replies(&self) -> bool {
        false
    }

    fn mic_muted(&self) -> bool {
        false
    }

    fn view_session(&mut self, _: Option<i64>) {}
}

// -------------------------------------------------------------- entrada

#[derive(Default)]
struct EmptyInput {
    text: String,
    cursor: usize,
}

impl TextField for EmptyInput {
    fn text(&self) -> &str {
        &self.text
    }

    fn cursor(&self) -> usize {
        self.cursor
    }

    fn empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    fn insert(&mut self, value: &str) {
        self.text.push_str(value);
        self.cursor = self.text.chars().count();
    }

    fn newline(&mut self) {
        self.insert("\n");
    }

    fn take(&mut self) -> String {
        let value = std::mem::take(&mut self.text);
        self.cursor = 0;
        value
    }

    fn move_caret(&mut self, _: Move, _: bool, _: bool) {}

    fn backspace(&mut self, _: bool) {
        self.text.pop();
        self.cursor = self.text.chars().count();
    }

    fn delete(&mut self, _: bool) {}

    fn select_all(&mut self) {}

    fn selected_text(&self) -> String {
        String::new()
    }

    fn selection(&self) -> Option<(usize, usize)> {
        None
    }

    fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    fn place_caret(&mut self, cursor: usize) {
        self.cursor = cursor;
    }
}

// ------------------------------------------------------------------ barra

struct FakeUi {
    scale: f64,
    fluid: Mutex<Option<(f64, Array3<u8>)>>,
    model: Mutex<DropModel>,
}

impl FakeUi {
    fn new(scale: f64) -> Self {
        let mut model = DropModel::new("pill", None);
        for index in 0..48 {
            model.update(&Update {
                now: index as f64 / 30.0,
                dt: 1.0 / 30.0,
                pose: "idle",
                rms: 0.35,
                theme: "dark",
                ..Update::default()
            });
        }
        Self {
            scale,
            fluid: Mutex::new(None),
            model: Mutex::new(model),
        }
    }
}

impl ChatUi for FakeUi {
    fn scale(&self) -> f64 {
        self.scale
    }

    fn bar_scale(&self) -> f64 {
        self.scale
    }

    fn form_size(&self) -> (f64, f64) {
        (PILL_W, PILL_H)
    }

    fn form_anchor(&self) -> Option<(f64, f64)> {
        None
    }

    fn layout(&self) -> &str {
        "pill"
    }

    fn reduced(&self) -> bool {
        false
    }

    fn theme_name(&self) -> String {
        String::from("dark")
    }

    fn chat_pose(&self) -> String {
        String::from("idle")
    }

    fn in_call(&self) -> bool {
        false
    }

    fn chat_hotkey(&self) -> String {
        String::new()
    }

    fn chat_size(&self) -> Option<(f64, f64)> {
        None
    }

    fn remember_chat_size(&self, _: f64, _: f64) {}

    fn chat_body_rgb(&self) -> [u8; 3] {
        ez_hud::tokens::palette("dark")
            .ok()
            .and_then(|palette| palette.get("BODY").copied())
            .map_or([0, 0, 0], |color| color.channels())
    }

    fn chat_fluid(&self, width: usize, height: usize, now: f64) -> Option<Array3<f32>> {
        let mut cache = self.fluid.lock();
        if cache.as_ref().is_none_or(|(at, _)| *at != now) {
            let fluid = render_fluid(&self.model.lock(), now);
            *cache = Some((now, fluid));
        }
        fluid_array(cache.as_ref().map(|(_, rgb)| rgb), width, height)
    }

    fn set_form_hidden(&self, _: bool) {}

    fn set_chat_open(&self, _: bool) {}

    fn set_scale(&self, _: f64) {}

    fn make_view(&self) -> Option<Box<ChatView>> {
        None
    }
}

// ------------------------------------------------------------- escenario

/// Un texto largo sin espacios (una palabra gigante).
fn long_word() -> String {
    "x".repeat(2_000)
}

/// Un bloque de markdown con todo lo que puede romper la maquetación.
fn rich() -> String {
    format!(
        "## Encabezado\n\n\
         Un párrafo con **negrita**, *cursiva*, `código` y un [enlace](https://example.com).\n\n\
         - Lista de primer nivel\n\
         - Lista de segundo nivel\n\
           - Y un tercer nivel\n\n\
         > Una cita que se repite para ocupar algo de alto.\n\n\
         ```rust\n\
         fn main() {{\n\
         \x20   println!(\"hola\");\n\
         }}\n\
         ```\n\n\
         | Caso | Antes | Después |\n\
         |---|---|---|\n\
         | 10 | 3 ms | 1 ms |\n\
         | 5000 | 900 ms | 12 ms |\n\n\
         ![una imagen](C:\\no\\existe\\imagen.png)\n\n\
         Emojis: 🦙 🧑‍💻 🇦🇷 ✅ 🎉\n\n\
         {}\n",
        long_word()
    )
}

/// El historial sintético: todo lo que el brief pide.
fn history() -> Vec<ChatMessage> {
    // Primero los pares (rol, texto) y después los mensajes: sin-PRNG.
    let mut pairs: Vec<(Role, String)> = vec![
        (Role::User, "Revisa la rama `work/DX-1`.".to_string()),
        (Role::Assistant, rich()),
        (Role::User, String::new()),
        (Role::Assistant, String::new()),
        (Role::User, long_word()),
        (Role::Assistant, "Una respuesta corta.".to_string()),
        (Role::System, "Conversación compactada.".to_string()),
        (
            Role::User,
            "Emojis 🦙 y texto con acentos: ñáéíóú ¿? ¡!".to_string(),
        ),
        (Role::Assistant, rich()),
    ];
    // Hasta 35 mensajes, mezclando los tres roles.
    let mut index = pairs.len();
    while pairs.len() < 35 {
        let role = match index % 3 {
            0 => Role::User,
            1 => Role::Assistant,
            _ => Role::System,
        };
        let text = match index % 4 {
            0 => rich(),
            1 => format!("Mensaje {index} con un poco de texto normal."),
            2 => String::new(),
            _ => long_word(),
        };
        pairs.push((role, text));
        index += 1;
    }
    pairs
        .into_iter()
        .enumerate()
        .map(|(number, (role, text))| ChatMessage::new(number as i64 + 1, role, text))
        .collect()
}

fn view_for(
    messages: Vec<ChatMessage>,
    width: f64,
    height: f64,
    scale: f64,
    theme: &str,
) -> ChatView {
    let mut view = ChatView::new(
        Box::new(StoreModel {
            messages,
            snapshots: Arc::new(AtomicUsize::new(0)),
        }),
        Box::new(RealHost),
        Box::new(EmptyInput::default()),
    );
    view.configure(
        width,
        height,
        scale,
        theme,
        ez_hud::theme::card_palette(theme),
    );
    view.set_focus_instant(0.0);
    view
}

/// Un caso: tamaño, escala y fase del cursor.
struct Case {
    name: &'static str,
    width: f64,
    height: f64,
    scale: f64,
    theme: &'static str,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "900×640 escala 1,0",
            width: 900.0,
            height: 640.0,
            scale: 1.0,
            theme: "dark",
        },
        Case {
            name: "440×600 escala 1,0",
            width: 440.0,
            height: 600.0,
            scale: 1.0,
            theme: "dark",
        },
        Case {
            name: "900×640 escala 1,5",
            width: 900.0,
            height: 640.0,
            scale: 1.5,
            theme: "dark",
        },
        Case {
            name: "472×672 escala 1,0 (la del chat)",
            width: 472.0,
            height: 672.0,
            scale: 1.0,
            theme: "dark",
        },
        Case {
            name: "434×704 escala 1,0 tema light",
            width: 434.0,
            height: 704.0,
            scale: 1.0,
            theme: "light",
        },
    ]
}

/// El cuadro completo: contenido, composición y parche del cursor.
fn frame(ui: &FakeUi, view: &mut ChatView, case: &Case, now: f64) {
    let margin = (ez_hud::geometry::MARGIN * ui.scale()).round_ties_even() as i32;
    let window = WindowRect {
        x: 0,
        y: 0,
        width: (case.width * case.scale).round_ties_even() as usize + 2 * margin as usize,
        height: (case.height * case.scale).round_ties_even() as usize + 2 * margin as usize,
    };
    let mut content = ContentCache::default();
    let mut scene = Scene {
        ui,
        view,
        anim: AnimState::Open,
        form: (0.0, 0.0, window.width as f64, window.height as f64),
        act: 0.0,
        now,
    };
    compose_cached(&mut scene, 1.0, true, window, &mut content);
}

/// Un caso completo: abre, pinta y deja correr el cursor.
fn run_case(case: &Case) -> Result<(), String> {
    let ui = FakeUi::new(case.scale);
    let mut view = view_for(history(), case.width, case.height, case.scale, case.theme);
    // Diez segundos de cuadros: el cursor parpadea (fase 0 y 1) y el morph
    // paints rows.
    for step in 0..200 {
        let now = step as f64 * 0.05;
        frame(&ui, &mut view, case, now);
    }
    Ok(())
}

fn main() {
    // El gancho del pánico escribe el mensaje y la ubicación: sin él, un
    // pánico en este ejemplo solo sale como « panicked at ... ».
    std::panic::set_hook(Box::new(|info| {
        let location = info.location().map_or_else(
            || "ubicación desconocida".to_string(),
            |where_| format!("{}:{}:{}", where_.file(), where_.line(), where_.column()),
        );
        eprintln!("PÁNICO: {info}\n  en {location}");
    }));

    let mut failures = 0;
    for case in cases() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_case(&case)));
        match result {
            Ok(Ok(())) => eprintln!("ok    {}", case.name),
            Ok(Err(error)) => {
                failures += 1;
                eprintln!("FALLA {}: {error}", case.name);
            }
            Err(_) => {
                failures += 1;
                eprintln!("FALLA {}: pánico", case.name);
            }
        }
    }
    if failures > 0 {
        eprintln!("{failures} caso(s) entraron en pánico");
        std::process::exit(1);
    }
    eprintln!("todos los casos pasaron");
}

//! DX-1: coste de la ventana del chat según crece el historial.
//!
//! Esta es una **medición**, no una prueba de regresión: imprime los números
//! que explican por qué la app de voz se congela al abrir el chat. Reproduce
//! el camino real de la app (parser de markdown de `ez-chat`, vista de
//! `ez-hud`, modelo de `ez-chat`) y mide tres cosas:
//!
//! 1. `ChatView::render`: lo que cuesta pintar el panel de mensajes.
//! 2. `ChatModel::snapshot`: lo que cuesta la copia del historial que la vista
//!    pide en cada fotograma.
//! 3. `compose_cached`: lo que cuesta el fotograma completo del panel.
//!
//! Vive en `examples/` (y no en `tests/`) a propósito: es lenta y su coste
//! depende de la máquina; las pruebas de regresión del arreglo son las
//! rápidas de `ez-hud/tests/chat_frame_cost.rs`.
//!
//! Ejecución (release; en debug los números no dicen nada del binario que se
//! distribuye):
//!
//! ```text
//! cargo run --release -p ez-app --example diag_chat_history_cost
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use ndarray::Array3;
use parking_lot::Mutex;

use ez_chat::markdown;
use ez_gfx::raster::Image;
use ez_hud::geometry::{panel_rect, WorkArea, MARGIN};
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

/// Tope de memoria de la medición: por encima se corta (no se deja que la
/// medición vacíe la máquina). Es el mismo límite que se pidió para DX-1.
const MEMORY_LIMIT_MB: u64 = 1024;

/// Memoria de trabajo del proceso, en MB (Windows, sin dependencias).
fn working_set_mb() -> u64 {
    #[repr(C)]
    struct Counters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut Counters,
            size: u32,
        ) -> i32;
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }
    let mut counters = Counters {
        cb: std::mem::size_of::<Counters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: `counters` es local y del tamaño que se declara.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<Counters>() as u32,
        )
    };
    if ok == 0 {
        return 0;
    }
    (counters.working_set_size / (1024 * 1024)) as u64
}

/// `true` si la medición sigue dentro del tope de memoria.
fn within_budget() -> bool {
    working_set_mb() <= MEMORY_LIMIT_MB
}

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

/// Modelo con un historial fijo; `snapshot` copia todo, como el de la app.
struct StoreModel {
    messages: Vec<ChatMessage>,
}

impl ChatModel for StoreModel {
    fn snapshot(&self) -> (i64, Vec<ChatMessage>) {
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

/// Un mensaje de usuario y otro del asistente, con markdown real.
fn conversation(count: usize) -> Vec<ChatMessage> {
    let user = "Revisa la rama `work/DX-1` y dime qué falla en el chat.\n\
        - ¿Repite el último error?\n\
        - ¿Qué pruebas pasan?";
    let assistant = "## Resumen\n\n\
        El panel del chat congela la interfaz cuando el historial crece.\n\n\
        1. La vista copia el historial entero en cada fotograma.\n\
        2. La caché de maquetación está limitada a 400 mensajes.\n\
        3. Con más de 400 mensajes la caché se vacía sola.\n\n\
        ```text\n\
        frame_ms = render(history)\n\
        ```\n\n\
        | Caso | Antes | Después |\n\
        |---|---|---|\n\
        | 10 | 3 ms | 1 ms |\n\
        | 5000 | 900 ms | 12 ms |\n\n\
        Eso explica que el audio siga sonando mientras la ventana no responde.";
    (0..count)
        .map(|index| {
            let role = if index % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            };
            let text = if role == Role::User { user } else { assistant };
            ChatMessage::new(index as i64 + 1, role, text)
        })
        .collect()
}

/// Panel de 900×640 lógicos (tamaño por defecto del chat) a escala 1,0.
const PANEL_W: f64 = 900.0;
const PANEL_H: f64 = 640.0;

fn view_for(count: usize) -> ChatView {
    let mut view = ChatView::new(
        Box::new(StoreModel {
            messages: conversation(count),
        }),
        Box::new(RealHost),
        Box::new(EmptyInput::default()),
    );
    view.configure(
        PANEL_W,
        PANEL_H,
        1.0,
        "dark",
        ez_hud::tokens::palette("dark").expect("the dark palette exists"),
    );
    view.set_focus_instant(0.0);
    view
}

fn ms<F: FnMut()>(runs: usize, mut body: F) -> f64 {
    let mut total = 0.0;
    for _ in 0..runs {
        let start = Instant::now();
        body();
        total += start.elapsed().as_secs_f64() * 1000.0;
    }
    total / runs as f64
}

// -------------------------------------------------------------- medición

/// Coste del contenido del panel según el número de mensajes.
///
/// El primer `render` es lo que hacía `ChatPanel::prepare_view` al abrir el
/// chat; los siguientes son los fotogramas de la ventana abierta.
fn content_cost_grows_with_the_history() {
    let counts = [10usize, 100, 1000];
    eprintln!(
        "{:>6} {:>12} {:>12} {:>12} {:>12} {:>8}",
        "msgs", "abre (frío)", "2º render", "3er render", "snapshot", "RSS MB"
    );
    let mut cold_small = f64::MAX;
    let mut cold_big = 0.0;
    for count in counts {
        let mut view = view_for(count);
        let start = Instant::now();
        view.render(0.0);
        let cold = start.elapsed().as_secs_f64() * 1000.0;
        if count == 100 {
            cold_small = cold;
        }
        if count == 1000 {
            cold_big = cold;
        }
        if !within_budget() {
            eprintln!(
                "{count:>6} {cold:>10.2} ms {:>12} {:>12} {:>12} {:>8}   (CORTADA: {MEMORY_LIMIT_MB} MB de RSS)",
                "—", "—", "—", working_set_mb()
            );
            break;
        }
        // Por encima de la caché (401) cada cuadro remaqueta el historial
        // entero: una sola pasada basta para verlo y evita minutos de prueba.
        let repeats = if count > 401 { 1 } else { 3 };
        let second = ms(repeats, || {
            view.render(0.1);
        });
        if !within_budget() {
            eprintln!(
                "{count:>6} {cold:>10.2} ms {second:>10.2} ms {:>12} {:>12}   (CORTADA: {MEMORY_LIMIT_MB} MB de RSS)",
                "—", "—"
            );
            break;
        }
        let third = ms(repeats, || {
            view.render(0.2);
        });
        let snapshot = ms(20, || {
            let model = view.model.snapshot();
            std::hint::black_box(model);
        });
        eprintln!(
            "{count:>6} {cold:>10.2} ms {second:>10.2} ms {third:>10.2} ms {snapshot:>10.3} ms {:>8}",
            working_set_mb()
        );
    }
    eprintln!(
        "el contenido cuesta {cold_big:.1} ms con 1000 mensajes y {cold_small:.1} ms con 100"
    );
}

/// La caché de maquetación: cuánto se remaqueta en cada fotograma.
fn the_layout_cache_thrashes_past_400_messages() {
    let mut rows = Vec::new();
    for count in [10usize, 100] {
        let mut view = view_for(count);
        view.render(0.0); // primer fotograma: maquetación completa
        let warm = ms(3, || {
            view.render(0.1);
        });
        rows.push((count, warm));
        eprintln!("{count:>5} mensajes: fotograma siguiente {warm:>8.2} ms");
    }
    if rows.len() == 2 {
        eprintln!(
            "con la caché completa (100 < 401) el fotograma posterior cuesta {:.2} ms",
            rows[1].1
        );
    }
}

/// Fotograma completo del panel (composición + contenido + cuantizado).
fn full_frame_cost_grows_with_the_history() {
    let scale = 1.0;
    let ui = FakeUi::new(scale);
    let work: WorkArea = (0, 0, 1920, 1080);
    let form = (
        960.0 - PILL_W / 2.0,
        1000.0 - PILL_H / 2.0,
        960.0 + PILL_W / 2.0,
        1000.0 + PILL_H / 2.0,
    );
    let rect = panel_rect(form, work, scale, None);
    let margin = (MARGIN * scale).round_ties_even() as i32;
    let window = WindowRect {
        x: rect.0.round_ties_even() as i32 - margin,
        y: rect.1.round_ties_even() as i32 - margin,
        width: (rect.2 - rect.0).round_ties_even() as usize + 2 * margin as usize,
        height: (rect.3 - rect.1).round_ties_even() as usize + 2 * margin as usize,
    };
    eprintln!("ventana del panel: {}×{} px", window.width, window.height);
    for count in [10usize, 100] {
        let mut view = view_for(count);
        let mut content = ContentCache::default();
        let mut frame = |now: f64, content: &mut ContentCache| {
            let mut scene = Scene {
                ui: &ui,
                view: &mut view,
                anim: AnimState::Open,
                form,
                act: 0.0,
                now,
            };
            compose_cached(&mut scene, 1.0, true, window, content);
        };
        frame(0.0, &mut content);
        if !within_budget() {
            eprintln!("{count:>5} mensajes: fotograma completo CORTADO ({MEMORY_LIMIT_MB} MB)");
            break;
        }
        let warm = ms(3, || {
            frame(0.1, &mut content);
        });
        eprintln!(
            "{count:>5} mensajes: fotograma completo {warm:>8.2} ms (RSS {} MB)",
            working_set_mb()
        );
    }
}

fn main() {
    eprintln!("== contenido según el historial ==");
    content_cost_grows_with_the_history();
    eprintln!("== caché de maquetación ==");
    the_layout_cache_thrashes_past_400_messages();
    eprintln!("== fotograma completo ==");
    full_frame_cost_grows_with_the_history();
}

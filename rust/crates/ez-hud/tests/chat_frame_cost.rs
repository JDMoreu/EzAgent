//! DX-1b: el panel del chat no puede pagar el historial entero en cada cuadro.
//!
//! Estas son las pruebas de regresión del arreglo (rápidas y acotadas); la
//! medición larga vive en `ez-app/examples/diag_chat_history_cost.rs`.
//!
//! - `the_layout_cache_does_not_empty_itself_past_the_old_cap`: con más
//!   mensajes que el tope antiguo (400), el segundo cuadro no vuelve a
//!   maquetar el historial.
//! - `the_frame_signature_does_not_copy_the_history`: la firma del panel no
//!   pide el historial al modelo (solo su versión).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ez_gfx::raster::Image;
use ez_hud::model::chat::view::{
    ChatMessage, ChatModel, ChatView, ConfirmCard, Move, Role, SessionInfo, TextField, ViewHost,
};
use ez_hud::model::drop::{DropModel, Update, PILL_H, PILL_W};
use ez_hud::paint::chat::{Block, ChatHost, FileInfo, Paragraph, Span};
use ez_hud::paint::drop::render_fluid;
use ez_hud::win::chat::compose::{compose_cached, fluid_array, ContentCache, Scene, WindowRect};
use ez_hud::win::chat::{AnimState, ChatUi};

/// Bar state of the panel: the real fluid and palette of the app.
struct FakeUi {
    scale: f64,
    fluid: parking_lot::Mutex<Option<(f64, ndarray::Array3<u8>)>>,
    model: parking_lot::Mutex<DropModel>,
}

impl FakeUi {
    fn new() -> Self {
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
            scale: 1.0,
            fluid: parking_lot::Mutex::new(None),
            model: parking_lot::Mutex::new(model),
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

    fn chat_fluid(&self, width: usize, height: usize, now: f64) -> Option<ndarray::Array3<f32>> {
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

/// Model with a fixed history that counts how many times it is copied.
struct CountingModel {
    messages: Vec<ChatMessage>,
    snapshots: Arc<AtomicUsize>,
}

impl ChatModel for CountingModel {
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

/// Markdown host: one paragraph per line, no files.
struct PlainHost;

impl ChatHost for PlainHost {
    fn parse(&self, text: &str) -> Vec<Block> {
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                Block::Paragraph(Paragraph {
                    spans: vec![Span {
                        text: line.to_string(),
                        bold: false,
                        italic: false,
                        code: false,
                        link: None,
                    }],
                })
            })
            .collect()
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

impl ViewHost for PlainHost {
    fn parse_inline(&self, text: &str) -> Vec<Span> {
        vec![Span {
            text: text.to_string(),
            bold: false,
            italic: false,
            code: false,
            link: None,
        }]
    }
}

/// Empty input box.
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

/// A view with `count` short messages and a counter of the copies.
fn view_for(count: usize) -> (ChatView, Arc<AtomicUsize>) {
    let messages: Vec<ChatMessage> = (0..count)
        .map(|index| {
            ChatMessage::new(
                index as i64 + 1,
                if index % 2 == 0 {
                    Role::User
                } else {
                    Role::Assistant
                },
                format!("mensaje {index}"),
            )
        })
        .collect();
    let snapshots = Arc::new(AtomicUsize::new(0));
    let mut view = ChatView::new(
        Box::new(CountingModel {
            messages,
            snapshots: Arc::clone(&snapshots),
        }),
        Box::new(PlainHost),
        Box::new(EmptyInput::default()),
    );
    view.configure(
        440.0,
        600.0,
        1.0,
        "dark",
        ez_hud::tokens::palette("dark").expect("the dark palette exists"),
    );
    view.set_focus_instant(0.0);
    (view, snapshots)
}

/// La caché de maquetación no se vacía sola con más mensajes que el tope
/// antiguo (400 entradas).
///
/// DECISION: el arreglo mantiene el tope en bytes y expulsa la tesela menos
/// usada recientemente que no esté en pantalla; además, la altura de cada
/// mensaje se cachea aparte de su tesela, así que un cuadro solo maqueta lo
/// que se ve. Antes, con 450 mensajes el segundo cuadro volvía a maquetar el
/// historial entero.
#[test]
fn the_layout_cache_does_not_empty_itself_past_the_old_cap() {
    let (mut view, _) = view_for(450);
    let start = Instant::now();
    view.render(0.0);
    let cold = start.elapsed().as_secs_f64() * 1000.0;
    let layouts = view.layouts();
    assert!(layouts > 0, "the first frame laid out nothing");

    let start = Instant::now();
    view.render(0.1);
    let warm = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(
        view.layouts(),
        layouts,
        "the second frame laid out messages again ({} -> {})",
        layouts,
        view.layouts()
    );
    assert!(
        warm < cold / 2.0,
        "the second frame cost {warm:.2} ms and the first {cold:.2} ms"
    );
}

/// En reposo el contenido no se repinta, aunque el cursor parpadee.
///
/// DECISION: la firma del contenido y la del cuadro están separadas (DX-1,
/// paso 4). Antes el parpadeo del cursor y el tic del orbe invalidaban la
/// capa de contenido, así que la historia entera se repintaba dos veces por
/// segundo (y treinta mientras el modelo contestaba).
#[test]
fn the_content_is_not_repainted_at_rest() {
    let ui = FakeUi::new();
    let (mut view, _) = view_for(20);
    let mut content = ContentCache::default();
    let window = WindowRect {
        x: 0,
        y: 0,
        width: 472,
        height: 672,
    };
    // Un segundo de cuadros: el cursor parpadea dos veces en ese tiempo.
    let mut painted = 0;
    for step in 0..20 {
        let now = step as f64 * 0.05;
        let mut scene = Scene {
            ui: &ui,
            view: &mut view,
            anim: AnimState::Open,
            form: (0.0, 0.0, 472.0, 672.0),
            act: 0.0,
            now,
        };
        compose_cached(&mut scene, 1.0, true, window, &mut content);
        painted = view.content_renders();
    }
    assert_eq!(
        painted, 1,
        "the content layer was painted {painted} times in one second at rest"
    );
}

/// La firma del panel no copia el historial.
///
/// La vista necesita los mensajes para dibujarlos (las copias del dibujo);
/// la firma solo necesita la versión, que se lee sin copiar (DX-1, paso 6).
/// Antes, cada firma pedía `snapshot()` y el historial se copiaba dos veces
/// más por cuadro.
#[test]
fn the_frame_signature_does_not_copy_the_history() {
    let (mut view, snapshots) = view_for(20);
    view.render(0.0);
    let drawn = snapshots.load(Ordering::SeqCst);
    view.render(0.1);
    let redrawn = snapshots.load(Ordering::SeqCst);
    let copies_of_the_draw = redrawn - drawn;

    let ui = FakeUi::new();
    let (mut view, snapshots) = view_for(20);
    let mut content = ContentCache::default();
    let window = WindowRect {
        x: 0,
        y: 0,
        width: 472,
        height: 672,
    };
    let before = snapshots.load(Ordering::SeqCst);
    let mut scene = Scene {
        ui: &ui,
        view: &mut view,
        anim: AnimState::Open,
        form: (0.0, 0.0, 472.0, 672.0),
        act: 0.0,
        now: 0.0,
    };
    compose_cached(&mut scene, 1.0, true, window, &mut content);
    let composed = snapshots.load(Ordering::SeqCst) - before;
    assert_eq!(
        composed, copies_of_the_draw,
        "the composition copied the whole history {composed} times and the drawing {copies_of_the_draw}"
    );
}

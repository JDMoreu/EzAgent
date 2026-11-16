//! Pruebas del cableado del chat (F6-08).

use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

use ez_chat::session::ChatModel;
use ez_chat::store::{ChatStore, Source};
use ez_hud::model::chat::view::{
    ChatModel as ViewModel, Move, Role as ViewRole, TextField, ViewHost,
};
use ez_hud::paint::chat::{Block, ChatHost, MediaKind};
use ez_hud::win::chat::{ChatEvent, ChatEvents, ChatPanel as ChatWindowPanel, ChatUi};
use ez_hud::win::drop::ChatPanel as DropChatPanel;
use ez_llm::messenger::LlmClient;
use ez_llm::openai::OpenAiChatClient;
use serde_json::{json, Value};

use ez_core::config::VoiceConfig;

use crate::app::core::AppEvent;

use super::host::{chat_file_info, HudHost};
use super::input::HudInput;
use super::model::HudModel;
use super::ui::HudChatUi;
use super::{HudChatEvents, PanelBridge};

use crate::app::states::tests::support::{
    chunk, drain_events, install_client, wait_event, Fixture, Script, ScriptedClient,
};

/// Archivo en la carpeta temporal propia del proceso; se borra al soltarlo.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn new(name: &str) -> Self {
        let unique = format!(
            "ezagent-f6-08-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        );
        Self {
            path: std::env::temp_dir().join(unique),
        }
    }

    fn write(&self, bytes: &[u8]) {
        std::fs::write(&self.path, bytes).expect("archivo temporal");
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn png_2x1() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut out), 2, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("cabecera PNG");
        writer
            .write_image_data(&[255, 0, 0, 255, 0, 255, 0, 255])
            .expect("píxeles PNG");
    }
    out
}

fn bmp_2x1() -> Vec<u8> {
    // 24 bits por píxel, fila rellenada a 4 bytes (2 px → 8). De abajo arriba.
    let mut out = Vec::new();
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&62_u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54_u32.to_le_bytes());
    out.extend_from_slice(&40_u32.to_le_bytes());
    out.extend_from_slice(&2_i32.to_le_bytes());
    out.extend_from_slice(&1_i32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&24_u16.to_le_bytes());
    for field in [0_u32, 8, 0, 0, 0, 0] {
        out.extend_from_slice(&field.to_le_bytes());
    }
    // Píxel 0 rojo, píxel 1 azul, en BGR.
    out.extend_from_slice(&[0, 0, 255, 255, 0, 0, 0, 0]);
    out
}

fn temp_dir() -> PathBuf {
    std::env::temp_dir()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Reads one HTTP request, headers and body, so the client finishes sending.
fn read_http_request(stream: &mut TcpStream) {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
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

/// Local OpenAI-compatible server: one streamed answer and `[DONE]`.
///
/// DECISION: no `tiny_http` in `ez-app`; a one-shot `TcpListener` serves the
/// same double as the Python tests' local HTTP server.
fn llm_server(answer: &str) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener del LLM");
    let address = listener.local_addr().expect("dirección del listener");
    let body = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n\
         data: {{\"choices\":[{{\"delta\":{{\"content\":\"{answer}\"}}}}]}}\n\ndata: [DONE]\n\n"
    );
    let handle = std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        read_http_request(&mut stream);
        let _ = stream.write_all(body.as_bytes());
        let _ = stream.flush();
    });
    (format!("http://{address}/v1"), handle)
}

fn fixture_config() -> VoiceConfig {
    let mut cfg = VoiceConfig::new();
    cfg.set("hud_hidden", Value::Bool(false));
    cfg.set("smart_turn", json!(false));
    cfg.set("chat_voice", json!(true));
    cfg.set("llm_provider", json!("openai"));
    cfg.set("llm_model", json!("gpt-prueba"));
    cfg
}

#[test]
fn model_projects_messages_sessions_and_confirmation() {
    let file = TempFile::new("history.db");
    let store = Arc::new(ChatStore::new(Some(file.path.clone())));
    let chat = Arc::new(ChatModel::new(Some(Arc::clone(&store)), true));
    chat.add_user("hola", Source::Chat);
    let reply = chat.begin_assistant(Source::Chat);
    chat.append(reply.id, "Hola.");
    chat.finish(reply.id);
    let mut model = HudModel::new(Arc::clone(&chat));

    let (version, messages) = model.snapshot();
    assert!(version > 0);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, ViewRole::User);
    assert_eq!(messages[0].text, "hola");
    assert_eq!(messages[0].source, "chat");
    assert_eq!(messages[1].role, ViewRole::Assistant);
    assert_eq!(messages[1].text, "Hola.");
    assert!(!messages[1].streaming);
    assert!(!model.busy());
    assert_eq!(model.sessions().len(), 1);
    assert_eq!(model.sessions()[0].session, 1);
    assert_eq!(model.current_session(), Some(1));
    assert!(model.voice_replies());

    chat.ask_confirm("¿Seguir?", true);
    let card = model.confirm().expect("confirmación");
    assert_eq!(card.summary, "¿Seguir?");
    assert!(card.strong);
    chat.clear_confirm();
    assert!(model.confirm().is_none());

    chat.new_session();
    chat.add_user("otra", Source::Chat);
    assert_eq!(model.sessions().len(), 2);
    model.view_session(Some(1));
    assert_eq!(model.viewing(), Some(1));
    let (_, shown) = model.snapshot();
    assert!(shown.iter().all(|message| message.id < 0));
    model.view_session(None);
    assert_eq!(model.viewing(), None);

    chat.set_mic_muted(true);
    assert!(model.mic_muted());
}

#[test]
fn saved_history_is_shown_when_the_chat_opens() {
    let file = TempFile::new("saved-history.db");
    {
        let store = ChatStore::new(Some(file.path.clone()));
        store.append(
            ez_chat::store::Role::User,
            "mensaje guardado",
            Source::Chat,
            Some(10.0),
        );
    }
    let store = Arc::new(ChatStore::new(Some(file.path.clone())));
    let chat = Arc::new(ChatModel::new(Some(store), false));
    let model = HudModel::new(chat);
    let (_, messages) = model.snapshot();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text, "mensaje guardado");
    assert_eq!(messages[0].t, 10.0);
}

#[test]
fn input_adapter_matches_text_input() {
    let mut input = HudInput::new();
    input.insert("hola mundo");
    assert_eq!(input.text(), "hola mundo");
    assert_eq!(input.cursor(), 10);
    assert!(!input.empty());

    input.move_caret(Move::Home, false, false);
    assert_eq!(input.cursor(), 0);
    input.insert("> ");
    assert_eq!(input.text(), "> hola mundo");
    input.place_caret(500);
    assert_eq!(input.cursor(), 12);
    input.select_all();
    assert_eq!(input.selected_text(), "> hola mundo");
    assert_eq!(input.selection(), Some((0, 12)));
    input.delete(false);
    assert!(input.empty());

    input.insert("  listo  ");
    assert_eq!(input.take(), "listo");
    assert!(input.empty());

    input.insert("uno dos");
    input.move_caret(Move::End, false, true);
    input.backspace(true);
    assert_eq!(input.text(), "uno ");
}

#[test]
fn host_parses_markdown_and_inline_spans() {
    let host = HudHost;
    let blocks =
        host.parse("## Título\n\n**negrita** y `código`\n\n| a | b |\n|---|--:|\n| 1 | 2 |");
    assert!(matches!(&blocks[0], Block::Heading(heading) if heading.level == 2));
    let Block::Paragraph(paragraph) = &blocks[1] else {
        panic!("párrafo en el bloque 1");
    };
    assert!(paragraph
        .spans
        .iter()
        .any(|span| span.bold && span.text == "negrita"));
    assert!(paragraph
        .spans
        .iter()
        .any(|span| span.code && span.text == "código"));
    let Block::Table(table) = &blocks[2] else {
        panic!("tabla en el bloque 2");
    };
    assert_eq!(table.aligns.len(), 2);
    assert_eq!(table.rows.len(), 1);

    let spans = host.parse_inline("hola **x**");
    assert_eq!(spans[0].text, "hola ");
    assert!(spans[1].bold);
}

#[test]
fn host_reads_file_cards_previews_and_images() {
    let host = HudHost;
    let text = TempFile::new("nota.txt");
    text.write(b"uno\ndos\n");
    let roots = vec![temp_dir()];
    let info = host
        .file_info(Some(&text.path), Some(&roots))
        .expect("tarjeta de texto");
    assert_eq!(info.kind, MediaKind::Text);
    assert!(info.allowed);
    assert_eq!(
        host.text_preview(&info),
        Some(("uno\ndos".to_string(), false))
    );

    let outside = host
        .file_info(Some(&text.path), Some(&[temp_dir().join("otra-carpeta")]))
        .expect("tarjeta fuera");
    assert!(!outside.allowed);
    assert!(host.text_preview(&outside).is_none());
    let name = text
        .path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    assert_eq!(host.resolve_target(&name, &roots), Some(text.path.clone()));

    let png = TempFile::new("imagen.png");
    png.write(&png_2x1());
    let card = host
        .file_info(Some(&png.path), Some(&roots))
        .expect("tarjeta de imagen");
    assert_eq!(card.kind, MediaKind::Image);
    let image = host.load_image(&card).expect("imagen PNG");
    assert_eq!((image.width(), image.height()), (2, 1));
    assert_eq!(chat_file_info(&card).size, card.size as i64);

    // El formato no se filtra por la extensión: Pillow abre cualquiera de
    // los que acepta `media.IMAGE_EXTENSIONS`.
    let bmp = TempFile::new("imagen.bmp");
    bmp.write(&bmp_2x1());
    let card = host
        .file_info(Some(&bmp.path), Some(&roots))
        .expect("tarjeta BMP");
    let image = host.load_image(&card).expect("imagen BMP");
    assert_eq!((image.width(), image.height()), (2, 1));
    assert_eq!(image.pixel(0, 0).to_array(), [255, 0, 0, 255]);
    assert_eq!(image.pixel(1, 0).to_array(), [0, 0, 255, 255]);

    // Cabecera GIF válida de 2×1 y cuerpo basura: el decodificador falla y la
    // ficha se queda sin imagen.
    let corrupt = TempFile::new("rota.gif");
    corrupt.write(b"GIF89a\x02\x00\x01\x00 basura");
    let card = host
        .file_info(Some(&corrupt.path), Some(&roots))
        .expect("tarjeta rota");
    assert_eq!(card.kind, MediaKind::Image);
    assert!(host.load_image(&card).is_none());
}

#[test]
fn install_wires_the_panel_and_the_saved_size() {
    let fixture = Fixture::new();
    fixture.app.inner.ui.hud.set_muted(true);
    assert!(fixture.app.inner.chat.chat.mic_muted());
    assert!(!fixture.app.inner.ui.hud.chat_is_open());
    assert_eq!(fixture.app.inner.ui.hud.chat_size(), None);

    let mut cfg = fixture_config();
    cfg.set("chat_width", json!(410.0));
    cfg.set("chat_height", json!(520.0));
    let sized = Fixture::with_config(cfg);
    assert_eq!(sized.app.inner.ui.hud.chat_size(), Some((410.0, 520.0)));
}

#[test]
fn ui_builds_a_view_with_the_shared_model() {
    let fixture = Fixture::new();
    fixture.app.inner.chat.chat.add_user("hola", Source::Chat);
    let ui = HudChatUi::new(&fixture.app, "ctrl+alt+t".to_string());
    assert_eq!(ui.chat_hotkey(), "ctrl+alt+t");
    assert_eq!(ui.scale(), fixture.app.inner.ui.hud.scale());
    assert_eq!(ui.bar_scale(), fixture.app.inner.ui.hud.bs());
    assert!(!ui.in_call());

    let mut view = ui.make_view().expect("vista del chat");
    assert_eq!(view.model.snapshot().1.len(), 1);
    view.input.insert("hola");
    assert_eq!(view.input.text(), "hola");

    let reply = fixture.app.inner.chat.chat.begin_assistant(Source::Chat);
    fixture.app.inner.chat.chat.append(
        reply.id,
        "**Listo**\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```rust\nfn x() {}\n```",
    );
    fixture.app.inner.chat.chat.finish(reply.id);
    view.configure(
        440.0,
        600.0,
        1.0,
        "dark",
        ez_hud::tokens::palette("dark").expect("paleta oscura"),
    );
    let frame = view.render(ez_core::metrics::monotonic());
    assert!(frame.width() > 0 && frame.height() > 0);

    if let Some(fluid) = ui.chat_fluid(4, 3, ez_core::metrics::monotonic()) {
        assert_eq!(fluid.dim(), (3, 4, 3));
    }
}

#[test]
fn events_reach_the_chat_and_the_messenger() {
    let fixture = Fixture::new();
    install_client(
        &fixture.app,
        ScriptedClient::new(vec![Script::Chunks(vec![chunk("Listo.")])]),
    );
    drain_events(&fixture.app);
    let events = HudChatEvents::new(&fixture.app);

    events.emit(ChatEvent::Send("  hola  ".to_string()));
    let event = fixture.app.events().get_nowait().expect("evento de envío");
    assert_eq!(event, AppEvent::ChatSend("  hola  ".to_string()));
    fixture.app.handle_event(event);

    let seen = wait_event(&fixture.app, |event| {
        matches!(event, AppEvent::AnswerDone(..))
    });
    assert!(seen
        .iter()
        .any(|event| matches!(event, AppEvent::AnswerDone(_, outcome) if outcome == "ok")));
    assert_eq!(fixture.app.inner.chat.chat.last_assistant_text(), "Listo.");
    assert_eq!(fixture.app.inner.chat.chat.snapshot().1[0].text, "hola");

    let view = HudChatUi::new(&fixture.app, String::new())
        .make_view()
        .expect("vista del chat");
    assert_eq!(view.model.snapshot().1.len(), 2);
}

#[test]
fn events_stream_through_a_local_llm_server() {
    let fixture = Fixture::new();
    let (base, server) = llm_server("Desde el servidor.");
    let client = OpenAiChatClient::new("", &base, &[], 10.0);
    let messenger = fixture.app.inner.agents.messenger.lock().clone();
    let messenger = messenger.expect("mensajero");
    messenger.set_client(Arc::new(client) as Arc<dyn LlmClient>);
    drain_events(&fixture.app);
    let events = HudChatEvents::new(&fixture.app);

    events.emit(ChatEvent::Send("hola".to_string()));
    let event = fixture.app.events().get_nowait().expect("evento de envío");
    fixture.app.handle_event(event);
    let seen = wait_event(&fixture.app, |event| {
        matches!(event, AppEvent::AnswerDone(..))
    });
    assert!(seen
        .iter()
        .any(|event| matches!(event, AppEvent::AnswerDone(_, outcome) if outcome == "ok")));
    assert_eq!(
        fixture.app.inner.chat.chat.last_assistant_text(),
        "Desde el servidor."
    );
    server.join().expect("servidor del LLM");
}

#[test]
fn events_queue_every_panel_callback() {
    let fixture = Fixture::new();
    drain_events(&fixture.app);
    let events = HudChatEvents::new(&fixture.app);
    events.emit(ChatEvent::New);
    events.emit(ChatEvent::Stop);
    events.emit(ChatEvent::Switch(7));
    events.emit(ChatEvent::Delete(8));
    events.emit(ChatEvent::Confirm(true));
    events.emit(ChatEvent::Mic(false));
    assert_eq!(
        drain_events(&fixture.app),
        vec![
            AppEvent::ChatNew,
            AppEvent::ChatStop,
            AppEvent::ChatSwitch(7),
            AppEvent::ChatDelete(8),
            AppEvent::ConfirmClick(true, None),
            AppEvent::ChatMute(false),
        ]
    );
}

#[test]
fn voice_and_mic_events_apply_immediately() {
    let fixture = Fixture::new();
    drain_events(&fixture.app);
    let events = HudChatEvents::new(&fixture.app);

    events.emit(ChatEvent::Voice(false));
    assert!(!fixture.app.inner.chat.chat.voice_replies());
    assert!(fixture.saved_config().contains("\"chat_voice\": false"));

    events.emit(ChatEvent::Mic(true));
    assert_eq!(
        fixture.app.events().get_nowait(),
        Some(AppEvent::ChatMute(true))
    );
}

#[test]
fn panel_bridge_forwards_mic_and_state() {
    let fixture = Fixture::new();
    let chat = Arc::new(ChatModel::new(None, false));
    let ui = Arc::new(HudChatUi::new(&fixture.app, String::new()));
    let events = Arc::new(HudChatEvents::new(&fixture.app));
    let window = Arc::new(ChatWindowPanel::new(ui, events));
    let bridge = PanelBridge {
        panel: window,
        chat: Arc::clone(&chat),
    };

    assert!(!bridge.is_open());
    bridge.set_mic_muted(true);
    assert!(chat.mic_muted());
    bridge.poll(ez_core::metrics::monotonic());
    bridge.close_now();
}

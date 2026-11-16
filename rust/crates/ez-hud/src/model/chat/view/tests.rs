use ez_gfx::sdf::Layer;

use super::test_support::{sample_view, shared_view, view_with, FakeInput};
use super::*;

fn dark() -> ColorMap {
    crate::tokens::palette("dark").expect("the dark palette exists")
}

fn action_kind(action: &Option<Action>) -> Option<&'static str> {
    action.as_ref().map(|action| action.kind)
}

#[test]
fn configure_keeps_the_messages_unless_the_width_changes() {
    let mut view = sample_view();
    let _ = view.render(0.0);
    assert_eq!(view.cache.len(), 1);
    view.configure(440.0, 700.0, 1.0, "dark", dark());
    assert_eq!(view.cache.len(), 1);
    view.configure(400.0, 600.0, 1.0, "dark", dark());
    assert_eq!(view.cache.len(), 0);
    assert_eq!(view.content_w(), 400.0 - 2.0 * PAD);
}

#[test]
fn a_new_palette_rebuilds_the_cached_messages() {
    let mut view = sample_view();
    let _ = view.render(0.0);
    assert_eq!(view.cache.len(), 1);
    let light = crate::tokens::palette("light").expect("the light palette exists");
    view.configure(440.0, 700.0, 1.0, "dark", light);
    assert_eq!(view.cache.len(), 0);
}

#[test]
fn the_panel_has_the_physical_size_of_the_window() {
    let mut view = sample_view();
    let frame = view.render(0.0);
    assert_eq!(frame.width(), 440);
    assert_eq!(frame.height(), 600);
    assert!(frame.as_raw().iter().any(|value| *value > 0.0));
}

#[test]
fn the_wheel_stops_at_the_laid_out_maximum() {
    let (mut view, model) = shared_view();
    for index in 0..20 {
        model.borrow_mut().messages.push(ChatMessage::new(
            index + 1,
            Role::Assistant,
            format!("mensaje {index}"),
        ));
    }
    let _ = view.render(0.0);
    let max = view.scroll.max();
    assert!(max > 0.0);
    view.wheel(120);
    assert_eq!(view.scroll.offset(), 56.0);
    view.wheel(1200 * 120);
    assert_eq!(view.scroll.offset(), max);
    view.wheel(-1200 * 120);
    assert_eq!(view.scroll.offset(), 0.0);
}

#[test]
fn enter_sends_the_trimmed_text_and_shift_enter_adds_a_line() {
    let mut view = sample_view();
    view.input.insert("  hola  ");
    let action = view.key(13, false, false);
    assert_eq!(
        action,
        Some(Action::with("send", ActionData::Text("hola".to_string())))
    );
    assert!(view.input.text().is_empty());
    assert_eq!(view.key(13, false, true), None);
    assert_eq!(view.input.text(), "\n");
}

#[test]
fn escape_clears_the_field_then_leaves_history_then_closes() {
    let mut view = sample_view();
    view.input.insert("algo");
    assert_eq!(view.key(27, false, false), None);
    assert!(view.input.text().is_empty());
    view.mode = Mode::History;
    assert_eq!(view.key(27, false, false), None);
    assert_eq!(view.mode, Mode::Chat);
    view.model.view_session(Some(2));
    assert!(view.model.viewing().is_some());
    assert_eq!(view.key(27, false, false), None);
    assert!(view.model.viewing().is_none());
    assert_eq!(action_kind(&view.key(27, false, false)), Some("close"));
}

#[test]
fn control_keys_copy_cut_and_select_the_input() {
    let mut view = sample_view();
    view.input.insert("uno dos");
    assert_eq!(view.key(65, true, false), None);
    assert_eq!(view.input.selection(), Some((0, 7)));
    let copied = view.key(67, true, false);
    assert_eq!(
        copied,
        Some(Action::with(
            "copy",
            ActionData::Text("uno dos".to_string())
        ))
    );
    let cut = view.key(88, true, false);
    assert_eq!(
        cut,
        Some(Action::with(
            "copy",
            ActionData::Text("uno dos".to_string())
        ))
    );
    assert!(view.input.text().is_empty());
    assert_eq!(view.key(67, true, false), None);
}

#[test]
fn dropped_files_enter_as_links_with_one_blank_line() {
    let mut view = sample_view();
    view.add_files(&["C:/documentos/informe.pdf".to_string(), String::new()]);
    assert_eq!(
        view.input.text(),
        "[informe.pdf](<C:/documentos/informe.pdf>)\n"
    );
    view.input.clear();
    view.input.insert("hola");
    view.add_files(&["C:/a.txt".to_string()]);
    assert_eq!(view.input.text(), "hola\n[a.txt](<C:/a.txt>)\n");
}

#[test]
fn pointer_moves_switch_the_hover_only_when_it_changes() {
    let mut view = sample_view();
    let _ = view.render(0.0);
    let text = view
        .hits
        .iter()
        .find(|zone| zone.kind == "send")
        .expect("send");
    let (x, y) = text.rect.center();
    assert!(view.pointer_move(x, y));
    assert_eq!(view.hover.as_deref(), Some("send"));
    assert!(view.cursor_is_hand(x, y));
    assert!(!view.pointer_move(x, y));
    assert!(view.pointer_move(1.0, 1.0));
    assert_eq!(view.hover, None);
    assert!(!view.cursor_is_hand(1.0, 1.0));
}

#[test]
fn navigation_and_audio_controls_are_separated_at_every_width() {
    for width in [340.0, 440.0, 760.0] {
        let mut view = sample_view();
        view.configure(width, 600.0, 1.0, "dark", dark());
        view.status = "Esperando tu respuesta".to_string();
        let _ = view.render(0.0);
        let zone = |kind: &str| {
            view.hits
                .iter()
                .find(|candidate| candidate.kind == kind)
                .unwrap_or_else(|| panic!("{kind}"))
                .rect
        };
        for kind in ["settings", "history", "close"] {
            assert!(zone(kind).y1 <= HEADER_H, "{kind} a {width}");
        }
        let input = view.input_box;
        let (mic, voice) = (zone("mic"), zone("voice"));
        for rect in [mic, voice] {
            assert!(rect.x1 < input.x0, "dentro del ancho a {width}");
            assert!(input.y0 <= rect.y0 && rect.y1 <= input.y1 && rect.y1 <= 600.0);
        }
        assert!(mic.x1 < voice.x0, "mic antes que voice a {width}");
        for rect in [zone("settings"), zone("history"), zone("close"), mic, voice] {
            assert!(0.0 <= rect.x0 && rect.x0 < rect.x1 && rect.x1 <= width);
        }
    }
}

#[test]
fn the_send_button_stops_when_busy_or_sends_the_text() {
    let (mut view, model) = shared_view();
    let _ = view.render(0.0);
    let send = view
        .hits
        .iter()
        .find(|zone| zone.kind == "send")
        .expect("send")
        .rect
        .center();
    model.borrow_mut().busy = true;
    let _ = view.render(0.0);
    assert_eq!(
        action_kind(&view.pointer_down(send.0, send.1)),
        Some("stop")
    );
    view.input.insert("hola");
    model.borrow_mut().busy = false;
    let _ = view.render(0.0);
    assert_eq!(
        view.pointer_down(send.0, send.1),
        Some(Action::with("send", ActionData::Text("hola".to_string())))
    );
}

#[test]
fn the_voice_and_mic_buttons_return_their_next_state() {
    let (mut view, model) = shared_view();
    model.borrow_mut().voice = true;
    view.in_call = true;
    let _ = view.render(0.0);
    let voice = view
        .hits
        .iter()
        .find(|zone| zone.kind == "voice")
        .expect("voice")
        .rect
        .center();
    let mic = view
        .hits
        .iter()
        .find(|zone| zone.kind == "mic")
        .expect("mic")
        .rect
        .center();
    assert_eq!(
        view.pointer_down(voice.0, voice.1),
        Some(Action::with("voice", ActionData::Flag(false)))
    );
    assert_eq!(
        view.pointer_down(mic.0, mic.1),
        Some(Action::with("mic", ActionData::Flag(true)))
    );
}

#[test]
fn ctrl_n_only_starts_a_new_conversation_when_the_current_one_has_text() {
    let (mut view, model) = shared_view();
    assert_eq!(view.key(78, true, false), None);
    model
        .borrow_mut()
        .messages
        .push(ChatMessage::new(1, Role::User, "hola"));
    assert_eq!(action_kind(&view.key(78, true, false)), Some("new"));
}

#[test]
fn a_session_row_switches_only_when_it_is_not_the_current_one() {
    let mut view = view_with(
        Vec::new(),
        vec![
            SessionInfo {
                session: 1,
                title: "Actual".to_string(),
                started: 1_735_689_612.0,
                messages: 2,
            },
            SessionInfo {
                session: 2,
                title: "Otra".to_string(),
                started: 1_735_689_612.0,
                messages: 3,
            },
        ],
    );
    view.mode = Mode::History;
    let _ = view.render(0.0);
    let rows: Vec<_> = view
        .hits
        .iter()
        .filter(|zone| zone.kind == "session")
        .map(|zone| (zone.rect.center(), zone.data.clone()))
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        view.pointer_down(rows[0].0 .0, rows[0].0 .1),
        None,
        "the current conversation just returns to chat"
    );
    assert_eq!(view.mode, Mode::Chat);
    view.mode = Mode::History;
    let _ = view.render(0.0);
    assert_eq!(
        view.pointer_down(rows[1].0 .0, rows[1].0 .1),
        Some(Action::with("switch", ActionData::Session(2)))
    );
}

#[test]
fn deleting_a_history_row_asks_then_confirms() {
    let mut view = view_with(
        Vec::new(),
        vec![SessionInfo {
            session: 2,
            title: "Otra".to_string(),
            started: 1_735_689_612.0,
            messages: 3,
        }],
    );
    view.mode = Mode::History;
    let _ = view.render(0.0);
    let ask = view
        .hits
        .iter()
        .find(|zone| zone.kind == "delete_ask")
        .expect("delete_ask")
        .rect
        .center();
    assert_eq!(view.pointer_down(ask.0, ask.1), None);
    assert_eq!(view.pending_delete(), Some(2));
    let _ = view.render(0.0);
    let yes = view
        .hits
        .iter()
        .find(|zone| zone.kind == "delete_yes")
        .expect("delete_yes")
        .rect
        .center();
    assert_eq!(
        view.pointer_down(yes.0, yes.1),
        Some(Action::with("delete", ActionData::Session(2)))
    );
    assert_eq!(view.pending_delete(), None);
    assert!(view.deleting.is_some());
}

#[test]
fn leaving_the_history_cancels_a_pending_delete() {
    let mut view = view_with(
        Vec::new(),
        vec![SessionInfo {
            session: 2,
            title: "Otra".to_string(),
            started: 1_735_689_612.0,
            messages: 3,
        }],
    );
    view.mode = Mode::History;
    let _ = view.render(0.0);
    let ask = view
        .hits
        .iter()
        .find(|zone| zone.kind == "delete_ask")
        .expect("delete_ask")
        .rect
        .center();
    assert_eq!(view.pointer_down(ask.0, ask.1), None);
    assert_eq!(view.pending_delete(), Some(2));
    let history = view
        .hits
        .iter()
        .find(|zone| zone.kind == "history")
        .expect("history")
        .rect
        .center();
    assert_eq!(view.pointer_down(history.0, history.1), None);
    assert_eq!(view.pending_delete(), None);
}

#[test]
fn animating_follows_busy_confirm_flash_and_fade() {
    let (mut view, model) = shared_view();
    assert!(!view.animating(10.0));
    model.borrow_mut().busy = true;
    assert!(view.animating(10.0));
    model.borrow_mut().busy = false;
    model.borrow_mut().confirm = Some(ConfirmCard {
        summary: "¿Seguro?".to_string(),
        strong: true,
    });
    assert!(view.animating(10.0));
    model.borrow_mut().confirm = None;
    view.flash_copied("hola", Some(10.0));
    assert!(view.animating(11.0));
    assert!(!view.animating(12.0));
}

#[test]
fn the_flash_token_truncates_to_twenty_characters() {
    let mut view = sample_view();
    assert_eq!(view.flash_token(), (String::new(), false));
    view.flash_copied("0123456789abcdefghij más", Some(monotonic() + 10.0));
    assert_eq!(view.flash_token().0, "0123456789abcdefghij");
    assert!(view.flash_token().1);
}

#[test]
fn a_note_that_wraps_stays_inside_the_column() {
    let mut view = view_with(Vec::new(), Vec::new());
    let note = ChatMessage::new(1, Role::System, "Conversación compactada. ".repeat(12));
    let entry = view.entry(&note, 0.0);
    assert!(entry.height > 30.0);
    assert!(entry.x + f64::from(entry.image.width()) <= 440.0);
}

#[test]
fn the_input_box_grows_up_to_six_lines() {
    let mut view = sample_view();
    view.input.insert("una\nsegunda\ntercera");
    let _ = view.render(0.0);
    assert_eq!(view.input_lines.len(), 3);
    assert_eq!(view.input_box.height(), input_box_height(3));
    view.input.insert("\ncuatro\ncinco\nseis\nsiete\nocho");
    let _ = view.render(0.0);
    assert_eq!(view.input_lines.len(), 8);
    assert_eq!(view.input_box.height(), input_box_height(INPUT_MAX_LINES));
}

#[test]
fn clicking_the_input_box_places_the_caret() {
    let mut view = sample_view();
    view.input.insert("hola mundo");
    let _ = view.render(0.0);
    let input = view.input_box;
    assert_eq!(
        view.pointer_down(input.x0 + 2.0, input.y0 + INPUT_MIN / 2.0),
        None
    );
    assert_eq!(view.input.cursor(), 0);
    view.pointer_down(input.x0 + 100.0, input.y0 + INPUT_MIN / 2.0);
    assert_eq!(view.input.cursor(), 10);
}
#[test]
fn assistant_and_user_messages_both_produce_copy_zones() {
    let mut view = view_with(
        vec![
            ChatMessage::new(1, Role::User, "hola"),
            ChatMessage::new(2, Role::Assistant, "respuesta"),
        ],
        Vec::new(),
    );
    let _ = view.render(0.0);
    let copies: Vec<_> = view
        .hits
        .iter()
        .filter(|zone| zone.kind == "copy_msg")
        .collect();
    assert_eq!(copies.len(), 2);
    assert_eq!(copies[0].data, Some(ZoneData::Text("hola".to_string())));
    assert!(copies
        .iter()
        .all(|zone| zone.rect.x1 - zone.rect.x0 <= 32.0));
}

#[test]
fn a_note_is_centred_and_has_no_copy_zone() {
    let mut view = view_with(
        vec![ChatMessage::new(1, Role::System, "conversación compactada")],
        Vec::new(),
    );
    let _ = view.render(0.0);
    assert!(!view.hits.iter().any(|zone| zone.kind == "copy_msg"));
    let message = view.model.snapshot().1[0].clone();
    let entry = view.entry(&message, 0.0);
    assert!(entry.x > PAD);
}

#[test]
fn the_entry_is_cached_until_the_text_changes() {
    let mut view = sample_view();
    let message = view.model.snapshot().1[0].clone();
    let first = view.entry(&message, 0.0);
    let second = view.entry(&message, 0.0);
    assert!(std::rc::Rc::ptr_eq(&first, &second));
    let mut changed = message.clone();
    changed.text = "otra cosa".to_string();
    let third = view.entry(&changed, 0.0);
    assert!(!std::rc::Rc::ptr_eq(&first, &third));
}

#[test]
fn a_streaming_message_reuses_its_tile_for_fifty_milliseconds() {
    let mut view = view_with(
        vec![{
            let mut message = ChatMessage::new(1, Role::Assistant, "hola");
            message.streaming = true;
            message
        }],
        Vec::new(),
    );
    let message = view.model.snapshot().1[0].clone();
    let first = view.entry(&message, 10.0);
    let mut grown = message.clone();
    grown.text = "hola mundo".to_string();
    let batched = view.entry(&grown, 10.02);
    assert!(std::rc::Rc::ptr_eq(&first, &batched));
    let fresh = view.entry(&grown, 10.1);
    assert!(!std::rc::Rc::ptr_eq(&first, &fresh));
}

#[test]
fn editing_keys_match_the_python_sequence() {
    let mut view = sample_view();
    view.input.insert("uno dos");
    assert_eq!(view.key(37, false, false), None);
    assert_eq!(view.key(8, false, false), None);
    assert_eq!(view.input.text(), "uno ds");
    assert_eq!(view.key(36, false, false), None);
    assert_eq!(view.key(46, false, false), None);
    assert_eq!(view.input.text(), "no ds");
    assert_eq!(view.key(8, true, false), None);
    assert_eq!(view.key(35, false, true), None);
    assert_eq!(view.input.selected_text(), "no ds");
}

#[test]
fn page_keys_scroll_the_messages() {
    let (mut view, model) = shared_view();
    for index in 0..20 {
        model.borrow_mut().messages.push(ChatMessage::new(
            index + 1,
            Role::Assistant,
            format!("mensaje {index}"),
        ));
    }
    let _ = view.render(0.0);
    assert_eq!(view.key(33, false, false), None);
    assert!(view.scroll.offset() > 0.0);
    assert_eq!(view.key(34, false, false), None);
    assert_eq!(view.scroll.offset(), 0.0);
}

#[test]
fn close_new_and_voice_buttons_return_their_actions() {
    let mut view = view_with(vec![ChatMessage::new(1, Role::User, "hola")], Vec::new());
    let _ = view.render(0.0);
    let click = |view: &mut ChatView, kind: &str| {
        let rect = view
            .hits
            .iter()
            .find(|zone| zone.kind == kind)
            .unwrap_or_else(|| panic!("{kind}"))
            .rect;
        let (x, y) = rect.center();
        view.pointer_down(x, y)
    };
    assert_eq!(action_kind(&click(&mut view, "close")), Some("close"));
    assert_eq!(action_kind(&click(&mut view, "history")), None);
    assert_eq!(action_kind(&click(&mut view, "new")), Some("new"));
    assert_eq!(view.mode, Mode::Chat);
    let voice = click(&mut view, "voice").expect("voice");
    assert_eq!(voice.kind, "voice");
    assert_eq!(voice.data, Some(ActionData::Flag(true)));
}

#[test]
fn home_and_end_keys_move_the_caret() {
    let mut view = sample_view();
    view.input.insert("uno dos");
    assert_eq!(view.key(36, false, false), None);
    assert_eq!(view.input.cursor(), 0);
    assert_eq!(view.key(35, false, false), None);
    assert_eq!(view.input.cursor(), 7);
}

#[test]
fn typing_control_characters_is_ignored() {
    let mut view = sample_view();
    view.char("\u{1}");
    assert!(view.input.text().is_empty());
    view.char("ñ");
    assert_eq!(view.input.text(), "ñ");
    view.char("");
    assert_eq!(view.input.text(), "ñ");
}

#[test]
fn the_panel_can_be_empty() {
    let mut view = view_with(Vec::new(), Vec::new());
    let frame = view.render(0.0);
    assert_eq!(frame.width(), 440);
    assert_eq!(view.scroll.max(), 0.0);
}

#[test]
fn the_populated_layer_is_a_float32_rgba_buffer() {
    let frame: Layer = sample_view().render(0.0);
    assert_eq!(frame.as_raw().len(), 440 * 600 * 4);
}

#[test]
fn the_fake_input_behaves_like_the_text_field() {
    let mut input = FakeInput::with_text("uno dos");
    input.select_all();
    assert_eq!(input.selected_text(), "uno dos");
    input.backspace(false);
    assert!(input.text().is_empty());
    input.insert("hola\n");
    assert!(!input.empty());
    assert_eq!(input.take(), "hola");
    assert!(input.empty());
}

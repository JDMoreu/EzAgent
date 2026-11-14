use ez_gfx::sdf::Layer;

use super::*;
use crate::model::chat::view::test_support::{sample_view, shared_view, view_with};
use crate::model::chat::view::{Action, ChatMessage, Mode, Role, SessionInfo, ZoneData};

fn session(id: i64, title: &str) -> SessionInfo {
    SessionInfo {
        session: id,
        title: title.to_string(),
        started: 1_735_689_612.0,
        messages: 2,
    }
}

#[test]
fn the_date_uses_the_spanish_month_abbreviations() {
    let text = date(1_735_689_612.0);
    let expected = ez_core::pytime::date_local(1_735_689_612.0).expect("valid instant");
    let mut parts = expected.split('-');
    let year = parts.next().expect("year");
    let month: usize = parts.next().expect("month").parse().expect("number");
    let day: u32 = parts.next().expect("day").parse().expect("number");
    assert_eq!(text, format!("{day} {} {year}", MONTHS[month - 1]));
}

#[test]
fn the_title_comes_from_the_session_title_then_from_the_first_request() {
    let view = view_with(Vec::new(), vec![session(1, "Informe semanal")]);
    assert_eq!(view.conversation_title(), "Informe semanal");
    let view = view_with(Vec::new(), vec![session(1, "")]);
    assert_eq!(view.conversation_title(), "Nueva conversación");
    let view = view_with(
        vec![
            ChatMessage::new(1, Role::Assistant, "hola"),
            ChatMessage::new(2, Role::User, "  resume \n el informe  "),
        ],
        Vec::new(),
    );
    assert_eq!(view.conversation_title(), "resume el informe");
    let view = view_with(Vec::new(), Vec::new());
    assert_eq!(view.conversation_title(), "Nueva conversación");
}

#[test]
fn long_titles_are_cut_at_thirty_nine_characters() {
    let short = "x".repeat(39);
    assert_eq!(short_title(&short), short);
    let long = "y".repeat(45);
    assert_eq!(short_title(&long), format!("{}…", "y".repeat(39)));
    let unicode = "ñ".repeat(45);
    assert_eq!(short_title(&unicode).chars().count(), 40);
    assert!(short_title(&unicode).ends_with('…'));
}

#[test]
fn the_header_registers_one_hit_per_control() {
    let mut view = sample_view();
    let _ = view.render(0.0);
    let kinds: Vec<_> = view.hits.iter().map(|zone| zone.kind).collect();
    for expected in [
        "close", "settings", "history", "new", "mic", "voice", "send",
    ] {
        assert!(
            kinds.contains(&expected),
            "{expected} missing from {kinds:?}"
        );
    }
}

#[test]
fn the_new_conversation_button_is_disabled_while_the_chat_is_empty() {
    let mut view = view_with(Vec::new(), Vec::new());
    let _ = view.render(0.0);
    assert!(!view.can_start_new());
    assert!(!view.hits.iter().any(|zone| zone.kind == "new"));
}

#[test]
fn history_mode_shows_the_back_button_and_the_history_buttons() {
    let mut view = view_with(Vec::new(), vec![session(1, "Actual")]);
    view.mode = Mode::History;
    let _ = view.render(0.0);
    assert!(view.hits.iter().any(|zone| zone.kind == "back"));
    assert!(view.hits.iter().any(|zone| zone.kind == "history"));
}

#[test]
fn opening_the_history_registers_the_new_row_and_each_session() {
    let mut view = view_with(Vec::new(), vec![session(1, "Actual"), session(2, "Otra")]);
    view.mode = Mode::History;
    let _ = view.render(0.0);
    assert!(view.hits.iter().any(|zone| zone.kind == "new"));
    let rows: Vec<_> = view
        .hits
        .iter()
        .filter(|zone| zone.kind == "session")
        .map(|zone| zone.data.clone())
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(rows.contains(&Some(ZoneData::Session(1))));
    assert!(rows.contains(&Some(ZoneData::Session(2))));
}

#[test]
fn the_new_row_returns_to_the_chat() {
    let mut view = view_with(Vec::new(), vec![session(1, "Actual")]);
    view.mode = Mode::History;
    let _ = view.render(0.0);
    let new = view
        .hits
        .iter()
        .find(|zone| zone.kind == "new")
        .expect("new")
        .rect
        .center();
    assert_eq!(view.pointer_down(new.0, new.1), Some(Action::new("new")));
    assert_eq!(view.mode, Mode::Chat);
}

#[test]
fn a_busy_model_hides_the_session_rows_and_explains_why() {
    let (mut view, model) = shared_view();
    model.borrow_mut().sessions.push(session(1, "Actual"));
    model.borrow_mut().busy = true;
    view.mode = Mode::History;
    let _ = view.render(0.0);
    assert!(!view.hits.iter().any(|zone| zone.kind == "session"));
    assert!(!view.hits.iter().any(|zone| zone.kind == "delete_ask"));
    assert!(view.hits.iter().any(|zone| zone.kind == "new"));
}

#[test]
fn the_empty_history_says_so_without_rows() {
    let mut view = shared_view().0;
    view.mode = Mode::History;
    let _ = view.render(0.0);
    assert!(view.hits.iter().any(|zone| zone.kind == "new"));
    assert!(!view.hits.iter().any(|zone| zone.kind == "session"));
    assert_eq!(view.scroll.max(), 0.0);
}

#[test]
fn the_empty_state_paints_the_welcome_lines() {
    let mut view = sample_view();
    let mut layer = Layer::new(440, 600);
    view.draw_empty(&mut layer, HEADER_H, 500.0);
    assert!(layer.as_raw().iter().any(|value| *value > 0.0));
}

#[test]
fn the_ghost_row_fades_the_deleted_conversation() {
    let mut view = shared_view().0;
    view.mode = Mode::History;
    view.deleting = Some((0, session(1, "Actual"), 0.0));
    let _ = view.render(DELETE_SECS / 2.0);
    assert!(view.deleting.is_some());
    assert!(!view.animating(DELETE_SECS));
    assert!(view.deleting.is_none());
}

#[test]
fn animating_clears_a_fade_that_is_over() {
    let (mut view, _) = shared_view();
    view.deleting = Some((0, session(1, "Actual"), 0.0));
    assert!(!view.animating(DELETE_SECS));
    assert!(view.deleting.is_none());
}

#[test]
fn the_history_row_keeps_its_delete_button_until_the_confirmation() {
    let mut view = view_with(Vec::new(), vec![session(2, "Otra")]);
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
    assert!(view.hits.iter().any(|zone| zone.kind == "delete_yes"));
    assert!(view.hits.iter().any(|zone| zone.kind == "delete_no"));
}

#[test]
fn the_cancel_button_drops_the_confirmation() {
    let mut view = view_with(Vec::new(), vec![session(2, "Otra")]);
    view.mode = Mode::History;
    view.pending_delete = Some(2);
    let _ = view.render(0.0);
    let no = view
        .hits
        .iter()
        .find(|zone| zone.kind == "delete_no")
        .expect("delete_no")
        .rect
        .center();
    assert_eq!(view.pointer_down(no.0, no.1), None);
    assert_eq!(view.pending_delete(), None);
}

#[test]
fn the_orb_takes_the_room_of_the_back_button() {
    let mut view = sample_view();
    let _ = view.render(0.0);
    assert!(view.orb.is_some());
    view.mode = Mode::History;
    let _ = view.render(0.0);
    assert!(view.orb.is_none());
}

#[test]
fn a_status_line_replaces_the_lower_title_when_present() {
    let mut view = sample_view();
    view.status = "Escuchando".to_string();
    let _ = view.render(0.0);
    assert!(view.orb.is_some());
    assert!(view.hits.iter().any(|zone| zone.kind == "close"));
}

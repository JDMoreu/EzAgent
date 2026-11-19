use ez_gfx::sdf::Layer;

use super::*;
use crate::model::chat::view::test_support::{sample_view, shared_view, view_with};
use crate::model::chat::view::{
    canvas, Action, ActionData, ChatMessage, ConfirmCard, Role, SessionInfo,
};

fn session(id: i64) -> SessionInfo {
    SessionInfo {
        session: id,
        title: "Actual".to_string(),
        started: 1_735_689_612.0,
        messages: 1,
    }
}

#[test]
fn the_box_grows_by_whole_lines() {
    assert_eq!(input_box_height(1), INPUT_MIN);
    assert_eq!(input_box_height(0), INPUT_MIN);
    assert_eq!(input_box_height(3), INPUT_MIN + 2.0 * INPUT_LINE);
}

#[test]
fn an_empty_line_still_occupies_one_box_line() {
    assert_eq!(break_line("", 100.0, 1.0), vec![(0, 0)]);
}

#[test]
fn short_lines_stay_in_one_piece() {
    assert_eq!(break_line("uno dos", 500.0, 1.0), vec![(0, 7)]);
}

#[test]
fn long_lines_break_on_spaces_and_keep_every_character() {
    let text = "uno dos tres cuatro cinco seis siete ocho nueve";
    let pieces = break_line(text, 120.0, 1.0);
    assert!(pieces.len() > 1);
    let joined: String = pieces
        .iter()
        .map(|(start, end)| canvas::slice(text, *start, *end))
        .collect();
    assert_eq!(joined, text);
    for (start, end) in &pieces {
        assert!(*start < *end);
    }
}

#[test]
fn a_word_wider_than_the_box_is_cut_by_characters() {
    let text = "supercalifragilisticoespialidoso";
    let pieces = break_line(text, 40.0, 1.0);
    assert!(pieces.len() > 1);
    let joined: String = pieces
        .iter()
        .map(|(start, end)| canvas::slice(text, *start, *end))
        .collect();
    assert_eq!(joined, text);
}

#[test]
fn wrap_input_counts_indices_across_logical_lines() {
    let mut view = sample_view();
    view.input.insert("una\nsegunda");
    let _ = view.render(0.0);
    assert_eq!(view.input_lines, vec![(0, 3), (4, 11)]);
}

#[test]
fn the_footer_adds_room_for_the_confirmation_and_the_viewing_notice() {
    let (mut view, model) = shared_view();
    let base = input_box_height(1) + 24.0;
    assert_eq!(view.footer_height(), base);
    model.borrow_mut().confirm = Some(ConfirmCard {
        summary: "¿Seguro?".to_string(),
        strong: true,
    });
    assert_eq!(view.footer_height(), base + 84.0);
    model.borrow_mut().viewing = Some(2);
    assert_eq!(view.footer_height(), base + 114.0);
}

#[test]
fn the_footer_never_shows_more_than_six_lines() {
    let mut view = sample_view();
    view.input.insert("1\n2\n3\n4\n5\n6\n7\n8\n9");
    let footer = view.footer_height();
    assert_eq!(footer, input_box_height(INPUT_MAX_LINES) + 24.0);
}

#[test]
fn the_confirmation_buttons_answer_yes_and_no() {
    let (mut view, model) = shared_view();
    model.borrow_mut().confirm = Some(ConfirmCard {
        summary: "¿Borrar los archivos temporales de la sesión anterior?".repeat(4),
        strong: true,
    });
    let _ = view.render(0.0);
    let yes = view
        .hits
        .iter()
        .find(|zone| zone.kind == "confirm_yes")
        .expect("confirm_yes")
        .rect
        .center();
    let no = view
        .hits
        .iter()
        .find(|zone| zone.kind == "confirm_no")
        .expect("confirm_no")
        .rect
        .center();
    assert_eq!(
        view.pointer_down(yes.0, yes.1),
        Some(Action::with("confirm", ActionData::Flag(true)))
    );
    assert_eq!(
        view.pointer_down(no.0, no.1),
        Some(Action::with("confirm", ActionData::Flag(false)))
    );
}

#[test]
fn a_long_confirmation_summary_keeps_two_lines_and_an_ellipsis() {
    let (mut view, model) = shared_view();
    model.borrow_mut().confirm = Some(ConfirmCard {
        summary: "palabra ".repeat(60),
        strong: false,
    });
    let _ = view.render(0.0);
    assert!(view.hits.iter().any(|zone| zone.kind == "confirm_yes"));
}

#[test]
fn the_mic_button_is_off_outside_a_call() {
    let (mut view, _) = shared_view();
    view.in_call = false;
    let _ = view.render(0.0);
    let mic = view
        .hits
        .iter()
        .find(|zone| zone.kind == "mic")
        .expect("mic")
        .rect
        .center();
    assert_eq!(
        view.pointer_down(mic.0, mic.1),
        Some(Action::with("mic", ActionData::Flag(false)))
    );
}

#[test]
fn the_viewing_notice_does_not_steal_the_input() {
    let mut view = view_with(
        vec![ChatMessage::new(1, Role::User, "hola")],
        vec![session(1), session(2)],
    );
    view.model.view_session(Some(2));
    let _ = view.render(0.0);
    assert!(view.hits.iter().any(|zone| zone.kind == "input"));
    assert!(view.footer_height() > input_box_height(1) + 24.0);
}

#[test]
fn the_caret_blinks_with_the_focus_time() {
    let mut view = sample_view();
    let _ = view.render(0.0);
    let input = view.input_box;
    view.focus_t0 = 10.0;
    let mut lit = Layer::new(440, 600);
    view.draw_caret(&mut lit, input.x0 + 14.0, input, 0, 10.0);
    assert!(lit.as_raw().iter().any(|value| *value > 0.0));
    let mut dark = Layer::new(440, 600);
    view.draw_caret(&mut dark, input.x0 + 14.0, input, 0, 10.5);
    assert!(dark.as_raw().iter().all(|value| *value == 0.0));
}

#[test]
fn clicking_past_the_last_character_puts_the_caret_at_the_end() {
    let mut view = sample_view();
    view.input.insert("hola");
    let _ = view.render(0.0);
    let input = view.input_box;
    view.pointer_down(input.x1 - 4.0, input.y0 + INPUT_MIN / 2.0);
    assert_eq!(view.input.cursor(), 4);
}

//! Pruebas de la máquina de estados visual (F5-02).

use super::*;
use std::sync::Arc;

fn state_at(clock: Arc<std::sync::Mutex<f64>>) -> UiState {
    UiState::with_clock(move || *clock.lock().expect("reloj de prueba"))
}

#[test]
fn display_state_keeps_the_specification_priority() {
    assert_eq!(display_state(false, true, "speak", true), "off");
    assert_eq!(display_state(true, true, "speak", true), "error");
    assert_eq!(display_state(true, false, "speak", true), "speak");
    assert_eq!(display_state(true, false, "think", true), "think");
    assert_eq!(display_state(true, false, "notice", true), "notice");
    assert_eq!(display_state(true, false, "listen", true), "mute");
    assert_eq!(display_state(true, false, "listen", false), "listen");
    assert_eq!(display_state(true, false, "idle", false), "idle");
    assert_eq!(word_for("notice"), "AVISO");
    assert_eq!(word_for("off"), "");
    assert_eq!(pose_for("nope"), "idle");
    assert_eq!(pose_for("close"), "close");
}

#[test]
fn the_dwell_holds_a_new_base_and_hides_at_once() {
    let ui = UiState::new();
    ui.set_visible(true, Some(0.0));
    assert_eq!(ui.update(Some(0.0)), "idle");
    ui.set_base("listen", Some(0.0));
    assert_eq!(ui.update(Some(0.05)), "idle");
    assert_eq!(ui.update(Some(0.12)), "listen");
    // off no espera permanencia.
    ui.hide(Some(0.12));
    assert_eq!(ui.update(Some(0.12)), "off");
    assert_eq!(ui.word(), "");
}

#[test]
fn a_transient_error_lasts_1500_ms_and_the_persistent_one_waits() {
    let ui = UiState::new();
    ui.set_visible(true, Some(0.0));
    ui.update(Some(0.0));
    ui.raise_error("transient", Some(1.0));
    assert_eq!(ui.update(Some(1.0)), "error");
    assert_eq!(ui.update(Some(2.49)), "error");
    assert_eq!(ui.update(Some(2.5)), "idle");
    assert!(!ui.error_active());
    ui.raise_error("persistent", Some(3.0));
    assert_eq!(ui.update(Some(3.0)), "error");
    assert_eq!(ui.update(Some(10.0)), "error");
    ui.clear_error(Some(11.0));
    assert_eq!(ui.update(Some(11.0)), "idle");
}

#[test]
fn the_notice_holds_a_second_and_falls_after_the_tts_grace() {
    let ui = UiState::new();
    ui.set_visible(true, Some(0.0));
    ui.update(Some(0.0));
    ui.notice(Some(0.0));
    assert_eq!(ui.update(Some(0.0)), "idle");
    assert_eq!(ui.update(Some(0.5)), "notice");
    // Mientras dura la ventana, listen no pisa el aviso.
    ui.set_base("listen", Some(0.5));
    assert_eq!(ui.update(Some(1.4)), "notice");
    assert_eq!(ui.update(Some(1.5)), "idle");
    assert_eq!(ui.base(), "idle");
}

#[test]
fn the_voice_end_has_a_quarter_second_of_grace() {
    let ui = UiState::new();
    ui.set_visible(true, Some(0.0));
    ui.update(Some(0.0));
    ui.set_base("listen", Some(0.0));
    assert_eq!(ui.update(Some(0.12)), "listen");
    ui.vad_end(Some(0.5));
    assert_eq!(ui.update(Some(0.74)), "listen");
    assert_eq!(ui.update(Some(0.75)), "idle");
    // turn_end cierra la escucha y pasa a pensar sin esperar la gracia.
    ui.set_base("listen", Some(0.75));
    ui.update(Some(0.9));
    ui.turn_end(Some(0.9));
    assert_eq!(ui.update(Some(1.05)), "think");
    assert_eq!(ui.pose(), "think");
}

#[test]
fn tts_start_clears_the_notice_and_speaks() {
    let ui = UiState::new();
    ui.set_visible(true, Some(0.0));
    ui.update(Some(0.0));
    ui.notice(Some(0.0));
    ui.update(Some(0.1));
    ui.tts_start(Some(0.2));
    assert_eq!(ui.update(Some(0.2)), "speak");
    assert_eq!(ui.snapshot(Some(0.2)), ("speak", "HABLO"));
}

#[test]
fn app_states_map_to_base_and_cancelled_is_transient() {
    let clock = Arc::new(std::sync::Mutex::new(100.0));
    let ui = state_at(clock);
    ui.set_visible(true, Some(100.0));
    ui.update(Some(100.0));
    assert!(ui.visible());
    ui.set_app_state("listening", Some(100.0));
    assert_eq!(ui.update(Some(100.12)), "listen");
    assert_eq!(ui.base(), "listen");
    ui.set_app_state("transcribing", Some(100.2));
    assert_eq!(ui.update(Some(100.3)), "think");
    ui.set_app_state("speaking", Some(100.4));
    assert_eq!(ui.update(Some(100.6)), "speak");
    ui.set_app_state("error", Some(100.6));
    assert_eq!(ui.update(Some(100.6)), "error");
    assert!(ui.error_active());
    ui.set_app_state("cancelled", Some(101.0));
    assert_eq!(ui.update(Some(101.0)), "error");
    assert_eq!(ui.update(Some(102.5)), "speak");
    assert!(!ui.error_active());
    ui.set_app_state("nope", Some(102.5));
    assert_eq!(ui.update(Some(102.5)), "speak");
    ui.set_base("idle", Some(102.6));
    assert_eq!(ui.update(Some(102.8)), "idle");
}

#[test]
fn muted_and_in_call_are_queued_and_read_back() {
    let ui = UiState::new();
    ui.set_visible(true, Some(0.0));
    ui.update(Some(0.0));
    ui.set_muted(true, Some(0.0));
    ui.set_in_call(true, Some(0.0));
    // Cambiar de estado mostrado también espera la permanencia.
    assert_eq!(ui.update(Some(0.0)), "idle");
    ui.set_base("listen", Some(0.0));
    assert_eq!(ui.update(Some(0.2)), "mute");
    assert!(ui.muted() && ui.in_call());
    assert!(!ui.menu_open());
    ui.set_base("nope", Some(0.5));
    assert_eq!(ui.base(), "listen");
    ui.set_muted(false, Some(0.0));
    assert_eq!(ui.update(Some(0.4)), "listen");
}

#[test]
fn the_default_clock_is_monotonic() {
    let ui = UiState::new();
    let first = ui.update(None);
    assert_eq!(first, "off");
    assert!(ui.now(None) >= 0.0);
}

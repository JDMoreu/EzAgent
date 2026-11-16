use super::*;

#[test]
fn packed_levels_round_trip() {
    let reference = PlaybackReference::new();
    reference.begin();
    reference.publish_at(1_000, 0.25);
    reference.publish_at(1_030, 0.5);
    let peak = reference.peak_at(1_040, 0.35).expect("hay eco");
    assert!((peak - 0.5).abs() < 1e-5);
}

#[test]
fn the_window_forgets_old_blocks() {
    let reference = PlaybackReference::new();
    reference.begin();
    reference.publish_at(1_000, 0.5);
    reference.publish_at(2_000, 0.3);
    // 350 ms de ventana: el bloque de 1_000 quedó fuera.
    let peak = reference.peak_at(2_100, 0.35).expect("hay eco");
    assert!((peak - 0.3).abs() < 1e-5);
    assert_eq!(reference.peak_at(2_400, 0.35), None);
}

#[test]
fn a_finished_phrase_has_no_peak() {
    let reference = PlaybackReference::new();
    reference.begin();
    reference.publish(0.4);
    reference.end();
    assert!(!reference.is_active());
    assert_eq!(reference.peak_in_window(REFERENCE_WINDOW_SECS), None);
}

#[test]
fn begin_starts_a_new_phrase_and_clears_the_envelope() {
    let reference = PlaybackReference::new();
    reference.begin();
    reference.publish(0.4);
    let first = reference.phrase();
    reference.begin();
    assert!(reference.phrase() > first);
    assert_eq!(reference.peak_in_window(REFERENCE_WINDOW_SECS), None);
}

#[test]
fn percentile_interpolates_like_numpy() {
    assert_eq!(percentile(&[], 90.0), 0.0);
    assert!((percentile(&[1.0], 90.0) - 1.0).abs() < 1e-12);
    assert!((percentile(&[1.0, 2.0], 90.0) - 1.9).abs() < 1e-12);
    assert!((percentile(&[1.0, 2.0, 3.0], 50.0) - 2.0).abs() < 1e-12);
}

#[test]
fn echo_only_blocks_teach_the_coupling() {
    let mut adaptive = AdaptiveBargeIn::default();
    assert!(adaptive.observe_phrase(1));
    for _ in 0..20 {
        adaptive.threshold(0.004, Some(0.01), 0.0005);
    }
    assert!((adaptive.coupling() - 0.4).abs() < 0.01);
    assert!(!adaptive.observe_phrase(1), "la misma frase no reinicia");
}

#[test]
fn learning_stops_when_the_user_is_clearly_louder() {
    let mut adaptive = AdaptiveBargeIn::default();
    adaptive.observe_phrase(1);
    // Voz por encima del umbral: no es eco, no se aprende de ella.
    for _ in 0..40 {
        adaptive.threshold(0.05, Some(0.01), 0.0005);
    }
    assert_eq!(
        adaptive.coupling(),
        AdaptiveSettings::default().initial_coupling
    );
}

#[test]
fn the_threshold_keeps_the_noise_floor_and_the_absolute_minimum() {
    let mut adaptive = AdaptiveBargeIn::default();
    adaptive.observe_phrase(1);
    let threshold = adaptive.threshold(0.0001, None, 0.003);
    assert!((threshold - 0.003 * 1.8).abs() < 1e-12);
    let mut quiet = AdaptiveBargeIn::default();
    quiet.observe_phrase(1);
    let threshold = quiet.threshold(0.0001, None, 0.0);
    assert!((threshold - 0.0012).abs() < 1e-12);
}

#[test]
fn the_echo_margin_sets_the_threshold_while_the_reference_sounds() {
    let mut adaptive = AdaptiveBargeIn::default();
    adaptive.observe_phrase(1);
    let threshold = adaptive.threshold(0.0001, Some(0.004), 0.0);
    assert!((threshold - 0.004 * 0.9 * 1.25).abs() < 1e-12);
}

#[test]
fn the_coupling_learned_from_echo_is_clamped() {
    let mut adaptive = AdaptiveBargeIn::default();
    adaptive.observe_phrase(1);
    // Un «eco» casi nulo no puede apagar el detector: mínimo 0,05.
    for _ in 0..20 {
        adaptive.threshold(0.0001, Some(0.01), 0.0);
    }
    assert!((adaptive.coupling() - 0.05).abs() < 1e-12);
}

#[test]
fn the_summary_reports_the_phrase_and_resets() {
    let mut adaptive = AdaptiveBargeIn::default();
    assert_eq!(adaptive.summary(), None);
    adaptive.observe_phrase(1);
    adaptive.threshold(0.004, Some(0.01), 0.0);
    adaptive.threshold(0.006, Some(0.01), 0.0);
    adaptive.note_barge();
    let summary = adaptive.summary().expect("hubo bloques");
    assert!((summary.mic_max - 0.006).abs() < 1e-12);
    assert!(summary.echo_max > 0.0);
    assert!(summary.barged);
    assert_eq!(adaptive.summary(), None);
}

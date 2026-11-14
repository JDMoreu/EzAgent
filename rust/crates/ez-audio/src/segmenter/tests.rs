//! Pruebas del segmentador con audio sintético, sin micrófono.

use super::*;

fn chunk(value: i16) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

/// Un chunk que no se calla y otro que sí (silencio digital).
fn loud() -> Vec<u8> {
    chunk(8000)
}

fn quiet() -> Vec<u8> {
    chunk(0)
}

/// El primer chunk fija el suelo de ruido en el mínimo.
fn calibrate(segmenter: &mut Segmenter) {
    segmenter.feed(&quiet(), 0.0001);
}

/// Alimenta silencio hasta que el turno se cierre.
fn close_turn(segmenter: &mut Segmenter) -> Option<Vec<u8>> {
    for _ in 0..120 {
        if let (Some(pcm), _) = segmenter.feed(&quiet(), 0.0) {
            return Some(pcm);
        }
    }
    None
}

#[test]
fn silence_never_starts_a_turn() {
    let mut segmenter = Segmenter::default();
    calibrate(&mut segmenter);
    for _ in 0..100 {
        let (pcm, started) = segmenter.feed(&quiet(), 0.0);
        assert!(pcm.is_none());
        assert!(!started);
    }
    assert!(segmenter.last_speech_end.is_none());
}

#[test]
fn speech_then_silence_closes_with_silence_reason() {
    let mut segmenter = Segmenter::default();
    calibrate(&mut segmenter);
    for _ in 0..2 {
        let (_, started) = segmenter.feed(&loud(), 0.2);
        assert!(!started);
    }
    let (_, started) = segmenter.feed(&loud(), 0.2);
    assert!(started);
    for _ in 0..10 {
        assert!(segmenter.feed(&loud(), 0.2).0.is_none());
    }
    let mut utterance = None;
    for _ in 0..60 {
        if let (Some(pcm), _) = segmenter.feed(&quiet(), 0.0) {
            utterance = Some(pcm);
            break;
        }
    }
    let pcm = utterance.expect("el turno debe cerrarse por silencio");
    // 20 chunks de pre-roll + 1 de arranque + 10 de habla + 44 de silencio.
    assert!(pcm.len() > 40 * 2);
    assert_eq!(segmenter.last_close_reason, "silence");
    assert!(segmenter.last_speech_end.is_some());
}

#[test]
fn click_is_discarded_with_empty_pcm() {
    let config = SegmenterConfig {
        silence_secs: 0.1,
        ..SegmenterConfig::default()
    };
    let mut segmenter = Segmenter::new(config);
    calibrate(&mut segmenter);
    for _ in 0..3 {
        segmenter.feed(&loud(), 0.2); // el tercero arranca el turno
    }
    let mut discarded = None;
    for _ in 0..10 {
        if let (Some(pcm), _) = segmenter.feed(&quiet(), 0.0) {
            discarded = Some(pcm);
            break;
        }
    }
    assert_eq!(discarded, Some(Vec::new()));
}

#[test]
fn flush_returns_the_turn_in_progress() {
    let mut segmenter = Segmenter::default();
    calibrate(&mut segmenter);
    for _ in 0..10 {
        segmenter.feed(&loud(), 0.2);
    }
    let pcm = segmenter.flush().expect("hay turno en curso");
    assert!(!pcm.is_empty());
    assert!(segmenter.flush().is_none());
}

#[test]
fn two_turns_separated_by_silence_are_two_utterances() {
    let mut segmenter = Segmenter::default();
    calibrate(&mut segmenter);
    for _ in 0..5 {
        segmenter.feed(&loud(), 0.2);
    }
    let first = close_turn(&mut segmenter).expect("primer turno");
    for _ in 0..5 {
        segmenter.feed(&loud(), 0.2);
    }
    let second = close_turn(&mut segmenter).expect("segundo turno");
    assert!(!first.is_empty());
    assert!(!second.is_empty());
}

#[test]
fn a_short_pause_does_not_split_the_phrase() {
    let mut segmenter = Segmenter::default();
    calibrate(&mut segmenter);
    for _ in 0..5 {
        segmenter.feed(&loud(), 0.2);
    }
    for _ in 0..5 {
        let (pcm, _) = segmenter.feed(&quiet(), 0.0);
        assert!(pcm.is_none());
    }
    for _ in 0..5 {
        segmenter.feed(&loud(), 0.2);
    }
    let pcm = close_turn(&mut segmenter).expect("un solo turno");
    assert!(pcm.len() > 10 * 2);
    assert_eq!(segmenter.last_close_reason, "silence");
}

#[test]
fn stable_noise_raises_the_floor() {
    let mut segmenter = Segmenter::default();
    for _ in 0..40 {
        segmenter.feed(&quiet(), 0.001);
    }
    let (start, _) = segmenter.thresholds();
    assert!(start > 0.0012, "suelo {start}");
    for _ in 0..5 {
        assert!(!segmenter.feed(&quiet(), 0.001).1);
    }
}

#[test]
fn short_voiced_runs_are_discarded() {
    let config = SegmenterConfig {
        min_speech_secs: 0.0,
        silence_secs: 0.1,
        ..SegmenterConfig::default()
    };
    let mut segmenter = Segmenter::new(config);
    calibrate(&mut segmenter);
    for _ in 0..3 {
        segmenter.feed(&loud(), 0.2); // 90 ms de voz: < min_voiced_secs
    }
    assert_eq!(close_turn(&mut segmenter), Some(Vec::new()));
}

#[test]
fn pending_probe_does_not_close_and_resuming_voice_forgets_it() {
    let config = SegmenterConfig {
        end_probe: Some(Box::new(|_pcm| Some(Probe::new()))),
        ..SegmenterConfig::default()
    };
    let mut segmenter = Segmenter::new(config);
    calibrate(&mut segmenter);
    for _ in 0..5 {
        segmenter.feed(&loud(), 0.2);
    }
    // La pausa supera quick_silence_secs: la sonda queda pendiente y el turno
    // no se cierra.
    for _ in 0..20 {
        assert!(segmenter.feed(&quiet(), 0.0).0.is_none());
    }
    for _ in 0..5 {
        segmenter.feed(&loud(), 0.2);
    }
    let pcm = close_turn(&mut segmenter).expect("turno");
    assert!(!pcm.is_empty());
    assert_eq!(segmenter.last_close_reason, "silence");
}

#[test]
fn thresholds_follow_noise_and_min_rms() {
    let mut segmenter = Segmenter {
        noise: 0.01,
        ..Segmenter::default()
    };
    let (start, end) = segmenter.thresholds();
    assert!((start - 0.013).abs() < 1e-9);
    assert!((end - 0.011).abs() < 1e-9);
    segmenter.noise = 0.0;
    let (start, end) = segmenter.thresholds();
    assert!((start - 0.0006).abs() < 1e-12);
    assert!((end - 0.00036).abs() < 1e-12);
}

#[test]
fn pre_roll_keeps_only_the_last_chunks() {
    let mut segmenter = Segmenter::default();
    for index in 0..20_u8 {
        segmenter.feed(&[index], 0.0);
    }
    segmenter.set_pre_roll(0.06); // 2 chunks
    assert_eq!(segmenter.pre.len(), 2);
    assert_eq!(segmenter.pre[0], vec![18]);
    assert_eq!(segmenter.pre[1], vec![19]);
    segmenter.set_pre_roll(f64::NAN);
    assert_eq!(segmenter.pre_roll_secs, 0.0);
}

#[test]
fn percentile_matches_numpy_linear_interpolation() {
    let values = [4.0, 1.0, 3.0, 2.0];
    assert!((percentile_linear(&values, 20.0) - 1.6).abs() < 1e-12);
    assert!((percentile_linear(&[7.0], 20.0) - 7.0).abs() < 1e-12);
    assert_eq!(percentile_linear(&[], 20.0), 0.0);
}

#[test]
fn finished_probe_closes_with_turn_reason() {
    let config = SegmenterConfig {
        end_probe: Some(Box::new(|_pcm| {
            let probe = Probe::new();
            probe.finish(true);
            Some(probe)
        })),
        ..SegmenterConfig::default()
    };
    let mut segmenter = Segmenter::new(config);
    calibrate(&mut segmenter);
    for _ in 0..10 {
        segmenter.feed(&loud(), 0.2);
    }
    segmenter.feed(&loud(), 0.2);
    let mut closed = None;
    for _ in 0..80 {
        if let (Some(pcm), _) = segmenter.feed(&quiet(), 0.0) {
            closed = Some(pcm);
            break;
        }
    }
    assert!(closed.is_some_and(|pcm| !pcm.is_empty()));
    assert_eq!(segmenter.last_close_reason, "turn");
}

#[test]
fn failed_probe_falls_back_to_long_silence() {
    let config = SegmenterConfig {
        end_probe: Some(Box::new(|_pcm| {
            let probe = Probe::new();
            probe.fail("sin modelo");
            Some(probe)
        })),
        ..SegmenterConfig::default()
    };
    let mut segmenter = Segmenter::new(config);
    calibrate(&mut segmenter);
    for _ in 0..10 {
        segmenter.feed(&loud(), 0.2);
    }
    segmenter.feed(&loud(), 0.2);
    let mut closed = None;
    for _ in 0..80 {
        if let (Some(pcm), _) = segmenter.feed(&quiet(), 0.0) {
            closed = Some(pcm);
            break;
        }
    }
    assert!(closed.is_some());
    assert_eq!(segmenter.last_close_reason, "silence");
}

#[test]
fn max_utterance_closes_with_max_reason() {
    let config = SegmenterConfig {
        max_utterance_secs: 0.3,
        silence_secs: 10.0,
        ..SegmenterConfig::default()
    };
    let mut segmenter = Segmenter::new(config);
    calibrate(&mut segmenter);
    for _ in 0..10 {
        segmenter.feed(&loud(), 0.2);
    }
    segmenter.feed(&loud(), 0.2);
    let mut closed = None;
    for _ in 0..10 {
        if let (Some(pcm), _) = segmenter.feed(&loud(), 0.2) {
            closed = Some(pcm);
            break;
        }
    }
    assert!(closed.is_some_and(|pcm| !pcm.is_empty()));
    assert_eq!(segmenter.last_close_reason, "max");
}

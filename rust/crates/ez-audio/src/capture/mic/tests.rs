
use super::*;

fn chunk(value: i16) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

fn pipeline() -> CapturePipeline {
    let settings = VadSettings::default();
    let mut pipeline = CapturePipeline::new(&settings, 16_000, None);
    pipeline.feed(&settings, &chunk(0), 0.0001, false, false); // calibra
    pipeline
}

#[test]
fn speech_then_silence_emits_a_start_and_an_utterance() {
    let settings = VadSettings::default();
    let mut pipeline = pipeline();
    let mut events = Vec::new();
    for _ in 0..5 {
        events.extend(pipeline.feed(&settings, &chunk(8000), 0.2, false, false));
    }
    assert!(events.contains(&CaptureEvent::SpeechStart(0.2)));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, CaptureEvent::SpeechStart(_)))
            .count(),
        1
    );
    for _ in 0..200 {
        events.extend(pipeline.feed(&settings, &chunk(0), 0.0, false, false));
    }
    let utterance = events
        .iter()
        .find_map(|event| match event {
            CaptureEvent::Utterance(pcm) if !pcm.is_empty() => Some(pcm.clone()),
            _ => None,
        })
        .expect("hay utterance");
    assert!(!utterance.is_empty());
    assert!(pipeline.last_speech_end.is_some());
}

#[test]
fn reset_forgets_the_turn_in_progress() {
    let settings = VadSettings::default();
    let mut pipeline = pipeline();
    for _ in 0..5 {
        pipeline.feed(&settings, &chunk(8000), 0.2, false, false);
    }
    let events = pipeline.feed(&settings, &chunk(8000), 0.2, false, true);
    assert!(events.is_empty());
    assert!(pipeline.segmenter.flush().is_none());
}

#[test]
fn barge_in_needs_sustained_voice_and_replays_the_run() {
    let settings = VadSettings {
        barge_in_rms: 0.008,
        barge_in_secs: 0.09, // 3 chunks
        ..VadSettings::default()
    };
    let mut pipeline = CapturePipeline::new(&settings, 16_000, None);
    pipeline.feed(&settings, &chunk(0), 0.0001, false, false); // calibra
    assert!(pipeline
        .feed(&settings, &chunk(8000), 0.2, true, false)
        .is_empty());
    assert!(pipeline
        .feed(&settings, &chunk(8000), 0.2, true, false)
        .is_empty());
    let third = pipeline.feed(&settings, &chunk(8000), 0.2, true, false);
    assert_eq!(third.first(), Some(&CaptureEvent::BargeIn(0.2)));
    assert!(third
        .iter()
        .any(|event| matches!(event, CaptureEvent::SpeechStart(_))));
}

#[test]
fn no_barge_in_while_the_assistant_is_silent() {
    let settings = VadSettings {
        barge_in_rms: 0.008,
        barge_in_secs: 0.06,
        ..VadSettings::default()
    };
    let mut pipeline = CapturePipeline::new(&settings, 16_000, None);
    for _ in 0..5 {
        let events = pipeline.feed(&settings, &chunk(8000), 0.2, false, false);
        assert!(events
            .iter()
            .all(|event| !matches!(event, CaptureEvent::BargeIn(_))));
    }
}

#[test]
fn live_settings_reach_the_segmenter() {
    let settings = VadSettings {
        min_rms: 0.05,
        silence_secs: 9.0,
        ..VadSettings::default()
    };
    let mut pipeline = CapturePipeline::new(&VadSettings::default(), 16_000, None);
    pipeline.feed(&settings, &chunk(0), 0.0, false, false);
    assert_eq!(pipeline.segmenter.min_rms, 0.05);
    assert_eq!(pipeline.segmenter.silence_secs, 9.0);
}

#[test]
fn silent_capture_stays_in_stream_and_applies_live_changes() {
    let mic = MicCapture::silent(VadSettings::default());
    let stop = Arc::new(AtomicBool::new(false));
    let worker = {
        let mic = mic.clone();
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || mic.stream(&stop, StreamHooks::default(), None, None, 0.0))
    };
    while !mic.capturing() {
        std::thread::sleep(Duration::from_millis(5));
    }
    // El menú cambia el VAD en vivo: no espera a que termine la llamada.
    let started = std::time::Instant::now();
    mic.set_min_rms(0.05);
    mic.set_barge_in_rms(0.008);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(mic.min_rms(), 0.05);
    // El bloque siguiente de `stream` lee esa instantánea.
    let live = mic.live_settings(0.0);
    assert_eq!(live.min_rms, 0.05);
    assert_eq!(live.barge_in_rms, 0.008);
    stop.store(true, Ordering::SeqCst);
    worker
        .join()
        .expect("hilo de captura")
        .expect("captura sin error");
    assert!(!mic.capturing());
}

#[test]
fn the_turn_detector_is_read_on_every_probe() {
    let mic = MicCapture::silent(VadSettings::default());
    // Siempre hay sonda, como `Segmenter(end_probe=self._end_probe)`.
    assert!(mic.end_probe().is_some());
    assert!(mic.turn_detector().is_none());
}

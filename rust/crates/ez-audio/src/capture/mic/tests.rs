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

#[test]
fn the_playback_reference_and_the_mode_travel_with_the_mic() {
    let mic = MicCapture::silent(VadSettings::default());
    assert_eq!(mic.barge_in_mode(), BargeInMode::Fixed);
    let reference = Arc::new(PlaybackReference::new());
    mic.set_playback_reference(Some(Arc::clone(&reference)));
    mic.set_barge_in_mode(BargeInMode::Adaptive);
    assert_eq!(mic.live_settings(0.0).barge_in_mode, BargeInMode::Adaptive);
    let seen = mic.inner.live.playback_reference().expect("hay referencia");
    assert!(Arc::ptr_eq(&seen, &reference));
}

/// Voz sintética: un tono plano del nivel pedido, 480 muestras por bloque.
fn tone(level: f64) -> Vec<u8> {
    let sample = (level.clamp(0.0, 1.0) * 32767.0) as i16;
    sample.to_le_bytes().repeat(480)
}

fn adaptive_settings() -> VadSettings {
    VadSettings {
        silence_secs: 0.2,
        min_speech_secs: 0.1,
        barge_in_rms: 0.008,
        barge_in_secs: 0.25,
        barge_in_mode: BargeInMode::Adaptive,
        ..VadSettings::default()
    }
}

fn adaptive_pipeline(
    settings: &VadSettings,
    reference: &Arc<PlaybackReference>,
) -> CapturePipeline {
    CapturePipeline::with_reference(settings, 16_000, None, Some(Arc::clone(reference)))
}

fn fired(events: &[CaptureEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, CaptureEvent::BargeIn(_)))
}

#[test]
fn adaptive_ignores_the_echo_of_the_assistant() {
    let reference = Arc::new(PlaybackReference::new());
    let settings = adaptive_settings();
    let mut pipeline = adaptive_pipeline(&settings, &reference);
    reference.begin();
    let mut barged = false;
    for block in 0..60 {
        let reference_level = 0.004 + 0.002 * f64::from(block % 10) / 10.0;
        reference.publish(reference_level);
        let level = reference_level * 0.8;
        barged |= fired(&pipeline.feed(&settings, &tone(level), level, true, false));
    }
    reference.end();
    assert!(!barged, "solo eco (acople 0,8) no interrumpe");
}

#[test]
fn adaptive_hears_a_voice_below_the_fixed_threshold() {
    let reference = Arc::new(PlaybackReference::new());
    let settings = adaptive_settings();
    let mut pipeline = adaptive_pipeline(&settings, &reference);
    reference.begin();
    let mut barge_at = None;
    for block in 0..60 {
        reference.publish(0.004);
        // Eco de 0,002 más una voz moderada: 0,007 en total, por debajo de 0,008.
        let voice = if block >= 10 { 0.005 } else { 0.0 };
        let level = 0.002 + voice;
        if fired(&pipeline.feed(&settings, &tone(level), level, true, false)) && barge_at.is_none()
        {
            barge_at = Some(block);
        }
    }
    reference.end();
    let block = barge_at.expect("la voz normal interrumpe");
    assert!(
        block <= 21,
        "tras 0,3 s de voz ya debe cortar (bloque {block})"
    );
}

#[test]
fn fixed_mode_keeps_the_python_threshold() {
    let reference = Arc::new(PlaybackReference::new());
    let settings = VadSettings {
        barge_in_mode: BargeInMode::Fixed,
        ..adaptive_settings()
    };
    let mut pipeline = adaptive_pipeline(&settings, &reference);
    reference.begin();
    let mut barged = false;
    for block in 0..60 {
        reference.publish(0.004);
        let voice = if block >= 10 { 0.005 } else { 0.0 };
        let level = 0.002 + voice;
        barged |= fired(&pipeline.feed(&settings, &tone(level), level, true, false));
    }
    reference.end();
    assert!(!barged, "con umbral fijo 0,007 no alcanza");
}

#[test]
fn short_clicks_and_steady_noise_do_not_interrupt() {
    let reference = Arc::new(PlaybackReference::new());
    let settings = adaptive_settings();
    let mut pipeline = adaptive_pipeline(&settings, &reference);
    reference.begin();
    let mut barged = false;
    // Dos golpes cortos por encima de cualquier umbral.
    for _ in 0..2 {
        barged |= fired(&pipeline.feed(&settings, &tone(0.2), 0.2, true, false));
    }
    // Ruido constante que sube el piso: el umbral lo sigue, no hay corte.
    for _ in 0..40 {
        reference.publish(0.004);
        barged |= fired(&pipeline.feed(&settings, &tone(0.005), 0.005, true, false));
    }
    reference.end();
    assert!(!barged);
}

#[test]
fn after_a_barge_in_the_utterance_starts_at_its_first_word() {
    let reference = Arc::new(PlaybackReference::new());
    let settings = adaptive_settings();
    let mut pipeline = adaptive_pipeline(&settings, &reference);
    reference.begin();
    let mut fed: Vec<Vec<u8>> = Vec::new();
    // El eco no dispara y la voz empieza en el bloque 10.
    for block in 0..60 {
        reference.publish(0.004);
        let voice = if block >= 10 { 0.005 } else { 0.0 };
        let level = 0.002 + voice;
        let chunk = tone(level);
        fed.push(chunk.clone());
        let events = pipeline.feed(&settings, &chunk, level, true, false);
        if fired(&events) {
            break;
        }
    }
    let run = fed[fed.len() - 9..].to_vec();
    assert_eq!(run.len(), 9, "la racha del barge-in son 9 bloques");
    // El TTS se cortó: la app vuelve a escuchar y la frase sigue.
    reference.end();
    let mut utterance = None;
    for block in 0..20 {
        let level = if block < 6 { 0.006 } else { 0.0 };
        let chunk = tone(level);
        let events = pipeline.feed(&settings, &chunk, level, false, false);
        if let Some(pcm) = events.iter().find_map(|event| match event {
            CaptureEvent::Utterance(pcm) if !pcm.is_empty() => Some(pcm.clone()),
            _ => None,
        }) {
            utterance = Some(pcm);
            break;
        }
    }
    let utterance = utterance.expect("hay frase");
    assert!(
        utterance.starts_with(&run[0]),
        "la racha reinyectada abre la frase, sin perder el principio"
    );
}

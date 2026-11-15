//! Casos del `Segmenter` (submódulo de `audio_parity`).

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ez_audio::capture::pcm_rms;
use ez_audio::segmenter::{EndProbe, Probe, Segmenter, SegmenterConfig, SegmenterState};
use ez_parity::fixtures::FixtureCase;

use crate::{close, hex, read_wav, sha256, ROOT};

// -------------------------------------------------------------- segmentador

/// Lo que el `end_probe` comparte con el bucle: chunk actual y llamadas.
pub(crate) struct ProbeHandle {
    chunk: Arc<AtomicUsize>,
    calls: Arc<Mutex<Vec<ProbeCall>>>,
}

pub(crate) struct ProbeCall {
    chunk: usize,
    tail_bytes: usize,
    tail_chunks: usize,
    tail_sha256: String,
    silence_secs: f64,
}

pub(crate) struct ProbeScript {
    chunk: Arc<AtomicUsize>,
    calls: Arc<Mutex<Vec<ProbeCall>>>,
    remaining_false: Arc<Mutex<usize>>,
}

pub(crate) fn end_probe(spec: &FixtureCase, chunk_bytes: usize) -> (EndProbe, ProbeHandle) {
    let mode = spec.input["probe"]["mode"]
        .as_str()
        .expect("modo")
        .to_string();
    let checks = spec.input["probe"]["checks"].as_u64().unwrap_or(3) as usize;
    let false_count = spec.input["probe"]["false_count"].as_u64().unwrap_or(1) as usize;
    let script = Arc::new(ProbeScript {
        chunk: Arc::new(AtomicUsize::new(0)),
        calls: Arc::new(Mutex::new(Vec::new())),
        remaining_false: Arc::new(Mutex::new(false_count)),
    });
    let recorder = Arc::clone(&script);
    let probe: EndProbe = Box::new(move |pcm: &[u8], state: &SegmenterState| {
        recorder.calls.lock().expect("calls").push(ProbeCall {
            chunk: recorder.chunk.load(Ordering::SeqCst),
            tail_bytes: pcm.len(),
            tail_chunks: pcm.len() / chunk_bytes.max(1),
            tail_sha256: hex(&sha256(pcm)),
            silence_secs: state.silence_secs,
        });
        match mode.as_str() {
            "none" | "raise" => None,
            "true" => Some(Probe::scripted(0, true)),
            // `result()` lanza: la sonda queda completada pero sin veredicto.
            "raise_result" => {
                let probe = Probe::new();
                probe.fail("sonda rota");
                Some(probe)
            }
            "pending_then_true" => Some(Probe::scripted(checks, true)),
            "false_then_true" => {
                let mut remaining = recorder.remaining_false.lock().expect("remaining");
                if *remaining > 0 {
                    *remaining -= 1;
                    Some(Probe::scripted(0, false))
                } else {
                    Some(Probe::scripted(0, true))
                }
            }
            other => panic!("sonda desconocida: {other}"),
        }
    });
    let handle = ProbeHandle {
        chunk: Arc::clone(&script.chunk),
        calls: Arc::clone(&script.calls),
    };
    (probe, handle)
}

#[derive(Clone, Debug)]
pub(crate) struct PcmInfo {
    bytes: usize,
    sha256: String,
    head_hex: String,
    tail_hex: String,
}

pub(crate) fn pcm_info(pcm: &[u8]) -> PcmInfo {
    PcmInfo {
        bytes: pcm.len(),
        sha256: hex(&sha256(pcm)),
        head_hex: hex(&pcm[..pcm.len().min(16)]),
        tail_hex: hex(&pcm[pcm.len().saturating_sub(16)..]),
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Event {
    SpeechStart {
        chunk: usize,
        level: f64,
        noise: f64,
        start_threshold: f64,
        end_threshold: f64,
        pre_chunks: usize,
        pre_capacity: usize,
    },
    Utterance {
        chunk: usize,
        discarded: bool,
        pcm: PcmInfo,
        close_reason: String,
        speech_end_chunk: i64,
    },
    Reset {
        chunk: usize,
        was_in_speech: bool,
        noise: f64,
    },
}

pub(crate) fn segmenter_config(input: &FixtureCase) -> SegmenterConfig {
    let mut config = SegmenterConfig::default();
    let Some(values) = input
        .input
        .get("config")
        .and_then(|value| value.as_object())
    else {
        return config;
    };
    for (key, value) in values {
        let number = value.as_f64().unwrap_or(0.0);
        match key.as_str() {
            "min_rms" => config.min_rms = number,
            "silence_secs" => config.silence_secs = number,
            "pre_roll_secs" => config.pre_roll_secs = number,
            "min_speech_secs" => config.min_speech_secs = number,
            "max_utterance_secs" => config.max_utterance_secs = number,
            "noise_multiplier" => config.noise_multiplier = number,
            "quick_silence_secs" => config.quick_silence_secs = number,
            "min_voiced_secs" => config.min_voiced_secs = number,
            "probe_interval_secs" => config.probe_interval_secs = number,
            other => panic!("config desconocida: {other}"),
        }
    }
    config
}

pub(crate) fn chunk_list(case: &FixtureCase) -> (Vec<Vec<u8>>, usize) {
    let input = &case.input;
    if let Some(relative) = input.get("wav").and_then(|value| value.as_str()) {
        let path = Path::new(ROOT).join(relative);
        let bytes = std::fs::read(&path).expect("WAV del segmentador");
        assert_eq!(
            hex(&sha256(&bytes)),
            input["wav_sha256"].as_str().expect("wav_sha256"),
            "sha256 de {relative}"
        );
        let (pcm, _, _) = read_wav(&path).expect("WAV legible");
        let chunk_bytes = input["chunk_bytes"].as_u64().unwrap_or(960) as usize;
        let usable = pcm.len() - pcm.len() % chunk_bytes;
        let chunks = pcm[..usable]
            .chunks(chunk_bytes)
            .map(<[u8]>::to_vec)
            .collect();
        return (chunks, pcm.len() - usable);
    }
    let chunk_bytes = input["chunk_pattern"]["bytes"].as_u64().unwrap_or(64) as usize;
    let seed = input["chunk_pattern"]["seed"].as_u64().unwrap_or(0) as usize;
    let levels = input["levels"].as_array().expect("levels");
    let chunks = (0..levels.len())
        .map(|index| {
            (0..chunk_bytes)
                .map(|offset| ((seed + index * chunk_bytes + offset) % 256) as u8)
                .collect()
        })
        .collect();
    (chunks, 0)
}

pub(crate) fn speech_end_chunk(value: Option<f64>, origin: f64) -> i64 {
    let Some(value) = value else {
        return -1;
    };
    ((value - origin) / ez_audio::CHUNK_SECS).round_ties_even() as i64 - 1
}

pub(crate) fn compare_segmenter(case: &FixtureCase) {
    let (chunks, ignored_tail) = chunk_list(case);
    let levels: Vec<f64> = match case.input.get("levels").and_then(|value| value.as_array()) {
        Some(levels) => levels
            .iter()
            .map(|value| value.as_f64().unwrap_or(0.0))
            .collect(),
        None => chunks.iter().map(|chunk| pcm_rms(chunk)).collect(),
    };
    let clock = Arc::new(Mutex::new(1000.0_f64));
    let origin = *clock.lock().expect("clock");
    let mut config = segmenter_config(case);
    config.clock = Some({
        let clock = Arc::clone(&clock);
        Arc::new(move || *clock.lock().expect("clock"))
    });
    let handle = if case.input["probe"].is_null() {
        None
    } else {
        let chunk_bytes = case
            .input
            .get("chunk_bytes")
            .or_else(|| {
                case.input
                    .get("chunk_pattern")
                    .and_then(|value| value.get("bytes"))
            })
            .and_then(|value| value.as_u64())
            .unwrap_or(64) as usize;
        let (probe, handle) = end_probe(case, chunk_bytes);
        config.end_probe = Some(probe);
        Some(handle)
    };
    let mut segmenter = Segmenter::new(config);
    let mut events = Vec::new();
    let mut updates = Vec::new();
    let mut flushes = Vec::new();
    let flush_at = numbers(case, "flush_at");
    let reset_at = numbers(case, "reset_at");
    for (index, (chunk, level)) in chunks.iter().zip(&levels).enumerate() {
        if let Some(values) = case
            .input
            .get("pre_roll_at")
            .and_then(|value| value.as_object())
        {
            if let Some(value) = values.get(&index.to_string()) {
                if let Some(seconds) = value.as_f64() {
                    segmenter.set_pre_roll(seconds);
                }
                let state = segmenter.state();
                updates.push((index, state.pre_roll_secs, state.pre_capacity));
            }
        }
        if flush_at.contains(&index) {
            flushes.push(segmenter.flush().map(|pcm| pcm_info(&pcm)));
        }
        if reset_at.contains(&index) {
            let was_in_speech = segmenter.state().in_speech;
            segmenter.reset();
            events.push(Event::Reset {
                chunk: index,
                was_in_speech,
                noise: segmenter.state().noise,
            });
        }
        if let Some(handle) = &handle {
            handle.chunk.store(index, Ordering::SeqCst);
        }
        *clock.lock().expect("clock") = origin + (index + 1) as f64 * ez_audio::CHUNK_SECS;
        let before = segmenter.state();
        let (pcm, started) = segmenter.feed(chunk, *level);
        if started {
            let state = segmenter.state();
            events.push(Event::SpeechStart {
                chunk: index,
                level: *level,
                noise: state.noise,
                start_threshold: state.start_threshold,
                end_threshold: state.end_threshold,
                pre_chunks: before.pre_chunks,
                pre_capacity: before.pre_capacity,
            });
        }
        if let Some(pcm) = pcm {
            let state = segmenter.state();
            events.push(Event::Utterance {
                chunk: index,
                discarded: pcm.is_empty(),
                pcm: pcm_info(&pcm),
                close_reason: state.last_close_reason,
                speech_end_chunk: speech_end_chunk(state.last_speech_end, origin),
            });
        }
    }
    if case.input["flush_end"].as_bool().unwrap_or(false) {
        flushes.push(segmenter.flush().map(|pcm| pcm_info(&pcm)));
    }
    compare_events(case, &events);
    compare_segmenter_tail(
        case,
        &segmenter,
        &updates,
        &flushes,
        handle.as_ref(),
        ignored_tail,
    );
}

pub(crate) fn numbers(case: &FixtureCase, key: &str) -> Vec<usize> {
    case.input
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_u64().map(|value| value as usize))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn compare_segmenter_tail(
    case: &FixtureCase,
    segmenter: &Segmenter,
    updates: &[(usize, f64, usize)],
    flushes: &[Option<PcmInfo>],
    handle: Option<&ProbeHandle>,
    ignored_tail: usize,
) {
    let expected_updates = case.output["pre_roll_updates"].as_array().expect("updates");
    assert_eq!(
        updates.len(),
        expected_updates.len(),
        "updates de {}",
        case.case
    );
    for (index, (chunk, secs, capacity)) in updates.iter().enumerate() {
        assert_eq!(
            expected_updates[index]["chunk"].as_u64().unwrap_or(0) as usize,
            *chunk
        );
        close(
            *secs,
            expected_updates[index]["pre_roll_secs"]
                .as_f64()
                .unwrap_or(0.0),
            1e-12,
            "pre_roll_secs",
            &case.case,
        );
        assert_eq!(
            expected_updates[index]["capacity"].as_u64().unwrap_or(0) as usize,
            *capacity
        );
    }
    let expected_flushes = case.output["flushes"].as_array().expect("flushes");
    assert_eq!(
        flushes.len(),
        expected_flushes.len(),
        "flushes de {}",
        case.case
    );
    for (index, pcm) in flushes.iter().enumerate() {
        let expected = &expected_flushes[index]["pcm"];
        match pcm {
            None => assert!(expected.is_null(), "flush {} de {}", index, case.case),
            Some(info) => compare_pcm_info(
                info,
                expected["bytes"].as_u64().unwrap_or(u64::MAX),
                expected["sha256"].as_str().unwrap_or(""),
                expected["head_hex"].as_str().unwrap_or(""),
                expected["tail_hex"].as_str().unwrap_or(""),
                &case.case,
            ),
        }
    }
    let state = segmenter.state();
    let expected = &case.output["final"];
    assert_eq!(
        state.in_speech,
        expected["in_speech"].as_bool().unwrap_or(false)
    );
    close(
        state.noise,
        expected["noise"].as_f64().unwrap_or(0.0),
        1e-5,
        "noise",
        &case.case,
    );
    close(
        state.pre_roll_secs,
        expected["pre_roll_secs"].as_f64().unwrap_or(0.0),
        1e-12,
        "pre_roll_secs",
        &case.case,
    );
    assert_eq!(
        state.last_close_reason,
        expected["last_close_reason"].as_str().unwrap_or("")
    );
    assert_eq!(
        state.last_speech_end.is_some(),
        expected["last_speech_end_set"].as_bool().unwrap_or(false)
    );
    close(
        state.start_threshold,
        expected["start_threshold"].as_f64().unwrap_or(0.0),
        1e-5,
        "start_threshold",
        &case.case,
    );
    close(
        state.end_threshold,
        expected["end_threshold"].as_f64().unwrap_or(0.0),
        1e-5,
        "end_threshold",
        &case.case,
    );
    close(
        state.speech_secs,
        expected["speech_secs"].as_f64().unwrap_or(0.0),
        1e-5,
        "speech_secs",
        &case.case,
    );
    close(
        state.silence_secs,
        expected["silence_secs"].as_f64().unwrap_or(0.0),
        1e-5,
        "silence_secs",
        &case.case,
    );
    assert_eq!(
        state.speech_run as u64,
        expected["speech_run"].as_u64().unwrap_or(0)
    );
    if case.input.get("wav").is_some() {
        assert_eq!(
            ignored_tail as u64,
            case.output["ignored_tail_bytes"].as_u64().unwrap_or(0)
        );
    }
    if let Some(handle) = handle {
        compare_probe_calls(case, &handle.calls.lock().expect("calls"));
    }
}

pub(crate) fn compare_probe_calls(case: &FixtureCase, calls: &[ProbeCall]) {
    let expected = case.output["probe_calls"].as_array().expect("probe_calls");
    assert_eq!(calls.len(), expected.len(), "probe_calls de {}", case.case);
    for (index, call) in calls.iter().enumerate() {
        let expected = &expected[index];
        assert_eq!(call.chunk as u64, expected["chunk"].as_u64().unwrap_or(0));
        assert_eq!(
            call.tail_bytes as u64,
            expected["tail_bytes"].as_u64().unwrap_or(0)
        );
        assert_eq!(
            call.tail_chunks as u64,
            expected["tail_chunks"].as_u64().unwrap_or(0)
        );
        assert_eq!(
            call.tail_sha256,
            expected["tail_sha256"].as_str().unwrap_or("")
        );
        close(
            call.silence_secs,
            expected["silence_secs"].as_f64().unwrap_or(0.0),
            1e-9,
            "silence_secs",
            &case.case,
        );
    }
}

pub(crate) fn compare_events(case: &FixtureCase, events: &[Event]) {
    let expected = case.output["events"].as_array().expect("events");
    assert_eq!(events.len(), expected.len(), "events de {}", case.case);
    for (index, event) in events.iter().enumerate() {
        let expected = &expected[index];
        let kind = expected["event"].as_str().unwrap_or("");
        assert_eq!(
            expected["chunk"].as_u64().unwrap_or(0) as usize,
            event_chunk(event),
            "chunk del evento {index} de {}",
            case.case
        );
        match event {
            Event::SpeechStart {
                level,
                noise,
                start_threshold,
                end_threshold,
                pre_chunks,
                pre_capacity,
                ..
            } => {
                assert_eq!(kind, "speech_start", "{}[{index}]", case.case);
                close(
                    *level,
                    expected["level"].as_f64().unwrap_or(0.0),
                    1e-5,
                    "level",
                    &case.case,
                );
                close(
                    *noise,
                    expected["noise"].as_f64().unwrap_or(0.0),
                    1e-5,
                    "noise",
                    &case.case,
                );
                close(
                    *start_threshold,
                    expected["start_threshold"].as_f64().unwrap_or(0.0),
                    1e-5,
                    "start_threshold",
                    &case.case,
                );
                close(
                    *end_threshold,
                    expected["end_threshold"].as_f64().unwrap_or(0.0),
                    1e-5,
                    "end_threshold",
                    &case.case,
                );
                assert_eq!(
                    *pre_chunks as u64,
                    expected["pre_chunks"].as_u64().unwrap_or(0)
                );
                assert_eq!(
                    *pre_capacity as u64,
                    expected["pre_capacity"].as_u64().unwrap_or(0)
                );
            }
            Event::Utterance {
                discarded,
                pcm,
                close_reason,
                speech_end_chunk,
                ..
            } => {
                assert_eq!(kind, "utterance", "{}[{index}]", case.case);
                assert_eq!(*discarded, expected["discarded"].as_bool().unwrap_or(false));
                let pcm_expected = &expected["pcm"];
                compare_pcm_info(
                    pcm,
                    pcm_expected["bytes"].as_u64().unwrap_or(u64::MAX),
                    pcm_expected["sha256"].as_str().unwrap_or(""),
                    pcm_expected["head_hex"].as_str().unwrap_or(""),
                    pcm_expected["tail_hex"].as_str().unwrap_or(""),
                    &case.case,
                );
                assert_eq!(
                    close_reason,
                    expected["close_reason"].as_str().unwrap_or("")
                );
                assert_eq!(
                    *speech_end_chunk,
                    expected["speech_end_chunk"].as_i64().unwrap_or(-1)
                );
            }
            Event::Reset {
                was_in_speech,
                noise,
                ..
            } => {
                assert_eq!(kind, "reset", "{}[{index}]", case.case);
                assert_eq!(
                    *was_in_speech,
                    expected["was_in_speech"].as_bool().unwrap_or(false)
                );
                close(
                    *noise,
                    expected["noise"].as_f64().unwrap_or(0.0),
                    1e-5,
                    "noise",
                    &case.case,
                );
            }
        }
    }
}

pub(crate) fn event_chunk(event: &Event) -> usize {
    match event {
        Event::SpeechStart { chunk, .. }
        | Event::Utterance { chunk, .. }
        | Event::Reset { chunk, .. } => *chunk,
    }
}

pub(crate) fn compare_pcm_info(
    produced: &PcmInfo,
    bytes: u64,
    sha: &str,
    head: &str,
    tail: &str,
    case: &str,
) {
    assert_eq!(produced.bytes as u64, bytes, "bytes de pcm en {case}");
    assert_eq!(produced.sha256, sha, "sha256 de pcm en {case}");
    assert_eq!(produced.head_hex, head, "head_hex de pcm en {case}");
    assert_eq!(produced.tail_hex, tail, "tail_hex de pcm en {case}");
}

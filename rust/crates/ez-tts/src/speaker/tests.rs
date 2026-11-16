//! Unit tests for `speaker.rs` (F4-05).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ez_core::config::VoiceConfig;
use parking_lot::Mutex;

use super::*;
use crate::kokoro::session::{DType, Model};
use crate::kokoro::{KoKoroConfig, Tokenizer};
use crate::phonemes::{PhonemeError, Phonemizer};
use crate::voices::{Style, Voices};

struct RecordingPhonemizer {
    calls: Arc<Mutex<Vec<(String, String)>>>,
}

impl Phonemizer for RecordingPhonemizer {
    fn phonemize(&self, text: &str, lang: &str) -> Result<String, PhonemeError> {
        self.calls.lock().push((text.to_string(), lang.to_string()));
        Ok("ao".to_string())
    }
}

#[derive(Default)]
struct ModelTrace {
    speed: Option<f64>,
}

struct FakeModel {
    dtypes: HashMap<String, DType>,
    outputs: Vec<String>,
    trace: Arc<Mutex<ModelTrace>>,
}

impl FakeModel {
    fn new() -> Self {
        Self {
            dtypes: HashMap::from([
                ("tokens".to_string(), DType::Int64),
                ("style".to_string(), DType::Float32),
                ("speed".to_string(), DType::Float32),
            ]),
            outputs: vec!["audio".to_string()],
            trace: Arc::new(Mutex::new(ModelTrace::default())),
        }
    }
}

impl Model for FakeModel {
    fn input_dtypes(&self) -> &HashMap<String, DType> {
        &self.dtypes
    }

    fn tokens_input(&self) -> &str {
        "tokens"
    }

    fn output_names(&self) -> &[String] {
        &self.outputs
    }

    fn metadata(&self, _key: &str) -> Option<&str> {
        None
    }

    fn infer(
        &mut self,
        tokens: &[i64],
        _style: &[f32],
        speed: f64,
    ) -> Result<(Vec<f32>, Option<Vec<i64>>), String> {
        self.trace.lock().speed = Some(speed);
        Ok((vec![0.1; tokens.len() * 100], None))
    }
}

#[derive(Default)]
struct FakeTrace {
    rates: Vec<u32>,
    devices: Vec<Option<usize>>,
    chunks: Vec<Vec<i16>>,
    finishes: Vec<bool>,
}

struct FakeSink {
    trace: Arc<Mutex<FakeTrace>>,
}

struct FakeStream {
    trace: Arc<Mutex<FakeTrace>>,
}

impl AudioSink for FakeSink {
    fn open(&self, rate: u32, device_index: Option<usize>) -> Result<Box<dyn AudioStream>, String> {
        let mut trace = self.trace.lock();
        trace.rates.push(rate);
        trace.devices.push(device_index);
        drop(trace);
        Ok(Box::new(FakeStream {
            trace: Arc::clone(&self.trace),
        }))
    }
}

impl AudioStream for FakeStream {
    fn write(&mut self, samples: &[i16]) -> Result<(), String> {
        self.trace.lock().chunks.push(samples.to_vec());
        Ok(())
    }

    fn finish(
        &mut self,
        stop: &AtomicBool,
        _reference: Option<&PlaybackReference>,
    ) -> Result<bool, String> {
        let sounded = !stop.load(Ordering::Relaxed);
        self.trace.lock().finishes.push(sounded);
        Ok(sounded)
    }
}

fn fake_sink() -> (Arc<FakeSink>, Arc<Mutex<FakeTrace>>) {
    let trace = Arc::new(Mutex::new(FakeTrace::default()));
    let sink = Arc::new(FakeSink {
        trace: Arc::clone(&trace),
    });
    (sink, trace)
}

fn config(entries: &str) -> VoiceConfig {
    let value = ez_core::pyloads::loads(entries).expect("json válido");
    let object = value.as_object().expect("objeto");
    VoiceConfig::from_json(object)
}

fn speaker(
    cfg: VoiceConfig,
    calls: Arc<Mutex<Vec<(String, String)>>>,
) -> (Speaker, Arc<Mutex<FakeTrace>>, Arc<Mutex<ModelTrace>>) {
    let tokenizer = Tokenizer::with_phonemizer(Box::new(RecordingPhonemizer { calls }), None);
    let style = Style::new(5, 2, vec![0.1; 10]).expect("estilo");
    let voices = Voices::from_styles(HashMap::from([
        ("voz".to_string(), style.clone()),
        ("otra".to_string(), style),
    ]));
    let model = FakeModel::new();
    let model_trace = Arc::clone(&model.trace);
    let kokoro = Kokoro::with_tokenizer(
        Box::new(model),
        tokenizer,
        voices,
        KoKoroConfig::new("modelo.onnx", "voces.bin"),
    );
    let (sink, sink_trace) = fake_sink();
    let speaker = Speaker::assemble(cfg, kokoro, sink).expect("hablante");
    (speaker, sink_trace, model_trace)
}

#[test]
fn samples_are_clipped_and_truncated_like_numpy() {
    assert_eq!(
        to_pcm(&[-1.5, 1.5, -1.0, 1.0, 0.5]),
        vec![-32767, 32767, -32767, 32767, 16383]
    );
}

#[test]
fn play_chunks_480_samples_and_reports_each_level() {
    let (sink, trace) = fake_sink();
    let stop = AtomicBool::new(false);
    let mut starts = 0;
    let mut levels = Vec::new();
    let sounded = play_samples(
        sink.as_ref(),
        &[0.5; 1000],
        24_000,
        &stop,
        Some(2),
        Some(&mut |level| levels.push(level)),
        Some(&mut || starts += 1),
        None,
    )
    .expect("suena");
    assert!(sounded);
    let trace = trace.lock();
    assert_eq!(trace.rates, vec![24_000]);
    assert_eq!(trace.devices, vec![Some(2)]);
    let sizes: Vec<usize> = trace.chunks.iter().map(Vec::len).collect();
    assert_eq!(sizes, vec![480, 480, 40]);
    assert_eq!(trace.chunks[0][0], 16383);
    assert_eq!(starts, 1);
    assert_eq!(levels.len(), 3);
    assert!(levels.iter().all(|level| (level - 0.5).abs() < 1e-3));
    assert_eq!(trace.finishes, vec![true]);
}

#[test]
fn stop_before_the_first_chunk_writes_nothing() {
    let (sink, trace) = fake_sink();
    let stop = AtomicBool::new(true);
    let mut starts = 0;
    let sounded = play_samples(
        sink.as_ref(),
        &[0.5; 100],
        24_000,
        &stop,
        None,
        None,
        Some(&mut || starts += 1),
        None,
    )
    .expect("abre y corta");
    assert!(!sounded);
    let trace = trace.lock();
    assert_eq!(trace.rates, vec![24_000]);
    assert!(trace.chunks.is_empty());
    assert!(trace.finishes.is_empty());
    assert_eq!(starts, 0);
}

#[test]
fn stop_after_the_first_chunk_keeps_only_that_chunk() {
    let (sink, trace) = fake_sink();
    let stop = AtomicBool::new(false);
    let mut on_level = |_level: f64| stop.store(true, Ordering::Relaxed);
    let sounded = play_samples(
        sink.as_ref(),
        &[0.5; 1000],
        24_000,
        &stop,
        None,
        Some(&mut on_level),
        None,
        None,
    )
    .expect("corta");
    assert!(!sounded);
    assert_eq!(trace.lock().chunks.len(), 1);
}

#[test]
fn empty_audio_still_opens_and_finishes() {
    let (sink, trace) = fake_sink();
    let stop = AtomicBool::new(false);
    let mut starts = 0;
    let sounded = play_samples(
        sink.as_ref(),
        &[],
        24_000,
        &stop,
        None,
        None,
        Some(&mut || starts += 1),
        None,
    )
    .expect("nada");
    assert!(sounded);
    assert_eq!(starts, 0);
    let trace = trace.lock();
    assert!(trace.chunks.is_empty());
    assert_eq!(trace.finishes, vec![true]);
}

#[test]
fn the_playback_reference_opens_and_closes_with_the_phrase() {
    let (sink, _trace) = fake_sink();
    let reference = PlaybackReference::new();
    let stop = AtomicBool::new(false);
    play_samples(
        sink.as_ref(),
        &[0.5; 100],
        24_000,
        &stop,
        None,
        None,
        None,
        Some(&reference),
    )
    .expect("suena");
    assert!(!reference.is_active(), "la frase quedó cerrada");
    assert_eq!(reference.phrase(), 1);
    // Un corte también deja la referencia inactiva (el `Drop` la cierra).
    let stop = AtomicBool::new(true);
    play_samples(
        sink.as_ref(),
        &[0.5; 100],
        24_000,
        &stop,
        None,
        None,
        None,
        Some(&reference),
    )
    .expect("corta");
    assert!(!reference.is_active());
    assert_eq!(reference.phrase(), 2);
}

#[test]
fn defaults_match_the_python_voice_config() {
    let cfg = VoiceConfig::default();
    assert_eq!(text_or(&cfg, "voice", "em_alex"), "em_alex");
    assert!((number_or(&cfg, "speed", 0.95) - 0.95).abs() < f64::EPSILON);
    assert_eq!(device_index(&cfg), None);
}

#[test]
fn missing_model_files_report_the_python_text() {
    let cfg =
        config(r#"{"kokoro_model_path":"no-existe.onnx","kokoro_voices_path":"no-existe.bin"}"#);
    let error = match Speaker::new(cfg) {
        Ok(_) => panic!("no debería cargar sin archivos"),
        Err(error) => error,
    };
    assert!(
        error
            .0
            .starts_with("Voices file not found at no-existe.bin"),
        "{error}"
    );
}

#[test]
fn synthesize_normalizes_and_uses_spanish() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (speaker, _, model) = speaker(config(r#"{"voice":"voz"}"#), Arc::clone(&calls));
    let (audio, rate) = speaker.synthesize("EzAgent.").expect("síntesis");
    assert_eq!(rate, 24_000);
    assert!(!audio.is_empty());
    assert_eq!(
        calls.lock()[0],
        ("Es Agente.".to_string(), "es".to_string()),
        "normalize_for_kokoro antes de create y lang=es"
    );
    assert_eq!(model.lock().speed, Some(0.95));
}

#[test]
fn synthesize_reads_the_current_config() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (speaker, _, model) = speaker(config(r#"{"voice":"voz"}"#), calls);
    speaker.set_config(config(r#"{"voice":"voz","speed":1.25}"#));
    speaker.synthesize("hola").expect("síntesis");
    assert_eq!(model.lock().speed, Some(1.25));
}

#[test]
fn speak_synthesizes_then_plays_at_the_returned_rate() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (speaker, trace, _) = speaker(config(r#"{"voice":"voz"}"#), calls);
    let stop = AtomicBool::new(false);
    let mut starts = 0;
    let sounded = speaker
        .speak("hola", &stop, None, Some(&mut || starts += 1))
        .expect("habla");
    assert!(sounded);
    let trace = trace.lock();
    assert_eq!(trace.rates, vec![24_000]);
    assert!(!trace.chunks.is_empty());
    assert_eq!(starts, 1);
}

#[test]
fn set_voice_accepts_combined_names_and_rejects_unknown_ones() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (speaker, _, _) = speaker(config(r#"{"voice":"voz"}"#), calls);
    speaker.set_voice("voz+otra").expect("voz combinada");
    let error = speaker.set_voice("nadie").expect_err("voz inválida");
    assert_eq!(error.0, "Ninguna voz de Kokoro válida en 'nadie'");
}

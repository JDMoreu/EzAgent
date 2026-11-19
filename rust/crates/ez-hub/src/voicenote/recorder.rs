//! Micrófono real de la nota: captura en `ez-hub-voice-note` y niveles al evento
//! `hub://voice-level` a lo sumo diez veces por segundo.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use ez_audio::barge_in::BargeInMode;
use ez_audio::capture::{CaptureError, MicCapture, StreamHooks, VadSettings};
use ez_core::config::VoiceConfig;
use ez_core::pyvalue::text_or_empty;
use parking_lot::Mutex;
use serde_json::Value;

use super::{join_fragments, NoteError, NoteRecorder, Pcm, LEVEL_INTERVAL};

type LevelFn = Arc<dyn Fn(f64) + Send + Sync>;

/// Captura con `MicCapture`; el hilo se lanza al empezar y se reúne al parar.
pub struct MicNoteRecorder {
    config: VoiceConfig,
    emit_level: LevelFn,
    stop: Arc<AtomicBool>,
    fragments: Arc<Mutex<Vec<Vec<u8>>>>,
    handle: Option<JoinHandle<Result<(), CaptureError>>>,
    rate: u32,
}

impl MicNoteRecorder {
    pub fn new(config: VoiceConfig, emit_level: LevelFn) -> Self {
        Self {
            config,
            emit_level,
            stop: Arc::new(AtomicBool::new(false)),
            fragments: Arc::new(Mutex::new(Vec::new())),
            handle: None,
            rate: 16_000,
        }
    }
}

impl NoteRecorder for MicNoteRecorder {
    fn start(&mut self) -> Result<(), NoteError> {
        self.stop.store(false, Ordering::SeqCst);
        let mic = mic_capture(&self.config);
        self.rate = mic.rate();
        let config = self.config.clone();
        let emit = Arc::clone(&self.emit_level);
        let stop = Arc::clone(&self.stop);
        let fragments = Arc::clone(&self.fragments);
        let handle = thread::Builder::new()
            .name("ez-hub-voice-note".to_string())
            .spawn(move || {
                let mut last = Instant::now()
                    .checked_sub(LEVEL_INTERVAL)
                    .unwrap_or_else(Instant::now);
                let mut on_level = |level: f64| {
                    let now = Instant::now();
                    if now.duration_since(last) >= LEVEL_INTERVAL {
                        last = now;
                        emit(level);
                    }
                };
                let mut on_utterance = |pcm: &[u8], _rate: u32| {
                    if !pcm.is_empty() {
                        fragments.lock().push(pcm.to_vec());
                    }
                };
                let hooks = StreamHooks {
                    on_level: Some(&mut on_level),
                    on_utterance: Some(&mut on_utterance),
                    ..StreamHooks::default()
                };
                mic.stream(
                    &stop,
                    hooks,
                    None,
                    None,
                    cfg_float(&config, "barge_in_rms", 0.008),
                )
            })
            .map_err(|_| NoteError::NoMicrophone)?;
        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) -> Result<Pcm, NoteError> {
        self.stop.store(true, Ordering::SeqCst);
        let handle = self.handle.take().ok_or(NoteError::NoMicrophone)?;
        let outcome = handle.join();
        let pieces = self.fragments.lock().clone();
        match outcome {
            Ok(Ok(())) => Ok(Pcm {
                samples: join_fragments(&pieces),
                rate: self.rate,
            }),
            _ => Err(NoteError::NoMicrophone),
        }
    }
}

/// Misma construcción que la app de voz (`app/core/callbacks.rs::mic_capture`).
fn mic_capture(cfg: &VoiceConfig) -> MicCapture {
    let index = cfg
        .get("input_device_index")
        .and_then(Value::as_i64)
        .and_then(|value| usize::try_from(value).ok());
    let name = text_or_empty(cfg.get("input_device_name"));
    let name = if name.is_empty() { None } else { Some(name) };
    let mode = match cfg.get("barge_in_mode").and_then(Value::as_str) {
        Some("fixed") => BargeInMode::Fixed,
        _ => BargeInMode::Adaptive,
    };
    MicCapture::new_named(
        index,
        name.as_deref(),
        VadSettings {
            min_rms: cfg_float(cfg, "min_rms", 0.0006),
            silence_secs: cfg_float(cfg, "silence_secs", 1.3),
            pre_roll_secs: cfg_float(cfg, "pre_roll_secs", 0.6),
            min_speech_secs: cfg_float(cfg, "min_speech_secs", 0.2),
            max_utterance_secs: cfg_float(cfg, "max_utterance_secs", 30.0),
            noise_multiplier: cfg_float(cfg, "noise_multiplier", 1.3),
            turn_quick_secs: cfg_float(cfg, "turn_quick_secs", 0.35),
            barge_in_rms: cfg_float(cfg, "barge_in_rms", 0.008),
            barge_in_secs: cfg_float(cfg, "barge_in_secs", 0.25),
            barge_in_mode: mode,
        },
    )
}

/// Lectura de un número de `voice.json`, con los tipos que acepta Python.
fn cfg_float(cfg: &VoiceConfig, key: &str, default: f64) -> f64 {
    match cfg.get(key) {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(default),
        Some(Value::Bool(flag)) => {
            if *flag {
                1.0
            } else {
                0.0
            }
        }
        Some(Value::String(text)) => text.trim().parse().unwrap_or(default),
        _ => default,
    }
}

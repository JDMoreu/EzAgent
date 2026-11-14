//! `MicCapture`: micrófono real con captura continua y segmentación por VAD.
//!
//! El bucle de audio solo toca memoria y canales acotados: ni red ni disco, y
//! nunca bloquea al hilo del dispositivo.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use crossbeam_channel::{bounded, RecvTimeoutError};

use super::{
    device_at, device_name, input_config, pcm_rms, pick_input_device, to_mono_i16, CaptureError,
};
use crate::barge_in::{BargeInDetector, BARGE_IN_SECS};
use crate::segmenter::{EndProbe, Segmenter, SegmenterConfig};
use crate::turn::SmartTurn;
use crate::CHUNK_SECS;

/// Valores en vivo del VAD y del barge-in: el menú del HUD los cambia sin
/// esperar otra llamada.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VadSettings {
    pub min_rms: f64,
    pub silence_secs: f64,
    pub pre_roll_secs: f64,
    pub min_speech_secs: f64,
    pub max_utterance_secs: f64,
    pub noise_multiplier: f64,
    pub turn_quick_secs: f64,
    pub barge_in_rms: f64,
    pub barge_in_secs: f64,
}

impl Default for VadSettings {
    fn default() -> Self {
        Self {
            min_rms: 0.0006,
            silence_secs: 2.5,
            pre_roll_secs: 0.6,
            min_speech_secs: 0.2,
            max_utterance_secs: 30.0,
            noise_multiplier: 1.3,
            turn_quick_secs: 0.35,
            barge_in_rms: 0.0,
            barge_in_secs: BARGE_IN_SECS,
        }
    }
}

/// Callbacks de `MicCapture.stream`.
pub type LevelCallback<'a> = &'a mut dyn FnMut(f64);
pub type UtteranceCallback<'a> = &'a mut dyn FnMut(&[u8], u32);

#[derive(Default)]
pub struct StreamHooks<'a> {
    pub on_level: Option<LevelCallback<'a>>,
    pub on_speech_start: Option<LevelCallback<'a>>,
    pub on_utterance: Option<UtteranceCallback<'a>>,
    pub on_barge_in: Option<LevelCallback<'a>>,
}

/// Lo que el pipeline pide emitir, en el mismo orden que el Python.
#[derive(Clone, Debug, PartialEq)]
pub enum CaptureEvent {
    SpeechStart(f64),
    /// `Some(vec![])` = turno descartado (clic, soplido).
    Utterance(Vec<u8>),
    BargeIn(f64),
}

/// Lógica pura del bucle de captura: se prueba con audio sintético.
pub struct CapturePipeline {
    pub segmenter: Segmenter,
    pub barge: BargeInDetector,
    pub rate: u32,
    pub last_speech_end: Option<f64>,
}

impl CapturePipeline {
    pub fn new(settings: &VadSettings, rate: u32, end_probe: Option<EndProbe>) -> Self {
        let config = SegmenterConfig {
            min_rms: settings.min_rms,
            silence_secs: settings.silence_secs,
            pre_roll_secs: settings.pre_roll_secs,
            min_speech_secs: settings.min_speech_secs,
            max_utterance_secs: settings.max_utterance_secs,
            noise_multiplier: settings.noise_multiplier,
            end_probe,
            quick_silence_secs: settings.turn_quick_secs,
            ..SegmenterConfig::default()
        };
        Self {
            segmenter: Segmenter::new(config),
            barge: BargeInDetector::new(settings.barge_in_rms, settings.barge_in_secs),
            rate,
            last_speech_end: None,
        }
    }

    /// Consume un chunk y devuelve los eventos que toca emitir.
    pub fn feed(
        &mut self,
        settings: &VadSettings,
        chunk: &[u8],
        level: f64,
        speaking: bool,
        reset: bool,
    ) -> Vec<CaptureEvent> {
        self.apply(settings);
        if reset {
            // Fin del TTS: tira el eco acumulado antes de escuchar.
            self.segmenter.reset();
        }
        let mut events = Vec::new();
        let mut chunks = vec![(chunk.to_vec(), level)];
        if speaking && self.barge.threshold > 0.0 {
            if let Some(run) = self.barge.feed(chunk, level) {
                // El usuario habla por encima del TTS: corta y reinyecta el
                // principio de su frase para que Whisper no la oiga mocha.
                events.push(CaptureEvent::BargeIn(level));
                self.segmenter.reset();
                chunks = run;
            }
        } else {
            self.barge.reset();
        }
        for (data, chunk_level) in chunks {
            let (pcm, started) = self.segmenter.feed(&data, chunk_level);
            if started {
                let (start, end) = self.segmenter.thresholds();
                tracing::debug!(
                    "VAD start: level={chunk_level:.4} noise={:.4} start={start:.4} end={end:.4}",
                    self.segmenter.noise
                );
                events.push(CaptureEvent::SpeechStart(chunk_level));
            }
            match pcm {
                Some(pcm) if pcm.is_empty() => {
                    tracing::debug!("VAD end: descartado (sin voz sostenida)");
                    events.push(CaptureEvent::Utterance(Vec::new()));
                }
                Some(pcm) => {
                    self.last_speech_end = self.segmenter.last_speech_end;
                    let reason = match self.segmenter.last_close_reason.as_str() {
                        "turn" => " (fin de turno detectado)",
                        "max" => " (duración máxima)",
                        _ => "",
                    };
                    let seconds = pcm.len() as f64 / 2.0 / f64::from(self.rate);
                    tracing::debug!("VAD end: {seconds:.2}s{reason}");
                    events.push(CaptureEvent::Utterance(pcm));
                }
                None => {}
            }
        }
        events
    }

    fn apply(&mut self, settings: &VadSettings) {
        self.segmenter.min_rms = settings.min_rms;
        self.segmenter.silence_secs = settings.silence_secs;
        self.segmenter.noise_multiplier = settings.noise_multiplier;
        self.segmenter.min_speech_secs = settings.min_speech_secs;
        self.segmenter.max_utterance_secs = settings.max_utterance_secs;
        self.segmenter.quick_silence_secs = settings.turn_quick_secs;
        self.segmenter.set_pre_roll(settings.pre_roll_secs);
        self.barge.threshold = settings.barge_in_rms;
        self.barge.secs = settings.barge_in_secs;
    }
}

/// Micrófono real con captura continua y segmentación por VAD.
pub struct MicCapture {
    pub device_index: Option<usize>,
    pub device_name: String,
    pub rate: u32,
    pub min_rms: f64,
    pub silence_secs: f64,
    pub pre_roll_secs: f64,
    pub min_speech_secs: f64,
    pub max_utterance_secs: f64,
    pub noise_multiplier: f64,
    pub barge_in_rms: f64,
    pub barge_in_secs: f64,
    /// Detector de fin de turno (Smart Turn); `None` = solo silencio.
    pub turn_detector: Option<Arc<SmartTurn>>,
    pub turn_quick_secs: f64,
    pub last_speech_end: Option<f64>,
}

impl MicCapture {
    pub fn new(device_index: Option<usize>, settings: VadSettings) -> Self {
        let mut capture = Self {
            device_index: None,
            device_name: "ninguno".to_string(),
            rate: 16_000,
            min_rms: settings.min_rms,
            silence_secs: settings.silence_secs,
            pre_roll_secs: settings.pre_roll_secs,
            min_speech_secs: settings.min_speech_secs,
            max_utterance_secs: settings.max_utterance_secs,
            noise_multiplier: settings.noise_multiplier,
            barge_in_rms: settings.barge_in_rms,
            barge_in_secs: settings.barge_in_secs,
            turn_detector: None,
            turn_quick_secs: settings.turn_quick_secs,
            last_speech_end: None,
        };
        capture.device_index = pick_input_device(device_index);
        if let Some(index) = capture.device_index {
            match device_at(index) {
                Some(device) => {
                    let rate = device
                        .default_input_config()
                        .map_or(16_000, |config| config.sample_rate());
                    capture.rate = if rate == 0 { 16_000 } else { rate };
                    capture.device_name = device_name(&device.to_string());
                }
                None => capture.device_name = "desconocido".to_string(),
            }
        }
        capture
    }

    /// `close`: cpal no tiene `terminate`; el stream se cierra al salir de
    /// `stream`. Se conserva por paridad con la API del Python.
    pub fn close(&mut self) {}

    fn end_probe(&self) -> Option<EndProbe> {
        let detector = Arc::clone(self.turn_detector.as_ref()?);
        let rate = self.rate;
        Some(Box::new(move |pcm: &[u8]| detector.submit(pcm, rate)))
    }

    fn settings(&self) -> VadSettings {
        VadSettings {
            min_rms: self.min_rms,
            silence_secs: self.silence_secs,
            pre_roll_secs: self.pre_roll_secs,
            min_speech_secs: self.min_speech_secs,
            max_utterance_secs: self.max_utterance_secs,
            noise_multiplier: self.noise_multiplier,
            turn_quick_secs: self.turn_quick_secs,
            barge_in_rms: self.barge_in_rms,
            barge_in_secs: self.barge_in_secs,
        }
    }

    /// Captura continua; emite utterances y no se cierra entre turnos.
    pub fn stream(
        &mut self,
        stop_event: &AtomicBool,
        hooks: StreamHooks<'_>,
        reset_event: Option<&AtomicBool>,
        is_speaking: Option<&dyn Fn() -> bool>,
        barge_in_rms: f64,
    ) -> Result<(), CaptureError> {
        let index = self.device_index.ok_or(CaptureError::NoMicrophone)?;
        let device = device_at(index).ok_or(CaptureError::NoMicrophone)?;
        let (stream, receiver) = open_stream(&device, self.rate)?;
        let mut hooks = hooks;
        let mut pipeline = CapturePipeline::new(&self.settings(), self.rate, self.end_probe());
        let frames = ((f64::from(self.rate) * CHUNK_SECS) as usize).max(160);
        let mut pending: Vec<i16> = Vec::with_capacity(frames * 2);
        while !stop_event.load(Ordering::SeqCst) {
            let batch = match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(batch) => batch,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            pending.extend_from_slice(&batch);
            while pending.len() >= frames {
                let pcm = take_chunk(&mut pending, frames);
                let level = pcm_rms(&pcm);
                let settings = self.live_settings(barge_in_rms);
                let reset = reset_event.is_some_and(|flag| flag.swap(false, Ordering::SeqCst));
                let speaking = is_speaking.is_some_and(|check| check());
                let events = pipeline.feed(&settings, &pcm, level, speaking, reset);
                self.last_speech_end = pipeline.last_speech_end;
                dispatch(&mut hooks, events, level, self.rate);
            }
        }
        drop(stream);
        Ok(())
    }

    fn live_settings(&self, barge_in_rms: f64) -> VadSettings {
        let mut settings = self.settings();
        // `self.barge_in_rms or barge_in_rms`.
        settings.barge_in_rms = if self.barge_in_rms != 0.0 {
            self.barge_in_rms
        } else {
            barge_in_rms
        };
        settings
    }
}

/// Abre el stream de entrada y devuelve el receptor de buffers mono.
fn open_stream(
    device: &cpal::Device,
    rate: u32,
) -> Result<(cpal::Stream, crossbeam_channel::Receiver<Vec<i16>>), CaptureError> {
    let (config, format) = input_config(device, rate).ok_or(CaptureError::NoMicrophone)?;
    let channels = usize::from(config.channels);
    let (sender, receiver) = bounded::<Vec<i16>>(64);
    let stream = device
        .build_input_stream_raw(
            config,
            format,
            move |data, _| {
                let mono = to_mono_i16(data, channels);
                if !mono.is_empty() {
                    // Sin bloquear al hilo de audio: si el consumidor va lento,
                    // se descarta este buffer.
                    let _ = sender.try_send(mono);
                }
            },
            |error| tracing::warn!("Audio: {error}"),
            Some(Duration::from_secs(3)),
        )
        .map_err(|error| CaptureError::Device(error.to_string()))?;
    stream
        .play()
        .map_err(|error| CaptureError::Device(error.to_string()))?;
    Ok((stream, receiver))
}

fn take_chunk(pending: &mut Vec<i16>, frames: usize) -> Vec<u8> {
    pending.drain(..frames).flat_map(i16::to_le_bytes).collect()
}

/// `_emit`: on_barge_in, on_level y después el habla, en ese orden.
fn dispatch(hooks: &mut StreamHooks<'_>, events: Vec<CaptureEvent>, level: f64, rate: u32) {
    let (barge, rest): (Vec<_>, Vec<_>) = events
        .into_iter()
        .partition(|event| matches!(event, CaptureEvent::BargeIn(_)));
    for event in barge {
        if let (CaptureEvent::BargeIn(level), Some(callback)) =
            (event, hooks.on_barge_in.as_deref_mut())
        {
            callback(level);
        }
    }
    if let Some(callback) = hooks.on_level.as_deref_mut() {
        callback(level);
    }
    for event in rest {
        match event {
            CaptureEvent::SpeechStart(level) => {
                if let Some(callback) = hooks.on_speech_start.as_deref_mut() {
                    callback(level);
                }
            }
            CaptureEvent::Utterance(pcm) => {
                if let Some(callback) = hooks.on_utterance.as_deref_mut() {
                    callback(&pcm, rate);
                }
            }
            CaptureEvent::BargeIn(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
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
}

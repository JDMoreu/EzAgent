//! Ajustes y estado que `MicCapture` comparte con el hilo de la llamada.
//!
//! `audio.py` lee `self.min_rms`, `self.turn_detector`… en cada bloque, así que
//! el menú del HUD los cambia en vivo. Aquí viven en memoria compartida para
//! que nadie tenga que retener el micrófono mientras dura la captura.

use std::sync::atomic::{AtomicI64, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use super::{MicCapture, VadSettings};
use crate::barge_in::{BargeInMode, PlaybackReference};
use crate::turn::SmartTurn;

/// `f64` atómico (`f64::to_bits`) para leer un ajuste sin candado.
#[derive(Debug, Default)]
pub(super) struct AtomicF64(AtomicU64);

impl AtomicF64 {
    fn new(value: f64) -> Self {
        Self(AtomicU64::new(value.to_bits()))
    }

    pub(super) fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub(super) fn set(&self, value: f64) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }
}

/// Los atributos de `MicCapture` que se leen o escriben durante la llamada.
pub(super) struct Live {
    /// Índice en la enumeración de cpal (el que abre el stream).
    device_index: AtomicI64,
    /// Índice global de PortAudio (el que se guarda y compara la app).
    portaudio_index: AtomicI64,
    min_rms: AtomicF64,
    silence_secs: AtomicF64,
    pre_roll_secs: AtomicF64,
    min_speech_secs: AtomicF64,
    max_utterance_secs: AtomicF64,
    noise_multiplier: AtomicF64,
    barge_in_rms: AtomicF64,
    barge_in_secs: AtomicF64,
    barge_in_mode: AtomicU8,
    turn_quick_secs: AtomicF64,
    /// Envolvente de lo que suena en el altavoz; la pone la app al empezar.
    playback: Mutex<Option<Arc<PlaybackReference>>>,
    /// Detector de fin de turno (Smart Turn); `None` = solo silencio.
    turn_detector: Mutex<Option<Arc<SmartTurn>>>,
    last_speech_end: Mutex<Option<f64>>,
}

impl Live {
    pub(super) fn new(
        device_index: Option<usize>,
        portaudio_index: Option<usize>,
        settings: VadSettings,
    ) -> Self {
        Self {
            device_index: AtomicI64::new(index_id(device_index)),
            portaudio_index: AtomicI64::new(index_id(portaudio_index)),
            min_rms: AtomicF64::new(settings.min_rms),
            silence_secs: AtomicF64::new(settings.silence_secs),
            pre_roll_secs: AtomicF64::new(settings.pre_roll_secs),
            min_speech_secs: AtomicF64::new(settings.min_speech_secs),
            max_utterance_secs: AtomicF64::new(settings.max_utterance_secs),
            noise_multiplier: AtomicF64::new(settings.noise_multiplier),
            barge_in_rms: AtomicF64::new(settings.barge_in_rms),
            barge_in_secs: AtomicF64::new(settings.barge_in_secs),
            barge_in_mode: AtomicU8::new(mode_id(settings.barge_in_mode)),
            turn_quick_secs: AtomicF64::new(settings.turn_quick_secs),
            playback: Mutex::new(None),
            turn_detector: Mutex::new(None),
            last_speech_end: Mutex::new(None),
        }
    }

    pub(super) fn settings(&self) -> VadSettings {
        VadSettings {
            min_rms: self.min_rms.get(),
            silence_secs: self.silence_secs.get(),
            pre_roll_secs: self.pre_roll_secs.get(),
            min_speech_secs: self.min_speech_secs.get(),
            max_utterance_secs: self.max_utterance_secs.get(),
            noise_multiplier: self.noise_multiplier.get(),
            barge_in_rms: self.barge_in_rms.get(),
            barge_in_secs: self.barge_in_secs.get(),
            barge_in_mode: mode_of(self.barge_in_mode.load(Ordering::Relaxed)),
            turn_quick_secs: self.turn_quick_secs.get(),
        }
    }

    pub(super) fn playback_reference(&self) -> Option<Arc<PlaybackReference>> {
        self.playback.lock().clone()
    }

    pub(super) fn turn_detector(&self) -> Option<Arc<SmartTurn>> {
        self.turn_detector.lock().clone()
    }
}

fn index_id(index: Option<usize>) -> i64 {
    index.map_or(-1, |value| value as i64)
}

fn mode_id(mode: BargeInMode) -> u8 {
    match mode {
        BargeInMode::Fixed => 0,
        BargeInMode::Adaptive => 1,
    }
}

fn mode_of(value: u8) -> BargeInMode {
    match value {
        1 => BargeInMode::Adaptive,
        _ => BargeInMode::Fixed,
    }
}

impl MicCapture {
    /// Índice de PortAudio del micrófono en uso (`self.mic.device_index` del
    /// Python): el que se guarda en `voice.json` y el que entiende la app de
    /// Python.
    pub fn device_index(&self) -> Option<usize> {
        usize::try_from(self.inner.live.portaudio_index.load(Ordering::Relaxed)).ok()
    }

    /// Índice en la enumeración de cpal: el que abre el stream.
    pub(super) fn cpal_index(&self) -> Option<usize> {
        usize::try_from(self.inner.live.device_index.load(Ordering::Relaxed)).ok()
    }

    /// Cambia el dispositivo en vivo; `stream` lo consulta al abrir.
    pub fn set_device_index(&self, index: Option<usize>) {
        self.inner
            .live
            .device_index
            .store(index_id(index), Ordering::Relaxed);
        self.inner
            .live
            .portaudio_index
            .store(index_id(index), Ordering::Relaxed);
    }

    pub fn device_name(&self) -> &str {
        &self.inner.device_name
    }

    pub fn rate(&self) -> u32 {
        self.inner.rate
    }

    /// True mientras `stream` corre en el hilo de la llamada.
    pub fn capturing(&self) -> bool {
        self.inner.capturing.load(Ordering::SeqCst)
    }

    pub fn turn_detector(&self) -> Option<Arc<SmartTurn>> {
        self.inner.live.turn_detector()
    }

    pub fn set_turn_detector(&self, detector: Option<Arc<SmartTurn>>) {
        *self.inner.live.turn_detector.lock() = detector;
    }

    pub fn last_speech_end(&self) -> Option<f64> {
        *self.inner.live.last_speech_end.lock()
    }

    pub fn set_last_speech_end(&self, at: Option<f64>) {
        *self.inner.live.last_speech_end.lock() = at;
    }

    pub fn min_rms(&self) -> f64 {
        self.inner.live.min_rms.get()
    }

    pub fn set_min_rms(&self, value: f64) {
        self.inner.live.min_rms.set(value);
    }

    pub fn silence_secs(&self) -> f64 {
        self.inner.live.silence_secs.get()
    }

    pub fn set_silence_secs(&self, value: f64) {
        self.inner.live.silence_secs.set(value);
    }

    pub fn pre_roll_secs(&self) -> f64 {
        self.inner.live.pre_roll_secs.get()
    }

    pub fn set_pre_roll_secs(&self, value: f64) {
        self.inner.live.pre_roll_secs.set(value);
    }

    pub fn min_speech_secs(&self) -> f64 {
        self.inner.live.min_speech_secs.get()
    }

    pub fn set_min_speech_secs(&self, value: f64) {
        self.inner.live.min_speech_secs.set(value);
    }

    pub fn max_utterance_secs(&self) -> f64 {
        self.inner.live.max_utterance_secs.get()
    }

    pub fn set_max_utterance_secs(&self, value: f64) {
        self.inner.live.max_utterance_secs.set(value);
    }

    pub fn noise_multiplier(&self) -> f64 {
        self.inner.live.noise_multiplier.get()
    }

    pub fn set_noise_multiplier(&self, value: f64) {
        self.inner.live.noise_multiplier.set(value);
    }

    pub fn barge_in_rms(&self) -> f64 {
        self.inner.live.barge_in_rms.get()
    }

    pub fn set_barge_in_rms(&self, value: f64) {
        self.inner.live.barge_in_rms.set(value);
    }

    pub fn barge_in_secs(&self) -> f64 {
        self.inner.live.barge_in_secs.get()
    }

    pub fn set_barge_in_secs(&self, value: f64) {
        self.inner.live.barge_in_secs.set(value);
    }

    pub fn barge_in_mode(&self) -> BargeInMode {
        mode_of(self.inner.live.barge_in_mode.load(Ordering::Relaxed))
    }

    pub fn set_barge_in_mode(&self, mode: BargeInMode) {
        self.inner
            .live
            .barge_in_mode
            .store(mode_id(mode), Ordering::Relaxed);
    }

    /// El altavoz publica aquí lo que suena; `None` deja el modo clásico.
    pub fn set_playback_reference(&self, reference: Option<Arc<PlaybackReference>>) {
        *self.inner.live.playback.lock() = reference;
    }

    pub fn turn_quick_secs(&self) -> f64 {
        self.inner.live.turn_quick_secs.get()
    }

    pub fn set_turn_quick_secs(&self, value: f64) {
        self.inner.live.turn_quick_secs.set(value);
    }

    /// Instantánea de los ajustes del VAD para el bloque siguiente.
    pub(super) fn settings(&self) -> VadSettings {
        self.inner.live.settings()
    }
}

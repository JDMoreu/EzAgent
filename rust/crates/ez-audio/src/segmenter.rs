//! Port of `src/ezagent/voice/audio.py` (`Segmenter`) (F4-01).
//!
//! Convierte un flujo continuo de chunks en utterances con VAD. Basado en
//! Dicta (MIT): suelo de ruido adaptativo (percentil de los niveles en
//! silencio), umbral de arranque más alto que el de cierre (histéresis) y una
//! espera de silencio larga, para no cortar al usuario en la primera pausa.

use std::collections::VecDeque;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use parking_lot::Mutex;

use crate::CHUNK_SECS;

/// Smart Turn mira como mucho los últimos 8 s.
const PROBE_CHUNKS: usize = (8.0f64 / CHUNK_SECS) as usize + 1;
const QUIET_CAPACITY: usize = 100;
const CALIBRATION_SAMPLES: usize = 20;
const START_CHUNKS: usize = 3;

/// `Future[bool]` de una sonda de Smart Turn: se completa desde otro hilo.
#[derive(Clone, Debug, Default)]
pub struct Probe {
    value: Arc<Mutex<Option<Result<bool, String>>>>,
}

impl Probe {
    pub fn new() -> Self {
        Self::default()
    }

    /// Deja el veredicto (`True` = turno terminado).
    pub fn finish(&self, ended: bool) {
        *self.value.lock() = Some(Ok(ended));
    }

    /// Marca la sonda como fallida; el segmentador la ignora y sigue esperando.
    pub fn fail(&self, message: impl Into<String>) {
        *self.value.lock() = Some(Err(message.into()));
    }

    pub fn done(&self) -> bool {
        self.value.lock().is_some()
    }

    /// `Some(true/false)` si terminó con veredicto; `None` si sigue o falló.
    pub fn result(&self) -> Option<bool> {
        self.value
            .lock()
            .as_ref()
            .and_then(|result| result.as_ref().ok().copied())
    }
}

/// `end_probe(pcm) -> Future[bool] | None`.
pub type EndProbe = Box<dyn Fn(&[u8]) -> Option<Probe> + Send + Sync>;

/// Valores de arranque de `Segmenter.__init__`.
pub struct SegmenterConfig {
    pub min_rms: f64,
    pub silence_secs: f64,
    pub pre_roll_secs: f64,
    pub min_speech_secs: f64,
    pub max_utterance_secs: f64,
    pub noise_multiplier: f64,
    pub end_probe: Option<EndProbe>,
    pub quick_silence_secs: f64,
    pub min_voiced_secs: f64,
    pub probe_interval_secs: f64,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            min_rms: 0.0006,
            silence_secs: 1.3,
            pre_roll_secs: 0.6,
            min_speech_secs: 0.2,
            max_utterance_secs: 30.0,
            noise_multiplier: 1.3,
            end_probe: None,
            quick_silence_secs: 0.35,
            min_voiced_secs: 0.15,
            probe_interval_secs: 0.3,
        }
    }
}

/// Reloj monótono en segundos, como `time.monotonic()`.
fn monotonic_secs() -> f64 {
    static BASE: OnceLock<Instant> = OnceLock::new();
    BASE.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// `int(seconds / CHUNK_SECS)` con el mínimo de un chunk.
fn pre_roll_len(seconds: f64) -> usize {
    ((seconds / CHUNK_SECS) as usize).max(1)
}

/// `np.percentile(values, q)` con interpolación lineal.
fn percentile_linear(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let position = (sorted.len() - 1) as f64 * q / 100.0;
    let low = position.floor();
    let index = low as usize;
    let fraction = position - low;
    match sorted.get(index + 1) {
        Some(high) => sorted[index] + fraction * (high - sorted[index]),
        None => sorted[index],
    }
}

pub struct Segmenter {
    pub min_rms: f64,
    /// Segundos de chunks por encima del umbral de arranque que debe tener un
    /// turno. Un clic o un soplido no los suman; no cambia ninguna espera.
    pub min_voiced_secs: f64,
    pub end_probe: Option<EndProbe>,
    pub quick_silence_secs: f64,
    pub probe_interval_secs: f64,
    /// Motivo del último cierre: «silence», «turn» o «max».
    pub last_close_reason: String,
    pub silence_secs: f64,
    pub min_speech_secs: f64,
    pub max_utterance_secs: f64,
    pub noise_multiplier: f64,
    pub pre_roll_secs: f64,
    pub last_speech_end: Option<f64>,
    pub noise: f64,
    voiced_secs: f64,
    pending: Option<Probe>,
    /// Silencio acumulado (segundos) en el que toca la próxima sonda.
    next_probe_at: f64,
    pre: VecDeque<Vec<u8>>,
    pre_capacity: usize,
    buf: Vec<Vec<u8>>,
    in_speech: bool,
    speech_run: usize,
    silence_run: f64,
    speech_secs: f64,
    speech_end_at: Option<f64>,
    quiet: VecDeque<f64>,
    noise_samples: usize,
}

impl Default for Segmenter {
    fn default() -> Self {
        Self::new(SegmenterConfig::default())
    }
}

impl Segmenter {
    pub fn new(config: SegmenterConfig) -> Self {
        let pre_capacity = pre_roll_len(config.pre_roll_secs);
        Self {
            min_rms: config.min_rms,
            min_voiced_secs: config.min_voiced_secs,
            voiced_secs: 0.0,
            end_probe: config.end_probe,
            quick_silence_secs: config.quick_silence_secs,
            probe_interval_secs: config.probe_interval_secs,
            pending: None,
            next_probe_at: 0.0,
            last_close_reason: String::new(),
            silence_secs: config.silence_secs,
            min_speech_secs: config.min_speech_secs,
            max_utterance_secs: config.max_utterance_secs,
            noise_multiplier: config.noise_multiplier,
            pre_roll_secs: config.pre_roll_secs,
            pre: VecDeque::with_capacity(pre_capacity),
            pre_capacity,
            buf: Vec::new(),
            in_speech: false,
            speech_run: 0,
            silence_run: 0.0,
            speech_secs: 0.0,
            speech_end_at: None,
            last_speech_end: None,
            noise: 0.0,
            quiet: VecDeque::with_capacity(QUIET_CAPACITY),
            noise_samples: 0,
        }
    }

    /// Ajusta la antelación sin perder los chunks ya capturados.
    pub fn set_pre_roll(&mut self, seconds: f64) {
        // Como `max(0.0, float(seconds))`: NaN también cae en 0.0.
        let value = if seconds > 0.0 { seconds } else { 0.0 };
        self.pre_roll_secs = value;
        let maximum = pre_roll_len(value);
        self.pre_capacity = maximum;
        while self.pre.len() > maximum {
            self.pre.pop_front();
        }
    }

    /// Devuelve `(utterance, speech_started)`.
    ///
    /// `Some(vec![])` es un turno descartado (clic, soplido): avisa a quien
    /// escucha de que el arranque de voz no llegó a frase.
    pub fn feed(&mut self, chunk: &[u8], level: f64) -> (Option<Vec<u8>>, bool) {
        let now = monotonic_secs();
        if !self.in_speech {
            self.learn_noise(level);
            self.push_pre(chunk);
            return self.try_start(level);
        }
        self.feed_speech(chunk, level, now)
    }

    fn try_start(&mut self, level: f64) -> (Option<Vec<u8>>, bool) {
        let (start_threshold, _) = self.thresholds();
        if level >= start_threshold {
            self.speech_run += 1;
        } else {
            self.speech_run = 0;
        }
        if self.speech_run < START_CHUNKS {
            return (None, false);
        }
        self.in_speech = true;
        self.buf = self.pre.iter().cloned().collect();
        self.speech_secs = 0.0;
        self.voiced_secs = self.speech_run as f64 * CHUNK_SECS;
        self.silence_run = 0.0;
        self.pre.clear();
        (None, true)
    }

    fn feed_speech(&mut self, chunk: &[u8], level: f64, now: f64) -> (Option<Vec<u8>>, bool) {
        let (start_threshold, end_threshold) = self.thresholds();
        self.buf.push(chunk.to_vec());
        self.speech_secs += CHUNK_SECS;
        if level >= start_threshold {
            self.voiced_secs += CHUNK_SECS;
        }
        // Histéresis: dentro del turno solo cuenta como silencio lo que baja
        // del umbral de cierre, no cualquier valle por debajo del de arranque.
        if level < end_threshold {
            if self.silence_run == 0.0 {
                self.speech_end_at = Some(now);
            }
            self.silence_run += CHUNK_SECS;
        } else {
            self.silence_run = 0.0;
            self.speech_end_at = None;
            // Sigue hablando: la evaluación de esta pausa ya no vale.
            self.pending = None;
            self.next_probe_at = 0.0;
        }
        let reason = if self.silence_run >= self.silence_secs {
            "silence"
        } else if self.speech_secs >= self.max_utterance_secs {
            "max"
        } else if self.turn_finished() {
            "turn"
        } else {
            ""
        };
        if reason.is_empty() {
            return (None, false);
        }
        self.last_close_reason = reason.to_string();
        let pcm = self.buf.concat();
        let speech_secs = self.speech_secs;
        let speech_end = self.speech_end_at.unwrap_or(now);
        let voiced_secs = self.voiced_secs;
        self.reset_state();
        self.last_speech_end = Some(speech_end);
        if speech_secs >= self.min_speech_secs && voiced_secs >= self.min_voiced_secs {
            return (Some(pcm), false);
        }
        (Some(Vec::new()), false)
    }

    /// Primeros 0,6 s: calibra con el mínimo. Después solo aprende ruido de
    /// niveles cercanos al suelo, para que una frase floja no suba el listón.
    fn learn_noise(&mut self, level: f64) {
        if self.noise_samples < CALIBRATION_SAMPLES {
            self.push_quiet(level);
            self.noise_samples += 1;
            let lo = self.quiet.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = self.quiet.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if self.noise_samples >= 4 && hi - lo > (lo * 0.5).max(self.min_rms) {
                // La energía cambia demasiado para ser un ventilador o un
                // zumbido estable: es una frase que empezó al arrancar.
                self.noise = lo.min(self.min_rms);
                self.quiet.clear();
                self.noise_samples = CALIBRATION_SAMPLES;
            } else {
                self.noise = lo;
            }
        } else {
            let ceiling = (self.noise * 1.6).max(self.min_rms * 0.9);
            if level <= ceiling {
                self.push_quiet(level);
                self.noise_samples += 1;
                self.noise = percentile_linear(self.quiet.make_contiguous(), 20.0);
            }
        }
    }

    fn push_quiet(&mut self, level: f64) {
        if self.quiet.len() == QUIET_CAPACITY {
            self.quiet.pop_front();
        }
        self.quiet.push_back(level);
    }

    fn push_pre(&mut self, chunk: &[u8]) {
        if self.pre.len() == self.pre_capacity {
            self.pre.pop_front();
        }
        self.pre.push_back(chunk.to_vec());
    }

    /// True si Smart Turn ya dijo que la frase terminó en esta pausa.
    fn turn_finished(&mut self) -> bool {
        if self.end_probe.is_none() || self.silence_run < self.quick_silence_secs {
            return false;
        }
        if let Some(pending) = self.pending.take() {
            if !pending.done() {
                self.pending = Some(pending);
                return false;
            }
            if pending.result() == Some(true) {
                return true;
            }
            self.schedule_probe();
            return false;
        }
        if self.silence_run < self.next_probe_at {
            return false;
        }
        self.schedule_probe();
        let start = self.buf.len().saturating_sub(PROBE_CHUNKS);
        let tail = self.buf[start..].concat();
        let probe = self.end_probe.as_ref().and_then(|probe| probe(&tail));
        self.pending = probe;
        false
    }

    fn schedule_probe(&mut self) {
        self.next_probe_at = self.silence_run + self.probe_interval_secs;
    }

    pub fn thresholds(&self) -> (f64, f64) {
        let start = (self.noise * self.noise_multiplier).max(self.min_rms);
        let end = (self.noise * 1.1).max(self.min_rms * 0.6);
        (start, end)
    }

    pub fn flush(&mut self) -> Option<Vec<u8>> {
        if self.in_speech && self.speech_secs >= self.min_speech_secs {
            let pcm = self.buf.concat();
            self.reset_state();
            return Some(pcm);
        }
        self.reset_state();
        None
    }

    /// Olvida el turno en curso (p. ej. para tirar el eco del TTS).
    pub fn reset(&mut self) {
        self.reset_state();
    }

    fn reset_state(&mut self) {
        self.in_speech = false;
        self.buf.clear();
        self.speech_run = 0;
        self.silence_run = 0.0;
        self.speech_secs = 0.0;
        self.voiced_secs = 0.0;
        self.speech_end_at = None;
        self.pending = None;
        self.next_probe_at = 0.0;
        self.pre.clear();
    }
}

#[cfg(test)]
mod tests;

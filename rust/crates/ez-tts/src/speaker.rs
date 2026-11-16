//! Port of `src/ezagent/voice/tts.py` (`Speaker`) (F4-05).
//!
//! Kokoro is loaded once (F4-04). Playback uses `cpal` instead of PyAudio:
//! each phrase opens its own output stream, writes 480-sample chunks and can be
//! interrupted by a stop flag (barge-in). Synthesis and playback are guarded by
//! separate locks so the next sentence can be synthesized while the current one
//! is sounding.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ez_audio::barge_in::PlaybackReference;
use ez_core::config::VoiceConfig;
use ez_core::downloads::DownloadError;
use ez_core::models::resolve_kokoro_files;
use ez_core::pyvalue::{py_strip, repr_str};
use ez_core::speech_text::normalize_for_kokoro;
use parking_lot::{Mutex, RwLock};

use crate::kokoro::{Kokoro, KokoroError, SynthesisOptions, Voice};
use crate::voices::Style;

/// `frames_per_buffer=480` de `tts.py`: un chunk por escritura.
pub const CHUNK_SAMPLES: usize = 480;

/// `Speaker` failures, with the Python texts when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakerError(pub String);

impl std::fmt::Display for SpeakerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SpeakerError {}

impl From<String> for SpeakerError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<KokoroError> for SpeakerError {
    fn from(error: KokoroError) -> Self {
        Self(error.0)
    }
}

impl From<DownloadError> for SpeakerError {
    fn from(error: DownloadError) -> Self {
        Self(error.0)
    }
}

/// Output device used by `Speaker::play`.
///
/// Implementations may buffer the writes; `AudioStream::finish` is the call
/// that actually sounds them and reports a barge-in.
pub trait AudioSink: Send + Sync {
    /// Opens the output at the sample rate returned by Kokoro.
    ///
    /// `device_index` is `VoiceConfig.output_device_index` (read on every
    /// phrase, like the Python); `None` is the system default.
    fn open(&self, rate: u32, device_index: Option<usize>) -> Result<Box<dyn AudioStream>, String>;
}

/// One open output. `write` receives mono `int16` chunks of [`CHUNK_SAMPLES`].
pub trait AudioStream {
    fn write(&mut self, samples: &[i16]) -> Result<(), String>;

    /// Sounds what was written; `false` if `stop` interrupted it.
    ///
    /// `reference` es la envolvente que ve el detector de barge-in: se publica
    /// al ritmo real de salida, no al de `write` (que puede solo acumular).
    fn finish(
        &mut self,
        stop: &AtomicBool,
        reference: Option<&PlaybackReference>,
    ) -> Result<bool, String>;
}

/// Marca la frase como sonando mientras viva y la cierra al salir.
struct ReferenceGuard<'a>(Option<&'a PlaybackReference>);

impl Drop for ReferenceGuard<'_> {
    fn drop(&mut self) {
        if let Some(reference) = self.0 {
            reference.end();
        }
    }
}

/// `Speaker._play`: clip, chunk, report levels and stop on barge-in.
///
/// Kept apart from [`Speaker`] so tests can use a fake [`AudioSink`] without a
/// model or a sound card.
#[allow(clippy::too_many_arguments)]
pub fn play_samples(
    sink: &dyn AudioSink,
    samples: &[f32],
    sample_rate: u32,
    stop: &AtomicBool,
    device_index: Option<usize>,
    mut on_level: Option<&mut dyn FnMut(f64)>,
    mut on_start: Option<&mut dyn FnMut()>,
    reference: Option<&PlaybackReference>,
) -> Result<bool, SpeakerError> {
    let mut stream = sink.open(sample_rate, device_index)?;
    if let Some(reference) = reference {
        reference.begin();
    }
    let _reference = ReferenceGuard(reference);
    // shortcut: un stream por frase, como PyAudio (F14); medir en hardware el
    // coste de abrirlo y mantenerlo solo si supera el hueco audible.
    let pcm = to_pcm(samples);
    let mut started = false;
    for chunk in pcm.chunks(CHUNK_SAMPLES) {
        if stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        if !started {
            if let Some(callback) = on_start.as_mut() {
                callback();
            }
            started = true;
        }
        stream.write(chunk)?;
        if let Some(callback) = on_level.as_mut() {
            callback(pcm_rms(chunk));
        }
    }
    stream.finish(stop, reference).map_err(SpeakerError::from)
}

/// `(np.clip(samples, -1.0, 1.0) * 32767).astype(np.int16)`.
fn to_pcm(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|value| (value.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect()
}

fn pcm_rms(samples: &[i16]) -> f64 {
    let bytes: Vec<u8> = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    ez_audio::capture::pcm_rms(&bytes)
}

struct SpeakerInner {
    kokoro: Kokoro,
    style: Style,
}

/// `Speaker`: carga Kokoro una vez y reproduce frases cortas.
pub struct Speaker {
    cfg: RwLock<VoiceConfig>,
    inner: Mutex<SpeakerInner>,
    // Kokoro no es seguro con dos síntesis a la vez; la reproducción va aparte
    // para poder sintetizar la frase siguiente mientras suena otra.
    synth_lock: Mutex<()>,
    play_lock: Mutex<()>,
    sink: Arc<dyn AudioSink>,
    /// Envolvente de lo que suena, para el barge-in adaptativo de `ez-audio`.
    reference: Arc<PlaybackReference>,
}

impl Speaker {
    /// `Speaker(cfg)`: resuelve los archivos del modelo y carga Kokoro.
    pub fn new(cfg: VoiceConfig) -> Result<Self, SpeakerError> {
        Self::with_sink(cfg, Arc::new(CpalSink))
    }

    /// Injection seam: same as [`Speaker::new`] with a custom output.
    pub fn with_sink(cfg: VoiceConfig, sink: Arc<dyn AudioSink>) -> Result<Self, SpeakerError> {
        let kokoro = load_kokoro(&cfg)?;
        Self::assemble(cfg, kokoro, sink)
    }

    fn assemble(
        cfg: VoiceConfig,
        kokoro: Kokoro,
        sink: Arc<dyn AudioSink>,
    ) -> Result<Self, SpeakerError> {
        let style = speaker_style(&kokoro, &text_or(&cfg, "voice", "em_alex"))?;
        Ok(Self {
            cfg: RwLock::new(cfg),
            inner: Mutex::new(SpeakerInner { kokoro, style }),
            synth_lock: Mutex::new(()),
            play_lock: Mutex::new(()),
            sink,
            reference: Arc::new(PlaybackReference::new()),
        })
    }

    /// Envolvente de lo que se reproduce (misma instancia toda la sesión).
    pub fn reference(&self) -> Arc<PlaybackReference> {
        Arc::clone(&self.reference)
    }

    /// Replaces the config snapshot read by `synthesize` and `reload_model`.
    ///
    /// DECISION: Python comparte el objeto `VoiceConfig` vivo; en Rust se
    /// guarda una copia y el menú llama a esto al cambiar los ajustes.
    pub fn set_config(&self, cfg: VoiceConfig) {
        *self.cfg.write() = cfg;
    }

    /// `set_voice`: resuelve el estilo (admite `a+b`) y lo cambia.
    pub fn set_voice(&self, voice: &str) -> Result<(), SpeakerError> {
        let mut inner = self.inner.lock();
        let style = speaker_style(&inner.kokoro, voice)?;
        inner.style = style;
        Ok(())
    }

    /// `reload_model`: recarga Kokoro si el menú cambia la variante o las rutas.
    pub fn reload_model(&self) -> Result<(), SpeakerError> {
        // Carga fuera del lock: la síntesis en curso termina con el modelo y la
        // voz anteriores, y el intercambio es atómico entre ambos.
        let cfg = self.config();
        let kokoro = load_kokoro(&cfg)?;
        let style = speaker_style(&kokoro, &text_or(&cfg, "voice", "em_alex"))?;
        {
            let _synth = self.synth_lock.lock();
            let mut inner = self.inner.lock();
            inner.kokoro = kokoro;
            inner.style = style;
        }
        Ok(())
    }

    /// `close`: `PyAudio.terminate`; cpal abre un stream por frase, así que no
    /// hay nada persistente que cerrar.
    pub fn close(&self) {}

    /// `synthesize`: normaliza el texto y sintetiza con la voz y los ajustes.
    pub fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32), SpeakerError> {
        let normalized = normalize_for_kokoro(text);
        let cfg = self.config();
        let options = SynthesisOptions {
            speed: number_or(&cfg, "speed", 0.95),
            lang: "es",
            sentence_pause: number_or(&cfg, "tts_sentence_pause", 0.3),
            clause_pause: number_or(&cfg, "tts_clause_pause", 0.12),
            ..SynthesisOptions::default()
        };
        let _synth = self.synth_lock.lock();
        let mut inner = self.inner.lock();
        let SpeakerInner { kokoro, style } = &mut *inner;
        kokoro
            .create(&normalized, Voice::Style(style), &options)
            .map_err(SpeakerError::from)
    }

    /// `play`: reproduce; `false` si se interrumpió.
    pub fn play(
        &self,
        samples: &[f32],
        sample_rate: u32,
        stop: &AtomicBool,
        on_level: Option<&mut dyn FnMut(f64)>,
        on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, SpeakerError> {
        let _play = self.play_lock.lock();
        play_samples(
            self.sink.as_ref(),
            samples,
            sample_rate,
            stop,
            device_index(&self.config()),
            on_level,
            on_start,
            Some(self.reference.as_ref()),
        )
    }

    /// `speak`: sintetiza y reproduce.
    pub fn speak(
        &self,
        text: &str,
        stop: &AtomicBool,
        on_level: Option<&mut dyn FnMut(f64)>,
        on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, SpeakerError> {
        let (samples, rate) = self.synthesize(text)?;
        self.play(&samples, rate, stop, on_level, on_start)
    }

    fn config(&self) -> VoiceConfig {
        self.cfg.read().clone()
    }
}

fn load_kokoro(cfg: &VoiceConfig) -> Result<Kokoro, SpeakerError> {
    let (model, voices) = resolve_kokoro_files(
        &text_or(cfg, "tts_model_variant", "int8"),
        path_of(cfg, "kokoro_model_path").as_deref(),
        path_of(cfg, "kokoro_voices_path").as_deref(),
    )?;
    // `str(None)` del Python es «None»: KoKoroConfig.validate da el texto de
    // «Model file not found at None».
    let model = model.unwrap_or_else(|| "None".to_string());
    let voices = voices.unwrap_or_else(|| "None".to_string());
    Kokoro::new(&model, &voices, None, None).map_err(SpeakerError::from)
}

/// `_voice_style(kokoro, voice)`: `a+b` promedia los estilos que existan.
fn speaker_style(kokoro: &Kokoro, voice: &str) -> Result<Style, SpeakerError> {
    let names: Vec<&str> = voice
        .split('+')
        .map(py_strip)
        .filter(|name| !name.is_empty())
        .collect();
    let mut styles: Vec<Style> = names
        .iter()
        .filter_map(|name| kokoro.get_voice_style(name).ok())
        .collect();
    match styles.len() {
        0 => Err(SpeakerError(format!(
            "Ninguna voz de Kokoro válida en {}",
            repr_str(voice)
        ))),
        1 => Ok(styles.remove(0)),
        _ => mean_style(&styles),
    }
}

/// `np.mean(styles, axis=0).astype(np.float32)`.
///
/// DECISION: `Style::mean` no es público; se reimplementa con sus accesores.
fn mean_style(styles: &[Style]) -> Result<Style, SpeakerError> {
    let first = styles
        .first()
        .ok_or_else(|| SpeakerError("no styles to average".to_string()))?;
    let (rows, cols) = (first.rows(), first.cols());
    if styles
        .iter()
        .any(|style| style.rows() != rows || style.cols() != cols)
    {
        return Err(SpeakerError(
            "voice styles with different shapes".to_string(),
        ));
    }
    let mut data = vec![0.0_f32; first.data().len()];
    for style in styles {
        for (slot, value) in data.iter_mut().zip(style.data()) {
            *slot += *value;
        }
    }
    let count = styles.len() as f32;
    for value in &mut data {
        *value /= count;
    }
    Style::new(rows, cols, data).map_err(SpeakerError::from)
}

fn text_or(cfg: &VoiceConfig, key: &str, default: &str) -> String {
    cfg.get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| default.to_string())
}

fn path_of(cfg: &VoiceConfig, key: &str) -> Option<String> {
    cfg.get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

fn number_or(cfg: &VoiceConfig, key: &str, default: f64) -> f64 {
    cfg.get(key)
        .and_then(|value| value.as_f64())
        .unwrap_or(default)
}

fn device_index(cfg: &VoiceConfig) -> Option<usize> {
    cfg.get("output_device_index")
        .and_then(|value| value.as_u64())
        .and_then(|index| usize::try_from(index).ok())
}

mod cpal;

pub use self::cpal::CpalSink;

#[cfg(test)]
mod tests;

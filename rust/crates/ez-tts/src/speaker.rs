//! Port of `src/ezagent/voice/tts.py` (`Speaker`) (F4-05).
//!
//! Kokoro is loaded once (F4-04). Playback uses `cpal` instead of PyAudio:
//! each phrase opens its own output stream, writes 480-sample chunks and can be
//! interrupted by a stop flag (barge-in). Synthesis and playback are guarded by
//! separate locks so the next sentence can be synthesized while the current one
//! is sounding.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
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
    fn finish(&mut self, stop: &AtomicBool) -> Result<bool, String>;
}

/// `Speaker._play`: clip, chunk, report levels and stop on barge-in.
///
/// Kept apart from [`Speaker`] so tests can use a fake [`AudioSink`] without a
/// model or a sound card.
pub fn play_samples(
    sink: &dyn AudioSink,
    samples: &[f32],
    sample_rate: u32,
    stop: &AtomicBool,
    device_index: Option<usize>,
    mut on_level: Option<&mut dyn FnMut(f64)>,
    mut on_start: Option<&mut dyn FnMut()>,
) -> Result<bool, SpeakerError> {
    let mut stream = sink.open(sample_rate, device_index)?;
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
    stream.finish(stop).map_err(SpeakerError::from)
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
        })
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

/// `pyaudio.PyAudio` + `open(output_device_index=...)` en cpal.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpalSink;

impl CpalSink {
    fn device(&self, index: Option<usize>) -> Result<cpal::Device, String> {
        let host = cpal::default_host();
        match index {
            Some(index) => host
                .devices()
                .map_err(|error| error.to_string())?
                .nth(index)
                .ok_or_else(|| format!("No hay ningún dispositivo de salida con índice {index}.")),
            None => host
                .default_output_device()
                .ok_or_else(|| "No hay ningún dispositivo de salida disponible.".to_string()),
        }
    }
}

impl AudioSink for CpalSink {
    fn open(&self, rate: u32, device_index: Option<usize>) -> Result<Box<dyn AudioStream>, String> {
        let device = self.device(device_index)?;
        let (config, format) = output_config(&device, rate)?;
        let device_rate = config.sample_rate;
        let channels = usize::from(config.channels);
        let (sender, receiver) = crossbeam_channel::bounded::<i16>(4_096);
        let pending = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&pending);
        let stream = device
            .build_output_stream_raw(
                config,
                format,
                move |data, _| fill_output(data, format, channels, &receiver, &counter),
                |error| tracing::warn!("Audio: {error}"),
                Some(Duration::from_secs(2)),
            )
            .map_err(|error| error.to_string())?;
        stream.play().map_err(|error| error.to_string())?;
        Ok(Box::new(CpalStream {
            source_rate: rate,
            device_rate,
            buffered: Vec::new(),
            sender: Some(sender),
            pending,
            stream: Some(stream),
            played: false,
        }))
    }
}

/// Output configuration: the phrase rate if the device supports it, otherwise
/// the default one (the samples are resampled before playing).
fn output_config(
    device: &cpal::Device,
    rate: u32,
) -> Result<(cpal::StreamConfig, cpal::SampleFormat), String> {
    if let Ok(ranges) = device.supported_output_configs() {
        for range in ranges {
            if range.contains_rate(rate) && supported_format(range.sample_format()) {
                return Ok((range.with_sample_rate(rate).config(), range.sample_format()));
            }
        }
    }
    let default = device
        .default_output_config()
        .map_err(|error| error.to_string())?;
    if !supported_format(default.sample_format()) {
        return Err(format!(
            "Formato de salida no soportado: {:?}",
            default.sample_format()
        ));
    }
    Ok((default.config(), default.sample_format()))
}

fn supported_format(format: cpal::SampleFormat) -> bool {
    matches!(
        format,
        cpal::SampleFormat::F32
            | cpal::SampleFormat::I16
            | cpal::SampleFormat::I32
            | cpal::SampleFormat::U16
    )
}

struct CpalStream {
    source_rate: u32,
    device_rate: u32,
    buffered: Vec<i16>,
    sender: Option<crossbeam_channel::Sender<i16>>,
    pending: Arc<AtomicUsize>,
    stream: Option<cpal::Stream>,
    played: bool,
}

impl AudioStream for CpalStream {
    fn write(&mut self, samples: &[i16]) -> Result<(), String> {
        // shortcut: `write` acumula y `finish` reproduce; el HUD recibe los
        // niveles en ráfaga antes del sonido (PyAudio los daba en tiempo real).
        // Medir en hardware si hace falta; entonces el sink debe enviar aquí.
        self.buffered.extend_from_slice(samples);
        Ok(())
    }

    fn finish(&mut self, stop: &AtomicBool) -> Result<bool, String> {
        if self.played {
            return Ok(true);
        }
        self.played = true;
        let samples = resample_pcm(&self.buffered, self.source_rate, self.device_rate);
        let Some(sender) = self.sender.take() else {
            return Ok(false);
        };
        for sample in samples {
            loop {
                if stop.load(Ordering::Relaxed) {
                    return Ok(false);
                }
                self.pending.fetch_add(1, Ordering::Relaxed);
                match sender.send_timeout(sample, Duration::from_millis(50)) {
                    Ok(()) => break,
                    Err(crossbeam_channel::SendTimeoutError::Timeout(_)) => {
                        self.pending.fetch_sub(1, Ordering::Relaxed);
                    }
                    Err(crossbeam_channel::SendTimeoutError::Disconnected(_)) => {
                        self.pending.fetch_sub(1, Ordering::Relaxed);
                        return Ok(false);
                    }
                }
            }
        }
        while self.pending.load(Ordering::Relaxed) > 0 {
            if stop.load(Ordering::Relaxed) {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        // Deja que el dispositivo renderice el último buffer antes de cerrar.
        std::thread::sleep(Duration::from_millis(50));
        self.stream = None;
        Ok(true)
    }
}

impl Drop for CpalStream {
    fn drop(&mut self) {
        self.sender = None;
        self.stream = None;
    }
}

/// Remuestreo lineal solo si el dispositivo no acepta la frecuencia de Kokoro.
fn resample_pcm(samples: &[i16], source_rate: u32, device_rate: u32) -> Vec<i16> {
    if source_rate == device_rate || source_rate == 0 || device_rate == 0 || samples.is_empty() {
        return samples.to_vec();
    }
    let source: Vec<f32> = samples
        .iter()
        .map(|sample| f32::from(*sample) / 32768.0)
        .collect();
    ez_audio::resample::resample_linear(&source, source_rate, device_rate)
        .into_iter()
        .map(|value| (value as f32 * 32768.0).clamp(-32768.0, 32767.0) as i16)
        .collect()
}

fn fill_output(
    data: &mut cpal::Data,
    format: cpal::SampleFormat,
    channels: usize,
    receiver: &crossbeam_channel::Receiver<i16>,
    pending: &AtomicUsize,
) {
    match format {
        cpal::SampleFormat::F32 => write_frames(
            data.as_slice_mut::<f32>().unwrap_or_default(),
            channels,
            receiver,
            pending,
            |sample| f32::from(sample) / 32768.0,
        ),
        cpal::SampleFormat::I16 => write_frames(
            data.as_slice_mut::<i16>().unwrap_or_default(),
            channels,
            receiver,
            pending,
            |sample| sample,
        ),
        cpal::SampleFormat::I32 => write_frames(
            data.as_slice_mut::<i32>().unwrap_or_default(),
            channels,
            receiver,
            pending,
            |sample| i32::from(sample) << 16,
        ),
        cpal::SampleFormat::U16 => write_frames(
            data.as_slice_mut::<u16>().unwrap_or_default(),
            channels,
            receiver,
            pending,
            |sample| (i32::from(sample) + 32_768) as u16,
        ),
        _ => {}
    }
}

/// Duplica cada muestra mono en todos los canales del dispositivo.
fn write_frames<T: Copy>(
    output: &mut [T],
    channels: usize,
    receiver: &crossbeam_channel::Receiver<i16>,
    pending: &AtomicUsize,
    convert: impl Fn(i16) -> T,
) {
    if channels == 0 {
        return;
    }
    for frame in output.chunks_mut(channels) {
        let next = receiver.try_recv().ok();
        if next.is_some() {
            pending.fetch_sub(1, Ordering::Relaxed);
        }
        let value = convert(next.unwrap_or(0));
        for slot in frame {
            *slot = value;
        }
    }
}

#[cfg(test)]
mod tests;

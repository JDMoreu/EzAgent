//! Micrófono de mentira (`ReplayMic` de `parity/replay/run_py.py`).
//!
//! Sustituye la captura del dispositivo por una cola de segmentos WAV, silencio
//! o ruido, entregados a 30 ms por bloque con el `Segmenter` y el
//! `BargeInDetector` reales (`CapturePipeline` de `ez-audio`). El hilo se llama
//! `ezagent-call` y solo publica intenciones en la cola de la app.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ez_audio::capture::{pcm_rms, CaptureEvent, CapturePipeline, VadSettings};
use parking_lot::Mutex;
use serde_json::json;

use crate::app::core::{AppEvent, VoiceApp};

use super::trace::Trace;

/// Frecuencia de los WAV del arnés.
pub const SAMPLE_RATE: u32 = 16_000;
/// Muestras por bloque: 30 ms a 16 kHz.
pub const CHUNK_SAMPLES: usize = 480;
/// Segundos de cada bloque.
pub const CHUNK_SECS: f64 = 0.03;

/// Cola de segmentos y bucle de captura simulado.
pub struct ReplayMic {
    trace: Arc<Trace>,
    audio_dir: PathBuf,
    settings: VadSettings,
    segments: Mutex<VecDeque<Vec<i16>>>,
    buffer: Mutex<Vec<i16>>,
    speech_active: AtomicBool,
    stop: Arc<AtomicBool>,
}

impl ReplayMic {
    /// Micrófono con los ajustes del VAD del escenario.
    pub fn new(trace: Arc<Trace>, audio_dir: PathBuf, settings: VadSettings) -> Arc<Self> {
        Arc::new(Self {
            trace,
            audio_dir,
            settings,
            segments: Mutex::new(VecDeque::new()),
            buffer: Mutex::new(Vec::new()),
            speech_active: AtomicBool::new(false),
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Encola un WAV del arnés, con ganancia opcional.
    pub fn push_audio(&self, name: &str, gain: f64) -> Result<(), String> {
        let samples = read_wav(&self.audio_dir.join(name))?;
        let samples = if (gain - 1.0).abs() < f64::EPSILON {
            samples
        } else {
            samples
                .into_iter()
                .map(|sample| (f64::from(sample) * gain).clamp(-32768.0, 32767.0) as i16)
                .collect()
        };
        self.segments.lock().push_back(samples);
        Ok(())
    }

    /// Encola silencio explícito.
    pub fn push_silence(&self, seconds: f64) {
        let count = (seconds * f64::from(SAMPLE_RATE)) as usize;
        self.segments.lock().push_back(vec![0; count]);
    }

    /// Encola `ruido.wav` repetido hasta cubrir los segundos.
    pub fn push_noise(&self, seconds: f64) {
        let count = (seconds * f64::from(SAMPLE_RATE)) as usize;
        match read_wav(&self.audio_dir.join("ruido.wav")) {
            Ok(noise) if !noise.is_empty() => {
                let mut samples = Vec::with_capacity(count);
                while samples.len() < count {
                    let take = (count - samples.len()).min(noise.len());
                    samples.extend_from_slice(&noise[..take]);
                }
                self.segments.lock().push_back(samples);
            }
            _ => self.push_silence(seconds),
        }
    }

    /// `idle()`: sin audio pendiente y sin voz en curso.
    pub fn idle(&self) -> bool {
        self.buffer.lock().is_empty()
            && self.segments.lock().is_empty()
            && !self.speech_active.load(Ordering::SeqCst)
    }

    /// Estados del micrófono para el mensaje de `wait_quiet`.
    pub fn diagnostic(&self) -> String {
        format!(
            "buffer={} queued={} speech={}",
            self.buffer.lock().len(),
            self.segments.lock().len(),
            self.speech_active.load(Ordering::SeqCst)
        )
    }

    /// Pide terminar el bucle de captura.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    /// Arranca el hilo `ezagent-call` que alimenta a la app.
    pub fn spawn(self: &Arc<Self>, app: VoiceApp) -> std::io::Result<std::thread::JoinHandle<()>> {
        let mic = Arc::clone(self);
        std::thread::Builder::new()
            .name("ezagent-call".to_string())
            .spawn(move || mic.stream(app))
    }

    /// Bucle de captura: bloque a bloque, como el micrófono real.
    pub fn stream(&self, app: VoiceApp) {
        let mut pipeline = CapturePipeline::new(&self.settings, SAMPLE_RATE, None);
        let mut latched = false;
        while !self.stop.load(Ordering::SeqCst) {
            let chunk = self.next_chunk();
            let level = pcm_rms(&chunk);
            let reset = app.inner.audio.reset_capture.is_set();
            if reset {
                app.inner.audio.reset_capture.clear();
            }
            let speaking = app.is_speaking();
            let allow_barge = speaking && !latched;
            let events = pipeline.feed(&self.settings, &chunk, level, allow_barge, reset);
            app.on_mic_level(level);
            for event in events {
                self.emit(&app, &pipeline, &mut latched, event);
            }
            if !speaking {
                latched = false;
            }
        }
    }

    /// Traduce un evento del pipeline a la traza y a la cola de la app.
    fn emit(
        &self,
        app: &VoiceApp,
        pipeline: &CapturePipeline,
        latched: &mut bool,
        event: CaptureEvent,
    ) {
        match event {
            CaptureEvent::BargeIn(level) => {
                self.trace.record("vad.barge_in", json!({}));
                *latched = true;
                app.events().put(AppEvent::AudioBargeIn(level));
            }
            CaptureEvent::SpeechStart(level) => {
                self.speech_active.store(true, Ordering::SeqCst);
                self.trace.record("vad.speech_start", json!({}));
                app.events().put(AppEvent::AudioSpeechStart(level));
            }
            CaptureEvent::Utterance(pcm) => {
                self.speech_active.store(false, Ordering::SeqCst);
                if pcm.is_empty() {
                    self.trace.record("vad.discarded", json!({}));
                    app.events()
                        .put(AppEvent::AudioUtterance(Vec::new(), SAMPLE_RATE));
                } else {
                    let reason = pipeline.segmenter.last_close_reason.clone();
                    self.trace
                        .record("vad.utterance", json!({"reason": reason}));
                    app.inner.audio.mic.lock().last_speech_end = pipeline.last_speech_end;
                    app.events().put(AppEvent::AudioUtterance(pcm, SAMPLE_RATE));
                }
            }
        }
    }

    /// Siguiente bloque de 30 ms; si no hay cola, entrega silencio.
    fn next_chunk(&self) -> Vec<u8> {
        std::thread::sleep(Duration::from_secs_f64(CHUNK_SECS));
        let mut buffer = self.buffer.lock();
        if buffer.is_empty() {
            match self.segments.lock().pop_front() {
                Some(segment) => *buffer = segment,
                None => return vec![0_u8; CHUNK_SAMPLES * 2],
            }
        }
        let take = buffer.len().min(CHUNK_SAMPLES);
        let mut chunk: Vec<i16> = buffer.drain(..take).collect();
        chunk.resize(CHUNK_SAMPLES, 0);
        chunk.into_iter().flat_map(i16::to_le_bytes).collect()
    }
}

/// WAV PCM16 mono 16 kHz → muestras (el `wave.open` del Python).
fn read_wav(path: &Path) -> Result<Vec<i16>, String> {
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let bytes = std::fs::read(path).map_err(|error| format!("{name}: {error}"))?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(format!("{name}: se esperaba PCM16 mono {SAMPLE_RATE} Hz"));
    }
    let mut format_ok = false;
    let mut data: &[u8] = &[];
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| format!("{name}: WAV truncado"))?,
        ) as usize;
        let body = offset + 8;
        let end = (body + size).min(bytes.len());
        if id == b"fmt " && size >= 16 {
            let channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
            let rate = u32::from_le_bytes(
                bytes[body + 4..body + 8]
                    .try_into()
                    .map_err(|_| format!("{name}: WAV truncado"))?,
            );
            let bits = u16::from_le_bytes([bytes[body + 14], bytes[body + 15]]);
            format_ok = channels == 1 && rate == SAMPLE_RATE && bits == 16;
        } else if id == b"data" {
            data = &bytes[body..end];
        }
        offset = end + (size % 2);
    }
    if !format_ok {
        return Err(format!("{name}: se esperaba PCM16 mono {SAMPLE_RATE} Hz"));
    }
    Ok(data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| i16::from_le_bytes(*pair))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn audio_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../parity/replay/audio")
    }

    #[test]
    fn reads_the_replay_wav_and_queues_blocks() {
        let trace = Trace::new(Path::new("/tmp/x"));
        let mic = ReplayMic::new(Arc::clone(&trace), audio_dir(), VadSettings::default());
        mic.push_audio("hola_como_estas.wav", 1.0).expect("WAV");
        assert!(!mic.idle());
        let mut voiced = false;
        for _ in 0..100 {
            let chunk = mic.next_chunk();
            assert_eq!(chunk.len(), CHUNK_SAMPLES * 2);
            voiced |= chunk.iter().any(|byte| *byte != 0);
            if voiced {
                break;
            }
        }
        assert!(voiced, "el WAV tiene voz en los primeros 3 s");
        mic.push_silence(0.06);
        let mut seen = 0;
        while !mic.idle() {
            mic.next_chunk();
            seen += 1;
            assert!(seen < 100);
        }
        assert!(mic.idle());
    }

    #[test]
    fn rejects_a_stereo_wav() {
        let error = read_wav(&audio_dir().join("no-existe.wav")).expect_err("falta");
        assert!(error.contains("no-existe.wav"));
    }
}

//! Altavoz de mentira (`ReplaySpeaker` de `parity/replay/run_py.py`).
//!
//! No reproduce audio: anota la frase, simula su duración
//! `max(min_secs, 0.045·len(texto))` y respeta el `stop` igual que
//! `Speaker.play`. El texto viaja dentro de las muestras (un valor por byte
//! UTF-8), porque `ez-tts` sintetiza y reproduce en dos hilos distintos.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ez_core::metrics::monotonic;
use parking_lot::Mutex;
use serde_json::json;

use crate::app::core::SpeakerApi;

use super::trace::Trace;

/// Frecuencia simulada de las muestras (la del WAV de entrada).
pub const SAMPLE_RATE: u32 = 16_000;

/// Altavoz que anota frases y cortes.
pub struct ReplaySpeaker {
    trace: Arc<Trace>,
    min_secs: f64,
    /// `degrade: [kokoro]`: sintetizar y hablar fallan como el doble del Python.
    degraded: bool,
    playing: AtomicUsize,
    lock: Mutex<()>,
    finished: Mutex<Vec<String>>,
}

impl ReplaySpeaker {
    /// Altavoz con el mínimo de reproducción por frase del escenario.
    pub fn new(trace: Arc<Trace>, min_secs: f64) -> Arc<Self> {
        Self::build(trace, min_secs, false)
    }

    /// Altavoz sin modelo de Kokoro: cada intento anota `degraded.kokoro`.
    pub fn degraded(trace: Arc<Trace>, min_secs: f64) -> Arc<Self> {
        Self::build(trace, min_secs, true)
    }

    fn build(trace: Arc<Trace>, min_secs: f64, degraded: bool) -> Arc<Self> {
        Arc::new(Self {
            trace,
            min_secs,
            degraded,
            playing: AtomicUsize::new(0),
            lock: Mutex::new(()),
            finished: Mutex::new(Vec::new()),
        })
    }

    /// Motivo del fallo del doble (`_break_speaker` del Python).
    const BROKEN: &'static str = "No encuentro el modelo de Kokoro; revisa la voz o la ruta.";

    fn broken(&self) -> Result<(), String> {
        self.trace.record("degraded.kokoro", json!({}));
        Err(Self::BROKEN.to_string())
    }

    /// ¿Ya se reprodujo una frase que contiene `needle`?
    pub fn finished(&self, needle: &str) -> bool {
        self.finished
            .lock()
            .iter()
            .any(|text| text.contains(needle))
    }

    /// ¿Hay una frase sonando ahora mismo?
    pub fn busy(&self) -> bool {
        self.playing.load(Ordering::SeqCst) > 0
    }

    /// Envuelve el texto en las muestras simuladas.
    fn encode(text: &str) -> Vec<f32> {
        text.as_bytes()
            .iter()
            .map(|byte| f32::from(*byte))
            .collect()
    }

    /// Recupera el texto de las muestras (lo que devolvió `synthesize`).
    fn decode(samples: &[f32]) -> String {
        let bytes: Vec<u8> = samples
            .iter()
            .map(|value| (*value).clamp(0.0, 255.0) as u8)
            .collect();
        String::from_utf8(bytes).unwrap_or_default()
    }
}

impl SpeakerApi for ReplaySpeaker {
    fn synthesize(&self, text: &str) -> Result<(Vec<f32>, u32), String> {
        if self.degraded {
            return self.broken().map(|()| (Vec::new(), SAMPLE_RATE));
        }
        Ok((Self::encode(text), SAMPLE_RATE))
    }

    fn play(
        &self,
        samples: &[f32],
        _sample_rate: u32,
        stop: &AtomicBool,
        mut on_level: Option<&mut dyn FnMut(f64)>,
        mut on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        let text = Self::decode(samples);
        let seconds = self.min_secs.max(0.045 * text.chars().count() as f64);
        let _guard = self.lock.lock();
        if stop.load(Ordering::SeqCst) {
            return Ok(false);
        }
        self.playing.fetch_add(1, Ordering::SeqCst);
        self.trace.record("speech.say", json!({"text": text}));
        if let Some(start) = on_start.as_mut() {
            start();
        }
        let end = monotonic() + seconds;
        while monotonic() < end {
            if stop.load(Ordering::SeqCst) {
                self.trace.record("speech.cut", json!({"text": text}));
                self.playing.fetch_sub(1, Ordering::SeqCst);
                return Ok(false);
            }
            if let Some(level) = on_level.as_mut() {
                level(0.01);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.finished.lock().push(text);
        self.playing.fetch_sub(1, Ordering::SeqCst);
        Ok(true)
    }

    fn speak(
        &self,
        text: &str,
        stop: &AtomicBool,
        on_level: Option<&mut dyn FnMut(f64)>,
        on_start: Option<&mut dyn FnMut()>,
    ) -> Result<bool, String> {
        if self.degraded {
            return self.broken().map(|()| false);
        }
        let (samples, rate) = self.synthesize(text)?;
        self.play(&samples, rate, stop, on_level, on_start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_survives_the_sample_roundtrip() {
        let text = "Claro, aquí estoy. ¿Confirmas?";
        assert_eq!(ReplaySpeaker::decode(&ReplaySpeaker::encode(text)), text);
    }

    #[test]
    fn a_stopped_phrase_is_cut_and_not_finished() {
        let trace = Trace::new(std::path::Path::new("/tmp/x"));
        let speaker = ReplaySpeaker::new(Arc::clone(&trace), 0.25);
        let stop = Arc::new(AtomicBool::new(false));
        let stopper = Arc::clone(&stop);
        let thread_speaker = Arc::clone(&speaker);
        let handle = std::thread::spawn(move || {
            let mut none = |_: f64| {};
            thread_speaker.speak("Hola.", &stopper, Some(&mut none), None)
        });
        while !speaker.busy() {
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(30));
        stop.store(true, Ordering::SeqCst);
        let result = handle.join().expect("hilo sin pánico").expect("sin error");
        assert!(!result);
        assert!(!speaker.finished("Hola."));
        let events: Vec<String> = trace
            .lines()
            .iter()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter_map(|item| {
                item.get("event")
                    .and_then(|e| e.as_str())
                    .map(str::to_string)
            })
            .collect();
        assert_eq!(events, vec!["speech.say", "speech.cut"]);
    }
}

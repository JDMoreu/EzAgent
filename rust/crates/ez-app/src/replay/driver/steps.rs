//! Tabla de pasos del guion (`Driver._apply` de `run_py.py`).
//!
//! Cada manejador reconoce su clave y devuelve la espera. Añadir un paso de
//! F7-01b/F7-01c es añadirlo a [`handlers`]; el bucle del [`super::Driver`] no
//! cambia.

use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use super::{describe_first, flag, ready, text, Driver, Pending};

/// Un paso del YAML.
pub trait Step: Send + Sync {
    /// ¿Este paso es mío?
    fn matches(&self, step: &Map<String, Value>) -> bool;
    /// Texto de `driver.step` (`_describe` del Python).
    fn describe(&self, step: &Map<String, Value>) -> String;
    /// Aplica el paso y deja la espera (o falla con un mensaje claro).
    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String>;
}

/// Todos los pasos soportados, en el orden de comprobación del Python.
pub fn handlers() -> Vec<Box<dyn Step>> {
    vec![
        Box::new(Audio),
        Box::new(Silence),
        Box::new(Noise),
        Box::new(Hotkey),
        Box::new(WaitState),
        Box::new(WaitRequests),
        Box::new(WaitQuiet),
        Box::new(WaitSpeech),
        Box::new(WaitTranscript),
        Box::new(Sleep),
    ]
}

fn seconds(step: &Map<String, Value>, key: &str) -> f64 {
    step.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn integer(step: &Map<String, Value>, key: &str) -> i64 {
    step.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// `audio: archivo.wav` con `gain:` opcional.
struct Audio;

impl Step for Audio {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        flag(step, "audio")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        format!("audio:{}", text(step, "audio"))
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let gain = step.get("gain").and_then(Value::as_f64).unwrap_or(1.0);
        driver.mic.push_audio(&text(step, "audio"), gain)?;
        Ok(ready())
    }
}

/// `silence: segundos`.
struct Silence;

impl Step for Silence {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.contains_key("silence")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "silence")
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        driver.mic.push_silence(seconds(step, "silence"));
        Ok(ready())
    }
}

/// `noise: segundos` con `ruido.wav`.
struct Noise;

impl Step for Noise {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.contains_key("noise")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "noise")
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        driver.mic.push_noise(seconds(step, "noise"));
        Ok(ready())
    }
}

/// `hotkey: toggle` publica un evento de atajo global.
struct Hotkey;

impl Step for Hotkey {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        flag(step, "hotkey")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "hotkey")
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        driver.app.hotkey_events().put(text(step, "hotkey"));
        Ok(ready())
    }
}

/// `wait_state: listening`.
struct WaitState;

impl Step for WaitState {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        flag(step, "wait_state")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "wait_state")
    }

    fn apply(
        &self,
        _driver: &Driver,
        step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        Ok(Some(Pending::State(text(step, "wait_state"))))
    }
}

/// `wait_requests: 1`.
struct WaitRequests;

impl Step for WaitRequests {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.contains_key("wait_requests")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "wait_requests")
    }

    fn apply(
        &self,
        _driver: &Driver,
        step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        Ok(Some(Pending::Requests(
            usize::try_from(integer(step, "wait_requests")).unwrap_or(0),
        )))
    }
}

/// `wait_quiet: 30`.
struct WaitQuiet;

impl Step for WaitQuiet {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.contains_key("wait_quiet")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "wait_quiet")
    }

    fn apply(
        &self,
        _driver: &Driver,
        step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        let timeout = seconds(step, "wait_quiet");
        Ok(Some(Pending::Quiet {
            deadline: Instant::now() + Duration::from_secs_f64(timeout.max(0.0)),
            since: None,
        }))
    }
}

/// `wait_speech: "¿Confirmas?"`.
struct WaitSpeech;

impl Step for WaitSpeech {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        flag(step, "wait_speech")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "wait_speech")
    }

    fn apply(
        &self,
        _driver: &Driver,
        step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        Ok(Some(Pending::Speech(text(step, "wait_speech"))))
    }
}

/// `wait_transcript: "Dale."`.
struct WaitTranscript;

impl Step for WaitTranscript {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        flag(step, "wait_transcript")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "wait_transcript")
    }

    fn apply(
        &self,
        _driver: &Driver,
        step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        Ok(Some(Pending::Transcript(text(step, "wait_transcript"))))
    }
}

/// `sleep: 0.5`.
struct Sleep;

impl Step for Sleep {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.contains_key("sleep")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "sleep")
    }

    fn apply(
        &self,
        _driver: &Driver,
        step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        Ok(Some(Pending::Sleep(
            Instant::now() + Duration::from_secs_f64(seconds(step, "sleep").max(0.0)),
        )))
    }
}

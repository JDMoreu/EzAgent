//! Reloj, hospedador y valores de los textos de los pasos.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use ez_core::pyvalue::text_or_empty;
use regex::Regex;
use serde_json::{Map, Value};

use crate::herdr::{Herdr, HerdrError};

use super::text::compiled;

/// Reloj y pausa de la sesión (Python usa `time.monotonic` y `time.sleep`).
pub(crate) trait SessionClock: Send + Sync {
    fn now(&self) -> f64;
    fn sleep(&self, seconds: f64);
}

pub(super) struct RealClock;

impl SessionClock for RealClock {
    fn now(&self) -> f64 {
        ez_core::metrics::monotonic()
    }

    fn sleep(&self, seconds: f64) {
        std::thread::sleep(Duration::from_secs_f64(seconds));
    }
}

/// Lo que la sesión pide del objeto que la hospeda (`self.get_agent`, ...).
pub trait SessionHost {
    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, HerdrError>;
    fn send_agent(
        &self,
        address: &str,
        text: &str,
        confirmed: bool,
    ) -> Result<Map<String, Value>, HerdrError>;
    fn keys_agent(&self, address: &str, keys: &[String]) -> Result<Map<String, Value>, HerdrError>;
    fn read_agent(&self, address: &str, lines: Option<i64>) -> Result<String, HerdrError>;
}

impl<T: SessionHost + ?Sized> SessionHost for Arc<T> {
    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, HerdrError> {
        (**self).get_agent(address)
    }

    fn send_agent(
        &self,
        address: &str,
        text: &str,
        confirmed: bool,
    ) -> Result<Map<String, Value>, HerdrError> {
        (**self).send_agent(address, text, confirmed)
    }

    fn keys_agent(&self, address: &str, keys: &[String]) -> Result<Map<String, Value>, HerdrError> {
        (**self).keys_agent(address, keys)
    }

    fn read_agent(&self, address: &str, lines: Option<i64>) -> Result<String, HerdrError> {
        (**self).read_agent(address, lines)
    }
}

impl SessionHost for Herdr {
    fn get_agent(&self, address: &str) -> Result<Map<String, Value>, HerdrError> {
        Herdr::get_agent(self, address)
    }

    fn send_agent(
        &self,
        address: &str,
        text: &str,
        confirmed: bool,
    ) -> Result<Map<String, Value>, HerdrError> {
        Herdr::send_agent(self, address, text, confirmed)
    }

    fn keys_agent(&self, address: &str, keys: &[String]) -> Result<Map<String, Value>, HerdrError> {
        Herdr::keys_agent(self, address, keys)
    }

    fn read_agent(&self, address: &str, lines: Option<i64>) -> Result<String, HerdrError> {
        Herdr::read_agent(self, address, lines, 6000)
    }
}

/// Valores con los que se rellenan los textos de los pasos.
#[derive(Debug, Clone, Default)]
pub struct FillValues {
    pub model: String,
    pub effort: String,
}

impl FillValues {
    pub fn new(model: &str, effort: &str) -> Self {
        Self {
            model: model.to_string(),
            effort: effort.to_string(),
        }
    }

    /// `pending` del lanzamiento (dict[str, str] en Python).
    pub(super) fn from_pending(pending: &Map<String, Value>) -> Self {
        Self {
            model: text_or_empty(pending.get("model")),
            effort: text_or_empty(pending.get("effort")),
        }
    }

    pub(super) fn get(&self, kind: &str) -> &str {
        match kind {
            "model" => &self.model,
            _ => &self.effort,
        }
    }
}

fn separators() -> &'static Regex {
    static SEPARATORS: OnceLock<Regex> = OnceLock::new();
    compiled(&SEPARATORS, r"[-_]+")
}

fn model_search(model: &str) -> String {
    let last = model.rsplit('/').next().unwrap_or(model);
    separators().replace_all(last, " ").into_owned()
}

/// Sustituye {model}, {model_search} y {effort}; `escape` para regex.
pub fn fill(text: &str, values: &FillValues, escape: bool) -> String {
    let quote = |value: &str| {
        if escape {
            regex::escape(value)
        } else {
            value.to_string()
        }
    };
    text.replace("{model_search}", &quote(&model_search(&values.model)))
        .replace("{model}", &quote(&values.model))
        .replace("{effort}", &quote(&values.effort))
}

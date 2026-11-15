//! Pasos del guion de F7-01b: agentes, confirmaciones y drenado.
//!
//! Añade a la tabla de [`Step`] los pasos que usan los escenarios 07-15
//! (`hc_set`, `wait_hc`, `wait_confirm_settled`, `wait_drained`) con la misma
//! semántica de `Driver._apply` de `parity/replay/run_py.py`.

use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use super::driver::{describe_first, flag, ready, Driver, Pending, Step};
use super::hc_b::{describe_set, hc_set};

/// `hc_set: {name, state, conclusion}`.
pub struct HcSet;

impl Step for HcSet {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.get("hc_set").is_some_and(Value::is_object)
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        step.get("hc_set")
            .map_or_else(|| "hc_set:None:None".to_string(), describe_set)
    }

    fn apply(&self, driver: &Driver, step: &Map<String, Value>) -> Result<Option<Pending>, String> {
        let Some(spec) = step.get("hc_set") else {
            return Ok(ready());
        };
        let path = driver
            .hc_path
            .as_deref()
            .ok_or_else(|| "hc_set sin ruta de estado".to_string())?;
        hc_set(path, spec)?;
        Ok(ready())
    }
}

/// `wait_hc: 1`: terminales creadas en `fake-hc`.
pub struct WaitHc;

impl Step for WaitHc {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.contains_key("wait_hc")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "wait_hc")
    }

    fn apply(
        &self,
        _driver: &Driver,
        step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        let wanted = step.get("wait_hc").and_then(Value::as_i64).unwrap_or(0);
        Ok(Some(Pending::Hc(usize::try_from(wanted).unwrap_or(0))))
    }
}

/// `wait_confirm_settled: true`: sin confirmación pendiente y fuera de turno.
pub struct WaitConfirmSettled;

impl Step for WaitConfirmSettled {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        flag(step, "wait_confirm_settled")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "wait_confirm_settled")
    }

    fn apply(
        &self,
        _driver: &Driver,
        _step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        Ok(Some(Pending::ConfirmSettled))
    }
}

/// `wait_drained: 40`: sin eventos, avisos ni hilos de voz/turno.
pub struct WaitDrained;

impl Step for WaitDrained {
    fn matches(&self, step: &Map<String, Value>) -> bool {
        step.contains_key("wait_drained")
    }

    fn describe(&self, step: &Map<String, Value>) -> String {
        describe_first(step, "wait_drained")
    }

    fn apply(
        &self,
        _driver: &Driver,
        step: &Map<String, Value>,
    ) -> Result<Option<Pending>, String> {
        let timeout = step
            .get("wait_drained")
            .and_then(Value::as_f64)
            .filter(|value| *value != 0.0)
            .unwrap_or(20.0);
        Ok(Some(Pending::Drained {
            deadline: Instant::now() + Duration::from_secs_f64(timeout.max(0.0)),
            timeout,
            since: None,
        }))
    }
}

//! Entrega de la tarea y ajustes dentro de la sesión: resultados de
//! `send_task`, `finish_launch` y `configure_agent`.

use ez_core::pyvalue::{py_strip, python_str, truthy};
use serde_json::{Map, Value};

use crate::herdr::{error_row, value_or_text, AgentLaunch};

use super::{truthy_key, FillValues, SessionHost, READY_TIMEOUT_S};

impl<H: SessionHost> super::AgentSession<H> {
    /// Pega la tarea y confirma que el agente la recibió.
    pub fn send_task(&self, address: &str, task: &str) -> Map<String, Value> {
        let sent = self.send_raw(address, task);
        let confirmed = truthy_key(&sent, "ok") && self.confirm_submitted(address, task);
        finish_send(sent, confirmed)
    }

    /// `send_task` con la confirmación ya decidida (costura de pruebas).
    #[doc(hidden)]
    pub fn send_task_with(&self, address: &str, task: &str, confirmed: bool) -> Map<String, Value> {
        let sent = self.send_raw(address, task);
        let confirmed = truthy_key(&sent, "ok") && confirmed;
        finish_send(sent, confirmed)
    }

    fn send_raw(&self, address: &str, task: &str) -> Map<String, Value> {
        self.host()
            .send_agent(address, task, true)
            .unwrap_or_else(|error| error_row(error.message()))
    }

    /// Espera al agente, fija modelo/esfuerzo pendientes y pega la tarea.
    pub fn finish_launch(
        &self,
        address: &str,
        key: &str,
        launch: &AgentLaunch,
        task: &str,
    ) -> Map<String, Value> {
        let ready = self.wait_ready(address, READY_TIMEOUT_S);
        let blocking = ready && self.blocking_dialog(address);
        self.finish_launch_with(
            address,
            key,
            launch,
            task,
            ready,
            blocking,
            |pending| {
                let values = FillValues::from_pending(pending);
                self.apply_in_session(address, key, &values)
            },
            |task| self.send_task(address, task),
        )
    }

    /// `finish_launch` con la espera, el diálogo y los dos pasos ya resueltos.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn finish_launch_with<A, B>(
        &self,
        address: &str,
        key: &str,
        launch: &AgentLaunch,
        task: &str,
        ready: bool,
        blocking: bool,
        apply: A,
        send: B,
    ) -> Map<String, Value>
    where
        A: FnOnce(&Map<String, Value>) -> (Vec<String>, Vec<String>, String),
        B: FnOnce(&str) -> Map<String, Value>,
    {
        if !ready {
            return error_row(format!(
                "Abrí la terminal {address}, pero no vi que {key} arrancara; no le \
                 envié nada. Revísala con action=read."
            ));
        }
        if blocking {
            return error_row(format!(
                "{key} está pidiendo confirmar algo en la terminal {address} (por \
                 ejemplo, confiar en la carpeta). No pulso Enter por ti: confírmalo \
                 tú y vuelve a pedirme la tarea."
            ));
        }
        let (applied, mut warnings, fatal) = apply(&launch.pending);
        if !fatal.is_empty() {
            let mut result = Map::new();
            result.insert("ok".to_string(), Value::Bool(false));
            result.insert("applied".to_string(), strings(applied.clone()));
            result.insert(
                "error".to_string(),
                Value::String(format!(
                    "No pude fijar el {fatal}. Dejé {key} abierto en {address} sin \
                     enviar la tarea; revísalo con action=read."
                )),
            );
            return result;
        }
        let mut task_sent = false;
        if !task.is_empty() {
            let sent = send(task);
            task_sent = truthy_key(&sent, "ok");
            if !task_sent {
                warnings.push(format!(
                    "No pude enviar la tarea: {}",
                    python_str(sent.get("error").unwrap_or(&Value::Null))
                ));
            } else if let Some(warning) = sent.get("warning").filter(|value| truthy(value)) {
                warnings.push(python_str(warning));
            }
        }
        let mut result = Map::new();
        result.insert("ok".to_string(), Value::Bool(true));
        result.insert("applied".to_string(), strings(applied.clone()));
        let all: Vec<String> = launch.warnings.iter().cloned().chain(warnings).collect();
        result.insert("warnings".to_string(), strings(all));
        result.insert("task_sent".to_string(), Value::Bool(task_sent));
        result
    }

    /// Cambia modelo/esfuerzo de un agente ya abierto con sus comandos.
    pub fn configure_agent(
        &self,
        address: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Map<String, Value> {
        match self.prepare_configure(address, model, effort) {
            Ok((key, values)) => {
                let outcome = self.apply_in_session(address, &key, &values);
                configure_result(address, &key, outcome)
            }
            Err(row) => row,
        }
    }

    /// `configure_agent` con el resultado de aplicar ya resuelto (costura).
    #[doc(hidden)]
    pub fn configure_agent_with(
        &self,
        address: &str,
        model: Option<&str>,
        effort: Option<&str>,
        apply: (Vec<String>, Vec<String>, String),
    ) -> Map<String, Value> {
        match self.prepare_configure(address, model, effort) {
            Ok((key, _values)) => configure_result(address, &key, apply),
            Err(row) => row,
        }
    }

    fn prepare_configure(
        &self,
        address: &str,
        model: Option<&str>,
        effort: Option<&str>,
    ) -> Result<(String, FillValues), Map<String, Value>> {
        let values = FillValues::new(
            py_strip(model.unwrap_or("")),
            py_strip(effort.unwrap_or("")),
        );
        if values.model.is_empty() && values.effort.is_empty() {
            return Err(error_row("Indica el modelo o el esfuerzo."));
        }
        let info = match self.host().get_agent(address) {
            // DECISION: el Python dejaría escapar la excepción; aquí un fallo
            // del host es un resultado más (`ok: false`).
            Ok(info) => info,
            Err(error) => return Err(error_row(error.message())),
        };
        let raw = {
            let agent = value_or_text(info.get("agent"), "");
            if agent.is_empty() {
                value_or_text(info.get("app"), "")
            } else {
                agent
            }
        };
        let Some(key) = crate::driver::resolve_agent_in(self.config(), &raw) else {
            let name = if raw.is_empty() {
                "sin nombre".to_string()
            } else {
                raw
            };
            return Err(error_row(format!(
                "No sé qué agente es {address} ({name}); no puedo cambiarle el modelo."
            )));
        };
        for (kind, label) in [("model", "modelo"), ("effort", "esfuerzo")] {
            let value = values.get(kind);
            if !value.is_empty() {
                let problem = crate::driver::check_value(self.config(), &key, label, value);
                if !problem.is_empty() {
                    return Err(error_row(problem));
                }
            }
        }
        Ok((key, values))
    }
}

/// Arma el resultado de `configure_agent` a partir de lo aplicado.
fn configure_result(
    address: &str,
    key: &str,
    (applied, warnings, fatal): (Vec<String>, Vec<String>, String),
) -> Map<String, Value> {
    if !fatal.is_empty() {
        return error_row(format!("No pude fijar el {fatal}."));
    }
    if applied.is_empty() {
        let message = if warnings.is_empty() {
            "Nada que aplicar.".to_string()
        } else {
            warnings.join(" ")
        };
        return error_row(message);
    }
    let mut result = Map::new();
    result.insert("ok".to_string(), Value::Bool(true));
    result.insert("address".to_string(), Value::String(address.to_string()));
    result.insert("agent".to_string(), Value::String(key.to_string()));
    result.insert("applied".to_string(), strings(applied));
    result.insert("warnings".to_string(), strings(warnings));
    result
}

/// El aviso de `send_task` cuando no se pudo confirmar la entrega.
fn finish_send(mut sent: Map<String, Value>, confirmed: bool) -> Map<String, Value> {
    if truthy_key(&sent, "ok") && !confirmed {
        sent.insert(
            "warning".to_string(),
            Value::String("No pude confirmar que el agente recibió la tarea.".to_string()),
        );
    }
    sent
}

fn strings(items: impl IntoIterator<Item = String>) -> Value {
    Value::Array(items.into_iter().map(Value::String).collect())
}

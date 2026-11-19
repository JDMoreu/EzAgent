//! Delegación en un subagente de cualquier tipo (§4–§7 del diseño).
//!
//! `ez delegate` valida el tipo, el modelo y el esfuerzo contra el catálogo,
//! elige el nombre del subagente (§6), lo lanza como hijo de la terminal que
//! delega (§8) y le entrega el primer mensaje (§7). Un rechazo no crea
//! terminales ni toca configuraciones.

mod message;
mod names;
#[cfg(test)]
mod tests;

use ez_core::pyvalue::{python_str, truthy};
use serde_json::{Map, Value};

use crate::driver::Choice;

use super::{value_or_text, Herdr, HerdrError, LaunchSupport, Placement, RunAgent};

pub use message::first_message;
pub use names::{default_name, role_short, valid_name};

/// Intentos con el nombre automático antes de rendirse (§6).
const MAX_NAME_ATTEMPTS: usize = 5;

/// Tope de espera de `hc ls` (segundos).
const LS_TIMEOUT_S: f64 = 20.0;

/// La validación del catálogo que recibe `delegate`.
///
/// Es `AgentDriver::validate_choice`: tipo, modelo y esfuerzo contra el
/// catálogo; si falla, la delegación se rechaza sin crear nada.
pub type DriverCheck<'a> = &'a dyn Fn(&str, Option<&str>, Option<&str>) -> Result<Choice, String>;

/// Pedido de delegación ya analizado (el contenido del encargo, no su ruta).
#[derive(Debug, Clone, Default)]
pub struct DelegateRequest {
    /// El papel tal como se escribió (`developer`, `qa`…).
    pub role: String,
    /// El tipo pedido (`opencode`, `qwen`…).
    pub agent: String,
    /// El modelo pedido, si lo hubo.
    pub model: Option<String>,
    /// El nivel de esfuerzo pedido, si lo hubo.
    pub effort: Option<String>,
    /// El contenido del encargo (el archivo `--brief`, ya leído).
    pub brief: String,
    /// El nombre pedido; por omisión, el de §6.
    pub name: Option<String>,
    /// Dónde se ve el subagente en Herdr.
    pub show: Placement,
    /// La carpeta de trabajo; por omisión, la actual.
    pub cwd: Option<String>,
    /// La terminal que delega (el padre del subagente).
    pub parent: String,
    /// Si `hc` avisa al padre de lo que hace el subagente.
    pub notify: bool,
    /// Si el subagente sobrevive al cierre del padre.
    pub outlive_parent: bool,
}

/// Por qué falló una delegación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegateError {
    /// Rechazado: validación, nombre ocupado o fallo de `hc` (código 1).
    Rejected(String),
    /// No hay `hc` o no hay dónde delegar (código 3).
    Unavailable(String),
}

impl DelegateError {
    /// El texto para la persona.
    pub fn message(&self) -> &str {
        match self {
            Self::Rejected(message) | Self::Unavailable(message) => message,
        }
    }

    /// El código de salida de `ez delegate`.
    pub fn code(&self) -> i32 {
        match self {
            Self::Rejected(_) => 1,
            Self::Unavailable(_) => 3,
        }
    }
}

impl std::fmt::Display for DelegateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for DelegateError {}

impl Herdr {
    /// Delega en un subagente de cualquier tipo (§4–§7).
    ///
    /// `driver_check` valida el tipo, el modelo y el esfuerzo contra el
    /// catálogo (`AgentDriver::validate_choice`); si falla, no se crea nada.
    pub fn delegate<L: LaunchSupport + Clone + Send + 'static>(
        &self,
        launcher: L,
        driver_check: DriverCheck<'_>,
        request: &DelegateRequest,
    ) -> Result<Map<String, Value>, DelegateError> {
        let choice = driver_check(
            request.agent.as_str(),
            request.model.as_deref(),
            request.effort.as_deref(),
        )
        .map_err(DelegateError::Rejected)?;
        self.require_hc()?;
        let mut warnings = choice.warnings.clone();
        let mut index = match self.count_children(request) {
            Ok(count) => count + 1,
            Err(message) => {
                warnings.push(message);
                1
            }
        };
        let asked = request
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let mut attempts = 0;
        loop {
            attempts += 1;
            let name = match asked {
                Some(name) => name.to_string(),
                None => default_name(&request.role, index, &choice.key),
            };
            let result = self.launch_delegate(&launcher, request, &choice, &name)?;
            if result.get("ok").is_some_and(truthy) {
                return Ok(self.delegate_row(request, &choice, &name, &result, warnings));
            }
            // DECISION: con el nombre automático se prueba el siguiente número
            // (§6); con un nombre pedido, el choque es un rechazo y ya está.
            if asked.is_none() && is_name_taken(&result) && attempts < MAX_NAME_ATTEMPTS {
                index += 1;
                continue;
            }
            return Err(DelegateError::Rejected(error_text(&result)));
        }
    }

    /// Lanza el subagente con su padre, sus etiquetas y su primer mensaje.
    fn launch_delegate<L: LaunchSupport + Clone + Send + 'static>(
        &self,
        launcher: &L,
        request: &DelegateRequest,
        choice: &Choice,
        name: &str,
    ) -> Result<Map<String, Value>, DelegateError> {
        let task = first_message(&request.role, &request.parent, &request.brief);
        let params = RunAgent {
            agent: choice.key.as_str(),
            task: task.as_str(),
            cwd: request.cwd.as_deref(),
            name: Some(name),
            confirmed: true,
            model: choice.model.as_deref(),
            effort: choice.effort.as_deref(),
            parent: Some(request.parent.as_str()),
            hc_name: Some(name),
            labels: Some(delegate_labels(request, choice)),
            show: request.show,
            notify: request.notify,
            outlive_parent: request.outlive_parent,
            ..RunAgent::default()
        };
        self.run_agent(launcher.clone(), params)
            .map_err(|error| classify(&error))
    }

    /// `hc` tiene que estar: sin él no hay a quién delegar.
    fn require_hc(&self) -> Result<(), DelegateError> {
        let path = self.hc_path();
        if self.runtime.which(&path).is_some() {
            return Ok(());
        }
        let program = if path.is_empty() { "hc" } else { path.as_str() };
        Err(DelegateError::Unavailable(format!(
            "No encuentro `{program}`. Revisa voice.json."
        )))
    }

    /// Cuántos subagentes de este papel cuelgan ya del padre (§6).
    fn count_children(&self, request: &DelegateRequest) -> Result<usize, String> {
        let args = vec![
            "ls".to_string(),
            "--all".to_string(),
            "--json".to_string(),
            "--label".to_string(),
            format!("ez.parent={}", request.parent),
            "--label".to_string(),
            format!("ez.role={}", request.role),
        ];
        let data = self
            .json(&args, LS_TIMEOUT_S)
            .map_err(|error| error.message().to_string())?;
        Ok(count_terminals(&data))
    }

    /// El resultado que ve la persona (§4).
    fn delegate_row(
        &self,
        request: &DelegateRequest,
        choice: &Choice,
        name: &str,
        result: &Map<String, Value>,
        mut warnings: Vec<String>,
    ) -> Map<String, Value> {
        warnings.extend(warning_list(result.get("warnings")));
        let mut row = Map::new();
        row.insert("ok".to_string(), Value::Bool(true));
        row.insert(
            "terminal".to_string(),
            Value::String(value_or_text(
                result.get("terminal"),
                &value_or_text(result.get("address"), ""),
            )),
        );
        row.insert(
            "name".to_string(),
            Value::String(value_or_text(result.get("name"), name)),
        );
        row.insert("agent".to_string(), Value::String(choice.key.clone()));
        row.insert("label".to_string(), Value::String(choice.label.clone()));
        row.insert(
            "model".to_string(),
            Value::String(choice.model.clone().unwrap_or_default()),
        );
        row.insert(
            "effort".to_string(),
            Value::String(choice.effort.clone().unwrap_or_default()),
        );
        row.insert("role".to_string(), Value::String(request.role.clone()));
        row.insert(
            "parent".to_string(),
            Value::String(value_or_text(result.get("parent"), &request.parent)),
        );
        row.insert(
            "show".to_string(),
            Value::String(request.show.as_str().to_string()),
        );
        row.insert(
            "placed".to_string(),
            Value::String(value_or_text(result.get("placed"), request.show.as_str())),
        );
        row.insert(
            "task_sent".to_string(),
            Value::Bool(result.get("task_sent").is_some_and(truthy)),
        );
        row.insert(
            "warnings".to_string(),
            Value::Array(warnings.into_iter().map(Value::String).collect()),
        );
        row
    }
}

/// Las etiquetas del subagente (§3).
fn delegate_labels(request: &DelegateRequest, choice: &Choice) -> Map<String, Value> {
    let mut labels = Map::new();
    labels.insert("ez.agent".to_string(), Value::String(choice.key.clone()));
    labels.insert("ez.role".to_string(), Value::String(request.role.clone()));
    labels.insert(
        "ez.parent".to_string(),
        Value::String(request.parent.clone()),
    );
    if let Some(model) = &choice.model {
        labels.insert("ez.model".to_string(), Value::String(model.clone()));
    }
    if let Some(effort) = &choice.effort {
        labels.insert("ez.effort".to_string(), Value::String(effort.clone()));
    }
    labels
}

/// Cuántas terminales trae `hc ls --json`.
fn count_terminals(data: &Value) -> usize {
    data.get("terminals")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

/// ¿`hc` dijo que el nombre ya está ocupado?
fn is_name_taken(result: &Map<String, Value>) -> bool {
    result.get("name_taken").is_some_and(truthy)
}

/// El mensaje de error que dejó `hc`.
fn error_text(result: &Map<String, Value>) -> String {
    let message = python_str(result.get("error").unwrap_or(&Value::Null));
    if message.is_empty() {
        "No pude lanzar el subagente.".to_string()
    } else {
        message
    }
}

/// Los avisos que dejó el lanzamiento.
fn warning_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items.iter().map(python_str).collect(),
        Some(value) if truthy(value) => vec![python_str(value)],
        _ => Vec::new(),
    }
}

/// Un fallo de `hc` que no encuentra el programa no es un rechazo.
fn classify(error: &HerdrError) -> DelegateError {
    if error.message().starts_with("No encuentro") {
        DelegateError::Unavailable(error.message().to_string())
    } else {
        DelegateError::Rejected(error.message().to_string())
    }
}

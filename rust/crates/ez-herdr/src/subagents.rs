//! Registro del estado de cada subagente y textos de aviso (§13 del diseño).
//!
//! Lógica pura con archivos: no habla con `hc`, no abre Herdr y no usa la red.
//! Un registro JSON por subagente bajo `<raíz>/<padre>/<nombre>.json`, con
//! escritura atómica y un candado por nombre para que dos escritores no pierdan
//! el historial de cambios. Los textos que lee una persona están en español
//! neutro y los escribe el código, nunca un agente.

use serde::{Deserialize, Serialize};

mod registry;
#[cfg(test)]
mod tests;
mod watch;

pub use registry::Registry;
pub use watch::{
    ensure_watcher, epoch_ms, watcher_args, watcher_name, DelegateState, HcOutput, HcRuntime,
    WatchConfig, WatchRuntime, WatchStop, Watcher, WatcherSetup, WatcherState, DEFAULT_GRACE_MS,
    DEFAULT_IDLE_MS, DEFAULT_INTERVAL_MS,
};

/// Estado de una tarea delegada (§13 y `novedades-subagentes.md` §2).
/// Serializa como `"en_proceso"`, `"bloqueado"`, `"terminado"`, `"fallido"`,
/// `"validado"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum TaskState {
    InProgress,
    Blocked,
    Done,
    Failed,
    Validated,
}

impl TaskState {
    /// Nombre del estado tal como se guarda y se muestra.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InProgress => "en_proceso",
            Self::Blocked => "bloqueado",
            Self::Done => "terminado",
            Self::Failed => "fallido",
            Self::Validated => "validado",
        }
    }

    /// Estado a partir de su nombre; `None` si no es un estado conocido.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "en_proceso" => Some(Self::InProgress),
            "bloqueado" => Some(Self::Blocked),
            "terminado" => Some(Self::Done),
            "fallido" => Some(Self::Failed),
            "validado" => Some(Self::Validated),
            _ => None,
        }
    }

    /// `validado` y `fallido` son estados finales (§13).
    pub fn is_final(self) -> bool {
        matches!(self, Self::Validated | Self::Failed)
    }
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<TaskState> for String {
    fn from(state: TaskState) -> Self {
        state.as_str().to_string()
    }
}

impl TryFrom<String> for TaskState {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text).ok_or_else(|| format!("estado desconocido: «{text}»"))
    }
}

/// Un cambio de estado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub state: TaskState,
    pub at_ms: u64,
    pub by: String,
    pub detail: String,
}

/// Resultado de una validación.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Validation {
    pub by: String,
    pub verdict: bool,
    pub summary: String,
    pub file: Option<String>,
    pub at_ms: u64,
}

/// Lo que se guarda de cada subagente (campos de §13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub name: String,
    pub terminal: String,
    pub parent: String,
    pub agent: String,
    pub label: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub role: String,
    pub cwd: String,
    pub brief_head: String,
    pub state: TaskState,
    pub since_ms: u64,
    pub history: Vec<Change>,
    pub summary: Option<String>,
    pub report: Option<String>,
    pub validation: Option<Validation>,
    /// Último estado del que se avisó al padre.
    pub notified: Option<TaskState>,
}

/// Fallo al leer, escribir o validar el registro de subagentes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    Io(String),
    Invalid(String),
    NotFound(String),
    Exists(String),
    Busy(String),
}

impl RegistryError {
    pub fn message(&self) -> &str {
        match self {
            Self::Io(text)
            | Self::Invalid(text)
            | Self::NotFound(text)
            | Self::Exists(text)
            | Self::Busy(text) => text,
        }
    }
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for RegistryError {}

/// Carpeta de un padre: dirección con `/` → `_`, sin `..` ni separadores.
pub(crate) fn parent_folder(parent: &str) -> Result<String, RegistryError> {
    let safe = !parent.is_empty()
        && !parent.contains("..")
        && parent
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'));
    if !safe {
        return Err(RegistryError::Invalid(format!(
            "dirección de padre inválida: «{parent}»"
        )));
    }
    Ok(parent.replace('/', "_"))
}

/// Nombre de hc (§6): `[a-z0-9][a-z0-9_.-]{0,31}` y sin forma de dirección.
pub(crate) fn validate_name(name: &str) -> Result<(), RegistryError> {
    let bytes = name.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= 32
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-')
        })
        && !looks_like_address(name);
    if !valid {
        return Err(RegistryError::Invalid(format!(
            "nombre de subagente inválido: «{name}»"
        )));
    }
    Ok(())
}

fn looks_like_address(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 2
        && bytes[0] == b't'
        && bytes[1].is_ascii_digit()
        && bytes
            .iter()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
}

pub(crate) fn transition_allowed(from: TaskState, to: TaskState) -> bool {
    matches!(
        (from, to),
        (TaskState::InProgress, TaskState::Blocked)
            | (TaskState::Blocked, TaskState::InProgress)
            | (TaskState::Blocked, TaskState::Done)
            | (TaskState::Blocked, TaskState::Failed)
            | (TaskState::InProgress, TaskState::Done)
            | (TaskState::InProgress, TaskState::Failed)
            | (TaskState::Done, TaskState::Validated)
            | (TaskState::Done, TaskState::Failed)
    )
}

/// Registros cuyo estado cambió y aún no se avisó, salvo cambios del propio padre.
pub fn pending_notices(records: &[Record]) -> Vec<&Record> {
    records
        .iter()
        .filter(|record| {
            record.notified != Some(record.state)
                && !record
                    .history
                    .last()
                    .is_some_and(|change| change.by == record.parent)
        })
        .collect()
}

/// Texto exacto del aviso de §13 para el estado actual del registro.
pub fn notice_text(record: &Record) -> String {
    let label = agent_label(record);
    match record.state {
        TaskState::Done => {
            let summary = one_line(record.summary.as_deref().unwrap_or(""), 300);
            // DECISION: sin informe se dice «sin informe» para no dejar «Informe: » vacío.
            let report = record.report.as_deref().unwrap_or("sin informe");
            format!(
                "[EzAgent · aviso] {} ({}) terminó. Resumen del subagente: «{}». Informe: {}. Para validarlo: ez validate {} --pass|--fail --file <evidencia>",
                record.name, label, summary, report, record.name
            )
        }
        TaskState::Failed => {
            let reason = record
                .history
                .last()
                .map(|change| one_line(&change.detail, 300))
                .unwrap_or_default();
            format!(
                "[EzAgent · aviso] {} ({}) falló: {}. Revisa su pantalla: hc read {}",
                record.name, label, reason, record.terminal
            )
        }
        TaskState::Blocked => {
            let detail = record
                .history
                .last()
                .map(|change| one_line(&change.detail, 300))
                .unwrap_or_default();
            let detail = detail.trim_end_matches('.');
            let detail = if detail.is_empty() {
                "necesita que alguien haga algo"
            } else {
                detail
            };
            format!(
                "[EzAgent · aviso] {} ({}) está bloqueado: {}. Opciones: ez retry {} --auto | ez retry {} --agent … | esperar.",
                record.name, label, detail, record.name, record.name
            )
        }
        TaskState::Validated => {
            let Some(validation) = &record.validation else {
                // DECISION: un registro validado sin validación es dato incompleto;
                // se avisa igual con el resumen vacío para no callar el cambio.
                return format!("[EzAgent · aviso] {} quedó validado: «».", record.name);
            };
            let summary = one_line(&validation.summary, 300);
            if validation.verdict {
                format!(
                    "[EzAgent · aviso] {} quedó validado por {}: «{}».",
                    record.name, validation.by, summary
                )
            } else {
                format!(
                    "[EzAgent · aviso] {} no pasó la validación de {}: «{}».",
                    record.name, validation.by, summary
                )
            }
        }
        // DECISION: §13 no define aviso para `en_proceso`; se usa este texto corto
        // para que el vigilante pueda avisar del alta sin inventar un formato nuevo.
        // Si vuelve de `bloqueado`, el aviso dice «siguió trabajando» (§3).
        TaskState::InProgress => {
            let resumed = record
                .history
                .len()
                .checked_sub(2)
                .and_then(|index| record.history.get(index))
                .is_some_and(|change| change.state == TaskState::Blocked);
            if resumed {
                format!(
                    "[EzAgent · aviso] {} ({}) siguió trabajando.",
                    record.name, label
                )
            } else {
                format!(
                    "[EzAgent · aviso] {} ({}) está en proceso.",
                    record.name, label
                )
            }
        }
    }
}

pub(crate) fn agent_label(record: &Record) -> String {
    match record.model.as_deref() {
        Some(model) if !model.is_empty() => format!("{} · {}", record.label, model),
        _ => record.label.clone(),
    }
}

/// Una línea, sin saltos ni caracteres de control, de hasta `max` letras;
/// si se corta, termina en «…».
pub fn one_line(text: &str, max: usize) -> String {
    let clean: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let flat = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    if max == 0 {
        return String::new();
    }
    let keep = flat.chars().take(max - 1).collect::<String>();
    format!("{keep}…")
}

//! Artefactos del modo autónomo (9.3).
//!
//! Carpeta `%APPDATA%/EzAgent/autonomo/<id>/` con `encargo.md`, `criterios.md`,
//! `estado.json` (escritura atómica), `bitacora.jsonl`, `ronda-N/veredicto.json`
//! y `correccion-N.md`.

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Local, TimeZone};
use ez_core::dirs;
use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pytime::now_local_iso_seconds;
use serde_json::{json, Value};

use super::state::AutonomousState;
use super::verdict::{Verdict, VerdictError};

/// Nombre de la carpeta bajo `%APPDATA%/EzAgent`.
pub const ROOT_DIR: &str = "autonomo";

/// Carpeta raíz de las tareas autónomas.
pub fn root() -> PathBuf {
    dirs::app_data_dir().join(ROOT_DIR)
}

/// `id = AAAAMMDD-HHMMSS-<4 hex>` en hora local.
pub fn new_id(now: f64) -> String {
    let local = match Local.timestamp_opt(now as i64, 0).single() {
        Some(local) => local,
        None => Local::now(),
    };
    let suffix = uuid::Uuid::new_v4().as_u128() as u16;
    format!("{}-{suffix:04x}", local.format("%Y%m%d-%H%M%S"))
}

/// Lo que se escribe en `encargo.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encargo {
    pub task: String,
    pub space: String,
    pub workdirs: Vec<String>,
}

impl Encargo {
    fn markdown(&self) -> String {
        let mut text = format!(
            "# Encargo\n\n{}\n\nEspacio: {}\n\nRutas de trabajo:\n",
            self.task, self.space
        );
        if self.workdirs.is_empty() {
            text.push_str(&format!("- {}\n", self.space));
        } else {
            for directory in &self.workdirs {
                text.push_str(&format!("- {directory}\n"));
            }
        }
        text
    }
}

/// Eventos de `bitacora.jsonl` (9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Start,
    Approve,
    Deny,
    VerifyStart,
    Verdict,
    Correction,
    Pause,
    Resume,
    Stop,
    Done,
    Incident,
}

impl Event {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Approve => "approve",
            Self::Deny => "deny",
            Self::VerifyStart => "verify_start",
            Self::Verdict => "verdict",
            Self::Correction => "correction",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Stop => "stop",
            Self::Done => "done",
            Self::Incident => "incident",
        }
    }
}

/// La carpeta de una tarea y sus rutas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDir {
    pub dir: PathBuf,
}

impl TaskDir {
    /// Crea la carpeta y los artefactos iniciales de 9.3.
    pub fn create(
        root: &Path,
        encargo: &Encargo,
        criteria: &[String],
        state: &AutonomousState,
    ) -> Result<Self, String> {
        let task = Self {
            dir: root.join(&state.id),
        };
        std::fs::create_dir_all(&task.dir).map_err(|error| error.to_string())?;
        task.write_text("encargo.md", &encargo.markdown())?;
        task.write_text("criterios.md", &criteria_markdown(criteria))?;
        task.write_state(state)?;
        Ok(task)
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// `estado.json` atómico: temporal + renombrado.
    pub fn write_state(&self, state: &AutonomousState) -> Result<(), String> {
        let value = serde_json::to_value(state).map_err(|error| error.to_string())?;
        let text = dumps_with(&value, &DumpOptions::utf8().with_indent(2));
        self.write_atomic("estado.json", &format!("{text}\n"))
    }

    /// Lee `estado.json` al arrancar (9.3).
    pub fn load_state(&self) -> Result<AutonomousState, String> {
        let text = std::fs::read_to_string(self.dir.join("estado.json"))
            .map_err(|error| error.to_string())?;
        serde_json::from_str(&text).map_err(|error| error.to_string())
    }

    fn write_text(&self, name: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.dir.join(name), text).map_err(|error| error.to_string())
    }

    fn write_atomic(&self, name: &str, text: &str) -> Result<(), String> {
        let temporary = self
            .dir
            .join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
        let written = std::fs::write(&temporary, text.as_bytes());
        if let Err(error) = written {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        if let Err(error) = std::fs::rename(&temporary, self.dir.join(name)) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        Ok(())
    }

    /// Una línea de `bitacora.jsonl` con el instante actual.
    pub fn log(&self, event: Event, detail: &Value) -> Result<(), String> {
        self.log_at(event, detail, &now_local_iso_seconds())
    }

    /// Igual que [`Self::log`] con el instante fijado (pruebas).
    pub fn log_at(&self, event: Event, detail: &Value, ts: &str) -> Result<(), String> {
        let record = json!({"ts": ts, "event": event.as_str(), "detail": detail});
        let line = dumps_with(&record, &DumpOptions::compact_utf8());
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("bitacora.jsonl"))
            .map_err(|error| error.to_string())?;
        file.write_all(format!("{line}\n").as_bytes())
            .map_err(|error| error.to_string())
    }

    pub fn round_dir(&self, round: i64) -> PathBuf {
        self.dir.join(format!("ronda-{round}"))
    }

    pub fn verdict_path(&self, round: i64) -> PathBuf {
        self.round_dir(round).join("veredicto.json")
    }

    pub fn correction_path(&self, round: i64) -> PathBuf {
        self.dir.join(format!("correccion-{round}.md"))
    }

    /// Crea `ronda-N/` y devuelve la ruta del veredicto que escribirá el pez.
    pub fn prepare_round(&self, round: i64) -> Result<PathBuf, String> {
        std::fs::create_dir_all(self.round_dir(round)).map_err(|error| error.to_string())?;
        Ok(self.verdict_path(round))
    }

    /// Lee y valida `ronda-N/veredicto.json`.
    pub fn read_verdict(&self, round: i64) -> Result<Verdict, VerdictError> {
        let text = std::fs::read_to_string(self.verdict_path(round))
            .map_err(|error| VerdictError(format!("No pude leer el veredicto: {error}")))?;
        Verdict::parse(&text)
    }

    /// Escribe `correccion-N.md` con los hallazgos aceptados (9.5).
    pub fn write_correction(&self, round: i64, verdict: &Verdict) -> Result<PathBuf, String> {
        let path = self.correction_path(round);
        let mut text = format!("# Corrección {round}\n\n");
        for finding in verdict.blocking() {
            let criterion = finding
                .criterion
                .map(|number| format!(" (criterio {number})"))
                .unwrap_or_default();
            text.push_str(&format!(
                "- {}{criterion}: {}\n",
                capitalize(finding.severity.as_str()),
                finding.problem
            ));
            if !finding.evidence.is_empty() {
                text.push_str(&format!("  Evidencia: {}\n", finding.evidence));
            }
        }
        std::fs::write(&path, text).map_err(|error| error.to_string())?;
        Ok(path)
    }
}

fn capitalize(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), characters.as_str()),
        None => String::new(),
    }
}

fn criteria_markdown(criteria: &[String]) -> String {
    let mut text = String::from("# Criterios\n\n");
    for (index, criterion) in criteria.iter().enumerate() {
        text.push_str(&format!("{}. {criterion}\n", index + 1));
    }
    text
}

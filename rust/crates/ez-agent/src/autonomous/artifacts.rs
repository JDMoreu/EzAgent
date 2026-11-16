//! Artefactos del modo autónomo (10.10).
//!
//! Carpeta `%APPDATA%/EzAgent/autonomo/<id>/` con:
//! - `privado/`: `estado.json`, `bitacora.jsonl`, `resumen.md` y copias de los
//!   veredictos. Nunca se da su ruta a ningún agente.
//! - `compartido/`: `encargo.md`, `criterios.md`, `correccion-N.md` y `fin-N.md`.
//! - `verificacion-K/`: `veredicto.json` de cada verificación (K global).

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{Local, TimeZone};
use ez_core::dirs;
use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pytime::now_local_iso_seconds;
use ez_core::pyvalue::collapse_spaces;
use serde_json::{json, Value};

use super::audit::redact_value;
use super::state::AutonomousState;
use super::verdict::{Verdict, VerdictError, VerdictSpec};
use super::Criterion;

mod correction;
mod summary;

pub use summary::{CriterionEvidence, SummaryData};

/// Nombre de la carpeta bajo `%APPDATA%/EzAgent`.
pub const ROOT_DIR: &str = "autonomo";
/// Carpeta que nunca se comparte con los agentes.
pub const PRIVATE_DIR: &str = "privado";
/// Carpeta que reciben los agentes.
pub const SHARED_DIR: &str = "compartido";
/// Prefijo de la carpeta de cada verificación.
pub const VERIFICATION_DIR: &str = "verificacion";

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

/// Nonce de 16 hex que el verificador debe repetir en su JSON (10.10).
pub fn new_token() -> String {
    format!("{:016x}", uuid::Uuid::new_v4().as_u128() as u64)
}

/// Lo que se escribe en `compartido/encargo.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encargo {
    pub task: String,
    pub space: String,
    pub workdir: String,
}

impl Encargo {
    fn markdown(&self, shared: &Path) -> String {
        format!(
            "# Encargo\n\n{}\n\nEspacio: {}\n\nRutas de trabajo:\n- {}\n\nCriterios: {}\n\n\
             ## Fin\n\nCuando termines, escribe un resumen en {} (con la N de la ronda en curso) y \
             avisa.\n",
            self.task,
            self.space,
            self.workdir,
            shared.join("criterios.md").display(),
            shared.join("fin-N.md").display()
        )
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

/// Una verificación concreta: su carpeta, su archivo y su nonce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    pub dir: PathBuf,
    pub verdict_path: PathBuf,
    pub token: String,
}

/// La carpeta de una tarea y sus rutas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDir {
    pub dir: PathBuf,
}

impl TaskDir {
    /// Crea la carpeta y los artefactos iniciales de 10.10.
    pub fn create(
        root: &Path,
        encargo: &Encargo,
        criteria: &[Criterion],
        state: &AutonomousState,
    ) -> Result<Self, String> {
        let task = Self {
            dir: root.join(&state.id),
        };
        std::fs::create_dir_all(task.private()).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(task.shared()).map_err(|error| error.to_string())?;
        task.write_shared("encargo.md", &encargo.markdown(&task.shared()))?;
        task.write_shared("criterios.md", &criteria_markdown(criteria))?;
        task.write_state(state)?;
        Ok(task)
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// Carpeta privada: estado, bitácora, resumen y copias de veredictos.
    pub fn private(&self) -> PathBuf {
        self.dir.join(PRIVATE_DIR)
    }

    /// Carpeta compartida con los agentes.
    pub fn shared(&self) -> PathBuf {
        self.dir.join(SHARED_DIR)
    }

    /// `privado/estado.json` atómico: temporal + renombrado.
    pub fn write_state(&self, state: &AutonomousState) -> Result<(), String> {
        let value = serde_json::to_value(state).map_err(|error| error.to_string())?;
        let text = dumps_with(&value, &DumpOptions::utf8().with_indent(2));
        self.write_atomic(&self.private(), "estado.json", &format!("{text}\n"))
    }

    /// Lee `privado/estado.json` al arrancar (9.3).
    pub fn load_state(&self) -> Result<AutonomousState, String> {
        let text = std::fs::read_to_string(self.private().join("estado.json"))
            .map_err(|error| error.to_string())?;
        serde_json::from_str(&text).map_err(|error| error.to_string())
    }

    /// Una línea de `privado/bitacora.jsonl` con el instante actual.
    pub fn log(&self, event: Event, detail: &Value) -> Result<(), String> {
        self.log_at(event, detail, &now_local_iso_seconds())
    }

    /// Igual que [`Self::log`] con el instante fijado (pruebas).
    ///
    /// El detalle se redacta igual que la auditoría (10.15).
    pub fn log_at(&self, event: Event, detail: &Value, ts: &str) -> Result<(), String> {
        let record = json!({"ts": ts, "event": event.as_str(), "detail": redact_value(detail)});
        let line = dumps_with(&record, &DumpOptions::compact_utf8());
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.private().join("bitacora.jsonl"))
            .map_err(|error| error.to_string())?;
        file.write_all(format!("{line}\n").as_bytes())
            .map_err(|error| error.to_string())
    }

    /// Carpeta de la verificación K (K global, empieza en 1).
    pub fn verification_dir(&self, k: i64) -> PathBuf {
        self.dir.join(format!("{VERIFICATION_DIR}-{k}"))
    }

    pub fn verdict_path(&self, k: i64) -> PathBuf {
        self.verification_dir(k).join("veredicto.json")
    }

    /// Copia del veredicto dentro de `privado/` (10.10).
    pub fn verdict_copy_path(&self, k: i64) -> PathBuf {
        self.private().join(format!("veredicto-{k}.json"))
    }

    pub fn correction_path(&self, round: i64) -> PathBuf {
        self.shared().join(format!("correccion-{round}.md"))
    }

    /// `compartido/fin-N.md`, el resumen que escribe el ejecutor (10.10).
    pub fn fin_path(&self, round: i64) -> PathBuf {
        self.shared().join(format!("fin-{round}.md"))
    }

    pub fn summary_path(&self) -> PathBuf {
        self.private().join("resumen.md")
    }

    /// Crea `verificacion-K/` y devuelve su ruta, su veredicto y su nonce.
    pub fn prepare_verification(&self, k: i64) -> Result<Verification, String> {
        let dir = self.verification_dir(k);
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(Verification {
            verdict_path: dir.join("veredicto.json"),
            dir,
            token: new_token(),
        })
    }

    /// Lee y valida `verificacion-K/veredicto.json` (10.11).
    pub fn read_verdict(&self, k: i64, spec: &VerdictSpec<'_>) -> Result<Verdict, VerdictError> {
        let text = std::fs::read_to_string(self.verdict_path(k))
            .map_err(|error| VerdictError(format!("No pude leer el veredicto: {error}")))?;
        Verdict::parse(&text, spec)
    }

    /// Guarda una copia del veredicto en la carpeta privada.
    pub fn store_verdict_copy(&self, k: i64) -> Result<(), String> {
        let source = self.verdict_path(k);
        let target = self.verdict_copy_path(k);
        std::fs::copy(&source, &target)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Escribe `compartido/correccion-N.md` con los hallazgos como dato (10.12).
    pub fn write_correction(&self, round: i64, verdict: &Verdict) -> Result<PathBuf, String> {
        let path = self.correction_path(round);
        std::fs::write(&path, correction::markdown(round, verdict))
            .map_err(|error| error.to_string())?;
        Ok(path)
    }

    /// Escribe `privado/resumen.md` con la evidencia del último veredicto (10.16).
    pub fn write_summary(&self, data: &SummaryData) -> Result<PathBuf, String> {
        let path = self.summary_path();
        std::fs::write(&path, data.markdown()).map_err(|error| error.to_string())?;
        Ok(path)
    }

    /// ¿El ejecutor escribió `fin-N.md` después del último envío? (10.10)
    pub fn fin_written_after(&self, round: i64, sent_at: f64) -> bool {
        std::fs::metadata(self.fin_path(round))
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .is_some_and(|elapsed| elapsed.as_secs_f64() > sent_at)
    }

    fn write_shared(&self, name: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.shared().join(name), text).map_err(|error| error.to_string())
    }

    fn write_atomic(&self, folder: &Path, name: &str, text: &str) -> Result<(), String> {
        let temporary = folder.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
        let written = std::fs::write(&temporary, text.as_bytes());
        if let Err(error) = written {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        if let Err(error) = std::fs::rename(&temporary, folder.join(name)) {
            let _ = std::fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        Ok(())
    }
}

/// Cada criterio en una línea, con su comprobación (10.3).
fn criteria_markdown(criteria: &[Criterion]) -> String {
    let mut text = String::from("# Criterios\n\n");
    for (index, criterion) in criteria.iter().enumerate() {
        text.push_str(&format!(
            "{}. {}\n   Comprobación: {}\n",
            index + 1,
            collapse_spaces(&criterion.text),
            collapse_spaces(&criterion.check)
        ));
    }
    text
}

//! Port of `src/ezagent/swarm/recovery.py` (F4-13b).
//!
//! Recuperación del enjambre: detecta incidentes, pregunta al usuario y retoma.
//!
//! Reglas (lo decide el código; el LLM solo transmite la elección del usuario):
//!
//! - **Sin uso (quota)**: se pausa a ese miembro (no a todo el enjambre), se
//!   avisa a los demás en el tablero y se pregunta al usuario si esperar,
//!   reemplazarlo o pausar. Si se sabe la hora de restablecimiento se espera
//!   sola: a esa hora se le escribe al agente para que siga.
//! - **Sin contexto**: se reabre en una sesión nueva sin preguntar (una vez por
//!   miembro y tarea; si se repite, se pregunta).
//! - **Ventana cerrada, error o sesión perdida**: se pregunta.
//! - **Vuelve solo**: el incidente se cierra y se avisa.

mod decisions;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{Local, TimeZone};
use ez_core::metrics::monotonic;
use ez_core::pytime::timestamp;
use ez_core::pyvalue::{py_rstrip, py_splitlines, py_strip, repr_str, tail_chars, truthy};
use serde_json::Value;
use tracing::{debug, warn};

use crate::db::{Clock, Db, DbResult};
use crate::incidents as inc;
use crate::models::{Incident, Member, SwarmError, SwarmInfo, BROADCAST, SYSTEM};
use crate::service::SwarmService;
use crate::supervisor::{Announce, SwarmHost};

/// Tiempo mínimo entre dos lecturas de la pantalla del mismo miembro.
pub const INSPECT_SECS: f64 = 30.0;
/// Margen tras la hora de restablecimiento antes de escribirle.
pub const RESUME_GRACE_SECS: f64 = 60.0;
/// Espera entre reintentos de «sigue».
pub const RESUME_RETRY_SECS: f64 = 300.0;
/// Reintentos de «sigue» tras el restablecimiento.
pub const MAX_RESUMES: u32 = 3;
/// Reaperturas automáticas por contexto antes de preguntar.
pub const MAX_RENEWALS: u32 = 1;
/// Líneas de pantalla que se guardan en el traspaso.
pub const SNAPSHOT_LINES: i64 = 160;
/// Caracteres de pantalla que se guardan en el traspaso.
pub const SNAPSHOT_CHARS: usize = 8000;
/// Últimas líneas no vacías que se miran de un agente libre.
pub const IDLE_TAIL: usize = 8;
/// Opciones que se ofrecen al usuario.
pub const CHOICES: [&str; 5] = ["wait", "replace", "reopen", "release", "pause"];

type Key = (String, String);
type RenewalKey = (String, String, Vec<i64>);

/// Reinicio de contadores del miembro reabierto o reemplazado.
pub type ResetLaunches = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Colaboradores opcionales (los argumentos con nombre del Python).
pub struct RecoveryOptions {
    /// Miembro reabierto o reemplazado: el supervisor reinicia sus contadores.
    pub reset_launches: ResetLaunches,
    /// Reloj de pared para fechas y horas de restablecimiento.
    pub wall: Clock,
    /// Reloj monotónico para el control de lecturas.
    pub clock: Clock,
}

impl Default for RecoveryOptions {
    fn default() -> Self {
        Self {
            reset_launches: Arc::new(|_, _| {}),
            wall: Arc::new(timestamp),
            clock: Arc::new(monotonic),
        }
    }
}

#[derive(Default)]
struct Book {
    inspected: HashMap<Key, f64>,
    resumes: HashMap<i64, (u32, f64)>,
    renewals: HashMap<RenewalKey, u32>,
}

/// Incidentes de los miembros: detección, avisos y decisiones.
pub struct Recovery {
    service: Arc<SwarmService>,
    host: Arc<dyn SwarmHost>,
    handoff_dir: PathBuf,
    announce: Announce,
    ask: Announce,
    reset_launches: ResetLaunches,
    wall: Clock,
    clock: Clock,
    active: Arc<Mutex<HashSet<Key>>>,
    book: Mutex<Book>,
}

impl Recovery {
    /// Recuperación con los valores por defecto del Python.
    pub fn new(
        service: Arc<SwarmService>,
        host: Arc<dyn SwarmHost>,
        handoff_dir: impl Into<PathBuf>,
        announce: Announce,
        ask: Announce,
    ) -> Self {
        Self::with_options(
            service,
            host,
            handoff_dir,
            announce,
            ask,
            RecoveryOptions::default(),
        )
    }

    /// Recuperación con relojes y reinicio de contadores inyectables.
    pub fn with_options(
        service: Arc<SwarmService>,
        host: Arc<dyn SwarmHost>,
        handoff_dir: impl Into<PathBuf>,
        announce: Announce,
        ask: Announce,
        options: RecoveryOptions,
    ) -> Self {
        let active: Arc<Mutex<HashSet<Key>>> = Arc::new(Mutex::new(HashSet::new()));
        {
            let active = Arc::clone(&active);
            service.on_activity(move |swarm, member| {
                active
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert((swarm.to_string(), member.to_string()));
            });
        }
        Self {
            service,
            host,
            handoff_dir: handoff_dir.into(),
            announce,
            ask,
            reset_launches: options.reset_launches,
            wall: options.wall,
            clock: options.clock,
            active,
            book: Mutex::new(Book::default()),
        }
    }

    fn store(&self) -> &Db {
        &self.service.store
    }

    fn book(&self) -> MutexGuard<'_, Book> {
        self.book
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // ------------------------------------------------------------ cada pasada

    /// `true` si el miembro está bajo un incidente y el supervisor no debe tocarlo.
    pub fn hold(&self, info: &SwarmInfo, member: &Member, working: bool) -> DbResult<bool> {
        let incident = self.store().member_incident(&info.id, &member.name)?;
        let key = (info.id.clone(), member.name.clone());
        let active = self
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&key);
        let Some(incident) = incident else {
            return Ok(false);
        };
        if working || active {
            self.back(info, member, &incident)?;
            return Ok(false);
        }
        if incident.state == "waiting" && incident.resets != 0.0 {
            self.maybe_resume(member, &incident);
        }
        Ok(true)
    }

    /// Lee la pantalla de un agente parado y abre un incidente si reconoce la causa.
    pub fn inspect(
        &self,
        info: &SwarmInfo,
        member: &Member,
        error: bool,
        stuck: bool,
    ) -> DbResult<bool> {
        let key = (info.id.clone(), member.name.clone());
        let now = (self.clock)();
        if now
            - self
                .book()
                .inspected
                .get(&key)
                .copied()
                .unwrap_or(-INSPECT_SECS)
            < INSPECT_SECS
        {
            return Ok(false);
        }
        if !(error || stuck) {
            return Ok(false);
        }
        self.book().inspected.insert(key, now);
        let screen = match self.host.read_agent(&member.address, 40) {
            Ok(screen) => screen,
            Err(error) => {
                debug!("No pude leer a {}: {error}", member.name);
                return Ok(false);
            }
        };
        // Libre: solo cuenta el final de la pantalla (menos falsos positivos).
        let screen = if error { screen } else { idle_tail(&screen) };
        let signal = inc::classify(&screen, local_from((self.wall)()));
        let Some(signal) = signal else {
            if error {
                self.raise_incident(
                    info,
                    member,
                    "error",
                    "Herdr marca el agente con error.",
                    0.0,
                )?;
                return Ok(true);
            }
            return Ok(false);
        };
        if signal.kind == "compacting" {
            return Ok(false);
        }
        if signal.kind == "context" {
            self.renew(info, member, &signal.detail)?;
        } else {
            self.raise_incident(info, member, &signal.kind, &signal.detail, signal.resets)?;
        }
        Ok(true)
    }

    /// Registra el incidente, avisa a los demás en el tablero y al usuario.
    pub fn raise_incident(
        &self,
        info: &SwarmInfo,
        member: &Member,
        kind: &str,
        detail: &str,
        resets: f64,
    ) -> DbResult<Incident> {
        let (incident, new) =
            self.store()
                .open_incident(&info.id, &member.name, kind, detail, resets)?;
        if !new {
            return Ok(incident);
        }
        self.store()
            .set_member_state(&info.id, &member.name, "blocked")?;
        let tasks = self.tasks(&info.id, &member.name)?;
        self.store().post(
            &info.id,
            SYSTEM,
            &inc::board_note(&incident, member, &tasks),
            BROADCAST,
            "sys",
            "",
        )?;
        let alt = self.suggestion(&info.id, member)?;
        warn!(
            "Enjambre {}: incidente I{} {} en {}.",
            info.id, incident.id, kind, member.name
        );
        if inc::ASK_KINDS.contains(&kind) {
            (self.ask)(&inc::question(&incident, member, &tasks, alt.as_ref()));
        } else {
            (self.announce)(&format!(
                "Enjambre: {}",
                inc::describe(&incident, member, &tasks)
            ));
        }
        Ok(incident)
    }

    // -------------------------------------------------------------- internos

    /// El agente ya no está: «exited» si salió solo, «closed» si cerraron la ventana.
    pub fn closed(&self, info: &SwarmInfo, member: &Member, exited: bool) -> DbResult<()> {
        let kind = if exited { "exited" } else { "closed" };
        let detail = if exited {
            "El programa salió solo."
        } else {
            "Su ventana ya no está abierta."
        };
        self.raise_incident(info, member, kind, detail, 0.0)?;
        Ok(())
    }

    /// Contexto lleno: sesión nueva con el avance guardado (una vez por tarea).
    fn renew(&self, info: &SwarmInfo, member: &Member, detail: &str) -> DbResult<()> {
        let tasks: Vec<i64> = self
            .store()
            .mine(&info.id, &member.name)?
            .iter()
            .map(|task| task.id)
            .collect();
        let key = (info.id.clone(), member.name.clone(), tasks);
        let count = self.book().renewals.get(&key).copied().unwrap_or(0);
        if count >= MAX_RENEWALS {
            self.raise_incident(info, member, "context", detail, 0.0)?;
            (self.ask)(&format!(
                "Enjambre: {} se quedó sin contexto otra vez en la misma tarea. \
                 ¿Lo reemplazo con otro agente, reparto su trabajo o pauso el enjambre?",
                member.name
            ));
            return Ok(());
        }
        self.book().renewals.insert(key, count + 1);
        let (incident, _) =
            self.store()
                .open_incident(&info.id, &member.name, "context", detail, 0.0)?;
        self.handoff(
            &incident,
            member,
            &member.agent,
            "Tu sesión anterior se quedó sin contexto",
            None,
            None,
        )?;
        self.store()
            .set_incident(incident.id, "resolved", "renew")?;
        (self.announce)(&format!(
            "Enjambre: {} se quedó sin contexto; lo reabro en una sesión nueva \
             con su avance guardado.",
            member.name
        ));
        Ok(())
    }

    fn back(&self, info: &SwarmInfo, member: &Member, incident: &Incident) -> DbResult<()> {
        self.store().set_incident(incident.id, "resolved", "back")?;
        self.book().resumes.remove(&incident.id);
        if member.state == "blocked" {
            self.store()
                .set_member_state(&info.id, &member.name, "working")?;
        }
        let tasks = self.tasks(&info.id, &member.name)?;
        self.store().post(
            &info.id,
            SYSTEM,
            &format!("{} volvió.", member.name),
            BROADCAST,
            "sys",
            "",
        )?;
        let tail = if tasks.is_empty() {
            ".".to_string()
        } else {
            format!(" y retoma {}.", tasks.join(", "))
        };
        (self.announce)(&format!(
            "Enjambre: {} ({}) volvió{tail}",
            member.name, member.agent
        ));
        Ok(())
    }

    /// Pasada la hora de restablecimiento, le escribe al agente para que siga.
    fn maybe_resume(&self, member: &Member, incident: &Incident) {
        let now = (self.wall)();
        if now < incident.resets + RESUME_GRACE_SECS || member.address.is_empty() {
            return;
        }
        let (count, last) = self
            .book()
            .resumes
            .get(&incident.id)
            .copied()
            .unwrap_or((0, 0.0));
        if count >= MAX_RESUMES || (count > 0 && now - last < RESUME_RETRY_SECS) {
            return;
        }
        let line = format!(
            "[EzAgent] Ya se restableció tu uso. Sigue con tu trabajo: {} next",
            self.service.command
        );
        match self.host.send_task(&member.address, &line) {
            Ok(sent) if truthy(sent.get("ok").unwrap_or(&Value::Null)) => {
                self.book().resumes.insert(incident.id, (count + 1, now));
            }
            Ok(_) => {}
            Err(error) => warn!("No pude retomar a {}: {error}", member.name),
        }
    }

    /// Pantalla del agente saliente en un archivo; devuelve su ruta o «».
    fn snapshot(&self, swarm: &str, member: &Member) -> DbResult<String> {
        let screen = match self.host.read_agent(&member.address, SNAPSHOT_LINES) {
            Ok(screen) => screen,
            Err(error) => {
                debug!("No pude guardar el avance de {}: {error}", member.name);
                return Ok(String::new());
            }
        };
        let lines: Vec<&str> = py_splitlines(&screen)
            .into_iter()
            .filter(|line| !py_strip(line).is_empty())
            .collect();
        let joined = lines
            .iter()
            .map(|line| py_rstrip(line))
            .collect::<Vec<_>>()
            .join("\n");
        let body = tail_chars(&joined, SNAPSHOT_CHARS).to_string();
        if body.is_empty() {
            return Ok(String::new());
        }
        fs::create_dir_all(&self.handoff_dir)?;
        let stamp = local_from((self.wall)())
            .format("%Y%m%d-%H%M%S")
            .to_string();
        let path = self
            .handoff_dir
            .join(format!("{swarm}-{}-{stamp}.md", member.name));
        let header = format!(
            "# Avance de {} ({})\n\nÚltima pantalla, sin editar:\n\n",
            member.name, member.agent
        );
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let tmp = path.with_file_name(format!("{name}.tmp"));
        fs::write(&tmp, format!("{header}```text\n{body}\n```\n"))?;
        fs::rename(&tmp, &path)?;
        Ok(path.to_string_lossy().into_owned())
    }

    fn tasks(&self, swarm: &str, member: &str) -> DbResult<Vec<String>> {
        Ok(self
            .store()
            .mine(swarm, member)?
            .iter()
            .map(|task| format!("T{}", task.id))
            .collect())
    }

    fn member(&self, swarm: &str, name: &str) -> DbResult<Member> {
        match self
            .store()
            .members(swarm)?
            .into_iter()
            .find(|item| item.name == name)
        {
            Some(member) => Ok(member),
            None => Err(SwarmError::new(format!(
                "{} ya no es miembro del enjambre {swarm}.",
                repr_str(name)
            ))
            .into()),
        }
    }

    fn suggestion(&self, swarm: &str, failing: &Member) -> DbResult<Option<Member>> {
        let busy: HashSet<String> = self
            .store()
            .incidents(swarm, true)?
            .into_iter()
            .map(|incident| incident.member)
            .collect();
        Ok(inc::suggest(&self.store().members(swarm)?, failing, &busy).cloned())
    }
}

/// Últimas [`IDLE_TAIL`] líneas no vacías de la pantalla, sin recortar.
fn idle_tail(screen: &str) -> String {
    let lines: Vec<&str> = py_splitlines(screen)
        .into_iter()
        .filter(|line| !py_strip(line).is_empty())
        .collect();
    let start = lines.len().saturating_sub(IDLE_TAIL);
    lines[start..].join("\n")
}

/// `datetime.fromtimestamp(secs)`, en hora local.
fn local_from(secs: f64) -> chrono::NaiveDateTime {
    Local
        .timestamp_opt(secs as i64, 0)
        .earliest()
        .map_or_else(|| Local::now().naive_local(), |value| value.naive_local())
}

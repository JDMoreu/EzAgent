//! `AutonomousHost`: los verbos que usan las herramientas y el arranque de una
//! tarea (10.1, 10.2 y 10.13).

use std::path::Path;

use ez_agent::autonomous::artifacts::{self, Encargo, Event, TaskDir};
use ez_agent::autonomous::{
    AutonomousHost, AutonomousState, Limits, Mode, StartSpec, Summary, TeamSpec,
};
use ez_core::pyvalue::{collapse_spaces, text_or_empty, truthy};
use serde_json::{json, Map, Value};

use super::{AutonomousLoop, Request, Session};

mod git;
pub(crate) mod restore;
pub(crate) mod start;

pub use git::{diff_stat, log_oneline};
pub use start::launch_executor;

impl AutonomousHost for AutonomousLoop {
    fn start(&self, spec: &StartSpec) -> Result<String, String> {
        self.start_task(spec)
    }

    fn start_team(&self, _spec: &TeamSpec) -> Result<String, String> {
        Err(ez_agent::autonomous::texts::TEAM_UNAVAILABLE.to_string())
    }

    fn status(&self) -> Option<Summary> {
        AutonomousLoop::status(self)
    }

    fn resume(&self, extend_rounds: i64, extend_minutes: i64) -> Result<(), String> {
        self.resume_task(extend_rounds, extend_minutes)
    }

    fn stop(&self, reason: &str, accept: bool) -> Result<(), String> {
        self.stop_task(reason, accept)
    }
}

impl AutonomousLoop {
    /// `start_autonomous_task`: valida, crea los artefactos y lanza.
    pub fn start_task(&self, spec: &StartSpec) -> Result<String, String> {
        self.reserve()?;
        match self.build(spec) {
            Ok(session) => {
                let id = session.id().to_string();
                self.adopt(session);
                self.say(ez_agent::autonomous::texts::START);
                Ok(id)
            }
            Err(error) => {
                self.unreserve();
                Err(error)
            }
        }
    }

    /// Construye la tarea y lanza el ejecutor; sin cerrojos tomados.
    fn build(&self, spec: &StartSpec) -> Result<Session, String> {
        let now = self.now();
        let workdir = canonical_workdir(&spec.workdir)?;
        if let Some(terminal) = spec.terminal.as_deref() {
            check_terminal(self.port.as_ref(), terminal, &spec.space)?;
        }
        let id = artifacts::new_id(now);
        let mut state = AutonomousState::new(
            &id,
            Mode::Single,
            &workdir,
            now,
            Limits {
                max_rounds: spec.max_rounds,
                max_minutes: spec.max_minutes,
                max_approvals: spec.max_approvals,
            },
        );
        state.base_commit = git::base_commit(&workdir);
        let encargo = Encargo {
            task: spec.task.clone(),
            space: spec.space.clone(),
            workdir: workdir.clone(),
        };
        let dir = TaskDir::create(&self.root, &encargo, &spec.done_criteria, &state)?;
        // DECISION: A1 escribe literalmente «fin-N.md (con la N de la ronda en
        // curso)»; el bucle necesita el nombre exacto de la ronda 1.
        fix_encargo_fin(&dir, 1)?;
        let mut session = Session::new(spec, state, dir, now);
        let launched = match spec.terminal.as_deref() {
            Some(address) => send_encargo(self.port.as_ref(), address, &session).map(|()| {
                session.state.executor_started(address, &spec.agent);
            }),
            None => launch_executor(self, &mut session),
        };
        if let Err(error) = launched {
            // Un arranque fallido no debe reaparecer como tarea viva al reiniciar.
            let _ = session.state.stop(&error, false);
            self.save(&session);
            return Err(error);
        }
        session.sent_at = self.now();
        self.log(
            &session,
            Event::Start,
            json!({
                "task": session.task,
                "space": session.space,
                "workdir": session.state.workdir,
                "executor": session.state.executor.as_ref().map(|executor| executor.address.clone()),
                "base_commit": session.state.base_commit,
            }),
        );
        self.save(&session);
        Ok(session)
    }

    /// «Para»: primero se retira la autorización (10.7).
    pub fn stop_task(&self, reason: &str, accept: bool) -> Result<(), String> {
        if self.status().is_none() {
            return Err(ez_agent::autonomous::texts::NO_TASK.to_string());
        }
        self.revoke();
        self.request(Request::Stop {
            reason: reason.to_string(),
            accept,
        });
        Ok(())
    }

    /// «Sigue» en una tarea en pausa (10.13).
    pub fn resume_task(&self, extend_rounds: i64, extend_minutes: i64) -> Result<(), String> {
        if !self
            .status()
            .is_some_and(|summary| summary.state == ez_agent::autonomous::TaskState::Paused)
        {
            return Err(ez_agent::autonomous::texts::NOT_PAUSED.to_string());
        }
        self.request(Request::Resume {
            rounds: extend_rounds,
            minutes: extend_minutes,
        });
        Ok(())
    }

    /// Pausa desde el hilo: primero el contador, luego el estado (10.7).
    pub(super) fn pause(&self, session: &mut Session, reason: &str) {
        self.revoke();
        let action = session.state.pause(reason, self.now());
        self.apply(session, action);
    }
}

/// Deja el nombre exacto de `fin-N.md` en el encargo de la ronda 1.
///
/// `TaskDir::create` (A1) escribe «fin-N.md (con la N de la ronda en curso)»;
/// el agente debe ver un archivo concreto o escribiría literalmente `fin-N.md`.
fn fix_encargo_fin(dir: &TaskDir, round: i64) -> Result<(), String> {
    let path = dir.shared().join("encargo.md");
    let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let fixed = text.replace(
        "fin-N.md (con la N de la ronda en curso)",
        &format!("fin-{round}.md"),
    );
    std::fs::write(&path, fixed).map_err(|error| error.to_string())
}

/// `10.2`: se canonicaliza y tiene que existir.
pub fn canonical_workdir(workdir: &str) -> Result<String, String> {
    let path = Path::new(workdir);
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| ez_agent::autonomous::texts::workdir_not_found(workdir))?;
    if !canonical.is_dir() {
        return Err(ez_agent::autonomous::texts::workdir_not_found(workdir));
    }
    Ok(canonical.to_string_lossy().to_string())
}

/// Decisión 2: la terminal solo entra si existe y está en el espacio pedido.
fn check_terminal(
    port: &dyn ez_agent::tools_herdr::port::HerdrPort,
    terminal: &str,
    space: &str,
) -> Result<(), String> {
    let row = port
        .get_agent(terminal)
        .map_err(|_| ez_agent::autonomous::texts::terminal_not_found(terminal, space))?;
    let found = space_of(&row);
    if found.is_empty() || spaces_equal(&found, space) {
        Ok(())
    } else {
        Err(ez_agent::autonomous::texts::terminal_not_found(
            terminal, space,
        ))
    }
}

fn spaces_equal(left: &str, right: &str) -> bool {
    collapse_spaces(left) == collapse_spaces(right)
}

/// El espacio de una fila de Herdr (`where` o `space`).
pub fn space_of(row: &Map<String, Value>) -> String {
    let where_text = collapse_spaces(&text_or_empty(row.get("where")));
    if let Some((left, _)) = where_text.split_once(" / ") {
        let space = left.trim();
        if !space.is_empty() {
            return space.to_string();
        }
    }
    collapse_spaces(&text_or_empty(row.get("space")))
}

/// Dirección devuelta por `run_agent`.
pub fn address_of(row: &Map<String, Value>) -> Option<String> {
    for key in ["address", "terminal"] {
        if let Some(value) = row.get(key).filter(|value| truthy(value)) {
            return Some(text_or_empty(Some(value)));
        }
    }
    None
}

pub fn ok_of(row: &Map<String, Value>) -> bool {
    row.get("ok").is_some_and(truthy)
}

/// Texto del error de una fila de Herdr.
pub fn error_of(row: &Map<String, Value>) -> String {
    let text = text_or_empty(row.get("error"));
    if text.is_empty() {
        "Herdr no confirmó el lanzamiento".to_string()
    } else {
        text
    }
}

/// Envía la ruta de `encargo.md` a una terminal ya abierta (9.5).
pub fn send_encargo(
    port: &dyn ez_agent::tools_herdr::port::HerdrPort,
    address: &str,
    session: &Session,
) -> Result<(), String> {
    let text = session.encargo_path().to_string_lossy().to_string();
    let result = port
        .send_agent(address, &text, true)
        .map_err(|error| error.to_string())?;
    if ok_of(&result) {
        Ok(())
    } else {
        Err(error_of(&result))
    }
}

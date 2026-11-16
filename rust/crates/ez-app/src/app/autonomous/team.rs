//! Modo autónomo con equipo sobre el enjambre (10.18).
//!
//! El bucle arranca un enjambre con `goal = task` y los papeles declarados; sus
//! miembros entran en `scope` con papel `member` y editan solo en `workdir`.
//! Cuando el tablero no tiene tareas pendientes ni en curso se lanza la
//! verificación de A3 con un verificador aparte; una ronda sucia vuelve al
//! tablero como una tarea `fix` por hallazgo bloqueante o mayor.
//!
//! El enjambre se marca como autónomo **antes** de activarse: sus miembros se
//! lanzan sin los argumentos que saltan permisos y su recuperación no pregunta.

use std::sync::Arc;

use ez_agent::autonomous::artifacts::{self, Encargo, Event, TaskDir};
use ez_agent::autonomous::texts;
use ez_agent::autonomous::{
    AutonomousState, Limits, Mode, RoleSpec, TaskState, Team, TeamRole, TeamSpec,
};
use ez_core::pyvalue::{char_len, head_chars, py_strip, repr_str, strip_accents};
use ez_swarm::models::{clip, member_name, role_name, MAX_GOAL, MAX_REF};
use ez_swarm::service::MemberSpec;
use ez_swarm::stack::SwarmStack;
use ez_swarm::supervisor::SwarmHost;
use serde_json::json;

use super::host::{base_commit, canonical_workdir, fix_encargo_fin};
use super::verify;
use super::{approve, AutonomousLoop, Session};

/// Alias de los papeles, los mismos de `tools_swarm` (10.18).
///
/// DECISION: `tools_swarm` no expone su tabla de alias y no es un archivo de
/// esta tarea, así que se copia aquí; si cambia allí, cambia aquí.
const ROLE_ALIASES: [(&str, &str); 13] = [
    (
        "plan",
        "planificar, planificador, planner, planear, planeacion, planificacion",
    ),
    ("arch", "arquitecto, arquitectura, architect, arquitecta"),
    (
        "lead",
        "orquestador, orquestar, coordinador, coordinar, lider, orchestrator",
    ),
    (
        "dev",
        "desarrollar, desarrollador, developer, implementar, programador",
    ),
    ("back", "backend, back-end, servidor, api"),
    ("front", "frontend, front-end, interfaz web, cliente"),
    ("ux", "ui, disenador, diseno, designer, ux/ui, ui/ux"),
    ("data", "datos, data engineer, base de datos"),
    (
        "ops",
        "devops, despliegue, desplegar, deploy, infraestructura",
    ),
    ("docs", "documentar, documentador, documentacion, writer"),
    (
        "qa",
        "validar, validador, auditar, auditor, revisar, reviewer, revisor",
    ),
    ("test", "tester, pruebas, probar, testing"),
    ("sec", "seguridad, security, pentester"),
];

/// Un miembro del tablero, tal como lo ve el bucle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamMember {
    pub name: String,
    pub role: String,
    pub address: String,
    /// Estado del tablero: `idle`, `working`, `blocked` o `gone`.
    pub state: String,
}

/// Lo que el bucle necesita del enjambre (10.18).
///
/// Producción usa [`StackSwarm`]; las pruebas, un tablero en memoria. Ninguna
/// llamada se hace con un cerrojo retenido.
pub trait TeamSwarm: Send + Sync {
    /// Clave canónica del agente, o `None` si no está en el catálogo.
    fn resolve_agent(&self, name: &str) -> Option<String>;
    /// Agentes del catálogo, para el error de uno desconocido.
    fn agent_names(&self) -> Vec<String>;
    /// Crea el enjambre ya marcado como autónomo y devuelve su id.
    fn start(&self, goal: &str, workdir: &str, members: &[MemberSpec]) -> Result<String, String>;
    /// Miembros del tablero, en orden de alta.
    fn members(&self, swarm: &str) -> Result<Vec<TeamMember>, String>;
    /// `active`, `paused`, `done`, `stopped` o «».
    fn status(&self, swarm: &str) -> Result<String, String>;
    /// Estado de cada tarea del tablero.
    fn task_states(&self, swarm: &str) -> Result<Vec<String>, String>;
    /// Incidentes activos del enjambre.
    fn incidents(&self, swarm: &str) -> Result<Vec<i64>, String>;
    /// Crea una tarea `fix` con su nota y la asigna a un papel.
    fn fix_task(&self, swarm: &str, title: &str, note: &str, role: &str) -> Result<i64, String>;
}

/// El enjambre de producción.
pub struct StackSwarm {
    stack: Arc<SwarmStack>,
}

impl StackSwarm {
    pub fn new(stack: Arc<SwarmStack>) -> Self {
        Self { stack }
    }
}

impl TeamSwarm for StackSwarm {
    fn resolve_agent(&self, name: &str) -> Option<String> {
        self.stack.host().resolve_agent(name)
    }

    fn agent_names(&self) -> Vec<String> {
        self.stack.host().agent_names()
    }

    fn start(&self, goal: &str, workdir: &str, members: &[MemberSpec]) -> Result<String, String> {
        if self.stack.command_dir.is_empty() {
            return Err(
                "No encuentro el comando ezs; reinstala EzAgent con «uv sync».".to_string(),
            );
        }
        let goal = long_goal(goal, self.stack.host().as_ref());
        self.stack
            .start_autonomous_team(&goal, workdir, members)
            .map_err(|error| error.to_string())
    }

    fn members(&self, swarm: &str) -> Result<Vec<TeamMember>, String> {
        Ok(self
            .stack
            .store()
            .members(swarm)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|member| TeamMember {
                name: member.name,
                role: member.role,
                address: member.address,
                state: member.state,
            })
            .collect())
    }

    fn status(&self, swarm: &str) -> Result<String, String> {
        Ok(self
            .stack
            .store()
            .get(swarm)
            .map_err(|error| error.to_string())?
            .map_or_else(String::new, |info| info.status))
    }

    fn task_states(&self, swarm: &str) -> Result<Vec<String>, String> {
        Ok(self
            .stack
            .store()
            .tasks(swarm)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|task| task.state)
            .collect())
    }

    fn incidents(&self, swarm: &str) -> Result<Vec<i64>, String> {
        Ok(self
            .stack
            .store()
            .incidents(swarm, true)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|incident| incident.id)
            .collect())
    }

    fn fix_task(&self, swarm: &str, title: &str, note: &str, role: &str) -> Result<i64, String> {
        self.stack
            .service
            .fix_task(swarm, title, note, role)
            .map_err(|error| error.to_string())
    }
}

/// Un encargo largo va a un archivo, como en `tools_swarm`.
fn long_goal(goal: &str, host: &dyn SwarmHost) -> String {
    if char_len(goal) <= MAX_GOAL {
        return goal.to_string();
    }
    let path = host
        .save_context(goal)
        .filter(|path| char_len(path) <= MAX_REF);
    let Some(path) = path else {
        return goal.to_string(); // el tablero lo recorta
    };
    let note = format!(" [Encargo completo, léelo antes de empezar: {path}]");
    let head = clip(goal, MAX_GOAL.saturating_sub(char_len(&note))).0;
    format!("{head}{note}")
}

impl AutonomousLoop {
    /// `start_autonomous_team`: valida, crea el enjambre y publica la tarea.
    pub fn start_team_task(&self, spec: &TeamSpec) -> Result<String, String> {
        self.reserve()?;
        match self.build_team(spec) {
            Ok(session) => {
                let id = session.id().to_string();
                self.adopt(session);
                self.say(texts::START);
                Ok(id)
            }
            Err(error) => {
                self.unreserve();
                Err(error)
            }
        }
    }

    /// Construye la tarea y arranca el enjambre; sin cerrojos tomados.
    fn build_team(&self, spec: &TeamSpec) -> Result<Session, String> {
        let now = self.now();
        let workdir = canonical_workdir(&spec.workdir)?;
        let team = self.team_swarm()?;
        let members = team_members(&spec.roles, team.as_ref())?;
        let verifier_agent = if spec.verifier_agent.is_empty() {
            members[0].agent.clone()
        } else {
            spec.verifier_agent.clone()
        };
        let roles: Vec<TeamRole> = members
            .iter()
            .map(|member| TeamRole {
                role: member.role.clone(),
                agent: member.agent.clone(),
                model: (!member.model.is_empty()).then(|| member.model.clone()),
            })
            .collect();
        let id = artifacts::new_id(now);
        let mut state = AutonomousState::new(
            &id,
            Mode::Team,
            &workdir,
            now,
            Limits {
                max_rounds: spec.max_rounds,
                max_minutes: spec.max_minutes,
                max_approvals: spec.max_approvals,
            },
        );
        state.base_commit = base_commit(&workdir);
        let encargo = Encargo {
            task: spec.task.clone(),
            space: spec.space.clone(),
            workdir: workdir.clone(),
        };
        let dir = TaskDir::create(&self.root, &encargo, &spec.done_criteria, &state)?;
        fix_encargo_fin(&dir, 1)?;
        let swarm = match team.start(&spec.task, &workdir, &members) {
            Ok(swarm) => swarm,
            Err(error) => {
                // Un arranque fallido no debe reaparecer como tarea viva al reiniciar.
                let _ = state.stop(&error, false);
                let _ = dir.write_state(&state);
                return Err(error);
            }
        };
        // DECISION: el estado del equipo se publica con los papeles ya
        // normalizados; `AutonomousState` no tiene una transición pura de
        // arranque del equipo y sus campos son públicos.
        state.team = Some(Team {
            swarm: swarm.clone(),
            roles,
        });
        state.state = TaskState::Running;
        let session = Session {
            state,
            dir,
            task: spec.task.clone(),
            criteria: spec.done_criteria.clone(),
            space: spec.space.clone(),
            executor_agent: verifier_agent.clone(),
            verifier_agent,
            verifier_model: spec.verifier_model.clone(),
            sent_at: now,
            idle_since: None,
            verification: None,
            last_verdict: None,
            last_clean: None,
            fingerprints: Vec::new(),
            announced_round: 0,
            seen_incidents: Default::default(),
        };
        self.log(
            &session,
            Event::Start,
            json!({
                "task": session.task,
                "space": session.space,
                "workdir": session.state.workdir,
                "swarm": swarm,
                "members": members.iter().map(|member| member.name.clone()).collect::<Vec<_>>(),
                "base_commit": session.state.base_commit,
            }),
        );
        self.save(&session);
        Ok(session)
    }

    /// Un tic del equipo: incidentes, miembros, tablero y permisos (10.18).
    pub(super) fn tick_team(&self, session: &mut Session) {
        let now = self.now();
        let action = session.state.on_deadline(now);
        if matches!(action, ez_agent::autonomous::Action::Pause(_)) {
            self.apply(session, action);
            return;
        }
        let team = match self.team_swarm() {
            Ok(team) => team,
            Err(reason) => {
                self.pause(session, &reason);
                return;
            }
        };
        let Some(info) = session.state.team.clone() else {
            return;
        };
        let swarm = info.swarm.as_str();
        if self.team_incident(session, team.as_ref(), swarm) {
            return;
        }
        let members = match team.members(swarm) {
            Ok(members) => members,
            Err(error) => {
                tracing::debug!("No pude leer los miembros del enjambre {swarm}: {error}");
                return;
            }
        };
        for member in &members {
            if member.address.is_empty() {
                continue;
            }
            let fresh = !covered(session, &member.address);
            if fresh {
                session.state.member_started(&member.address);
            }
            if fresh || member.state == "blocked" {
                // 10.2/10.9: el miembro se abrió desde el enjambre y su CLI
                // puede estar pidiendo confiar en la carpeta. Se aprueba con la
                // misma transacción que el ejecutor único; fuera de `workdir`,
                // se pausa.
                // DECISION: se reintenta mientras el tablero lo dé por
                // bloqueado (el supervisor lo marca al abrir con el diálogo).
                if let approve::Approval::Paused(reason) =
                    approve::approve_launch(self, session, &member.address)
                {
                    self.pause(session, &reason);
                    return;
                }
            }
        }
        let drained = match team.task_states(swarm) {
            Ok(states) => !states
                .iter()
                .any(|state| state == "pending" || state == "claimed"),
            Err(error) => {
                tracing::debug!("No pude leer el tablero {swarm}: {error}");
                return;
            }
        };
        if drained {
            let action = session.state.on_executor_finished(now);
            self.apply(session, action);
            return;
        }
        let status = team.status(swarm).unwrap_or_default();
        if status == "stopped" || status == "paused" {
            self.pause(session, texts::REASON_TEAM_STOPPED);
            return;
        }
        for member in &members {
            if member.address.is_empty() {
                continue;
            }
            self.poll_other(session, &member.address);
            if session.state.state != TaskState::Running {
                return;
            }
        }
    }

    /// Incidente del enjambre que se mantiene dos tics (10.18).
    ///
    /// Un tic basta para ver un incidente recién abierto que la recuperación
    /// resuelve en la misma pasada; pausar por eso sería un falso positivo.
    fn team_incident(&self, session: &mut Session, team: &dyn TeamSwarm, swarm: &str) -> bool {
        let active = team.incidents(swarm).unwrap_or_default();
        let repeated = active.iter().any(|id| session.seen_incidents.contains(id));
        session.seen_incidents = active.into_iter().collect();
        if repeated {
            self.pause(session, texts::REASON_TEAM_INCIDENT);
        }
        repeated
    }

    /// Una ronda sucia del equipo: una tarea `fix` por hallazgo (10.18).
    pub(super) fn team_correction(&self, session: &mut Session, path: &str) -> Result<(), String> {
        let verdict = verify::write_correction(session)?
            .ok_or_else(|| ez_agent::autonomous::texts::REASON_DOUBTFUL.to_string())?;
        let team = self.team_swarm()?;
        let info = session
            .state
            .team
            .clone()
            .ok_or_else(|| texts::REASON_TEAM_STOPPED.to_string())?;
        let role = assign_role(&info.roles).ok_or_else(|| texts::REASON_TEAM_ROLE.to_string())?;
        let findings: Vec<&ez_agent::autonomous::Finding> = verdict.blocking().collect();
        for (index, _finding) in findings.iter().enumerate() {
            let title = format!("Corregir hallazgo {}", index + 1);
            team.fix_task(&info.swarm, &title, path, &role)
                .map_err(|error| texts::send_failed("la corrección", &error))?;
        }
        session.sent_at = self.now();
        self.log(
            session,
            Event::Correction,
            json!({"round": session.state.round - 1, "path": path, "tasks": findings.len()}),
        );
        Ok(())
    }
}

/// Papel que recibe las correcciones: `dev`, si no el primero que no sea
/// `plan` ni `qa`; si no hay ninguno, no hay a quién mandarlas (10.18).
fn assign_role(roles: &[TeamRole]) -> Option<String> {
    if let Some(dev) = roles.iter().find(|role| role.role == "dev") {
        return Some(dev.role.clone());
    }
    roles
        .iter()
        .find(|role| role.role != "plan" && role.role != "qa")
        .map(|role| role.role.clone())
}

fn covered(session: &Session, address: &str) -> bool {
    session
        .state
        .scope
        .iter()
        .any(|entry| entry.address == address)
}

/// Papeles a miembros del tablero, con los alias de `tools_swarm` (10.18).
fn team_members(roles: &[RoleSpec], team: &dyn TeamSwarm) -> Result<Vec<MemberSpec>, String> {
    let mut specs: Vec<MemberSpec> = Vec::new();
    let mut used: Vec<String> = Vec::new();
    for spec in roles {
        let role = normalize_role(&spec.role);
        if role.is_empty() {
            return Err(format!(
                "Cada papel necesita un nombre válido: {}.",
                repr_str(&spec.role)
            ));
        }
        let Some(agent) = team.resolve_agent(&spec.agent) else {
            let mut known = team.agent_names();
            known.sort();
            return Err(format!(
                "Agente desconocido: {}. Disponibles: {}.",
                repr_str(&spec.agent),
                known.join(", ")
            ));
        };
        let base = member_name(&role).map_err(|error| error.to_string())?;
        let mut name = base.clone();
        let mut suffix = 2;
        while used.contains(&name) {
            name = format!("{base}{suffix}");
            suffix += 1;
        }
        used.push(name.clone());
        specs.push(MemberSpec {
            name,
            role,
            agent,
            model: spec.model.clone().unwrap_or_default(),
            effort: String::new(),
            task: String::new(),
            parallel: false,
        });
    }
    if specs.is_empty() {
        return Err("Indica al menos un papel.".to_string());
    }
    Ok(specs)
}

/// Rol normalizado: alias conocido o nombre corto.
fn normalize_role(raw: &str) -> String {
    let plain = strip_accents(&py_strip(raw).to_lowercase());
    let chosen = alias(&plain).unwrap_or_else(|| slug(&plain));
    role_name(&chosen).unwrap_or_default()
}

fn alias(plain: &str) -> Option<String> {
    for (role, names) in ROLE_ALIASES {
        if names.split(", ").any(|name| name == plain) {
            return Some(role.to_string());
        }
    }
    None
}

/// Nombre corto: sin tildes, a-z, 0-9, «-» y «_», sin guiones sobrantes.
fn slug(text: &str) -> String {
    let plain = strip_accents(&text.to_lowercase());
    let mut kept = String::new();
    let mut separate = false;
    for character in plain.chars() {
        let kept_char = character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '_'
            || character == '-';
        if kept_char {
            kept.push(character);
            separate = false;
        } else if !separate && !kept.is_empty() {
            kept.push('-');
            separate = true;
        }
    }
    let trimmed = kept.trim_matches(|character| character == '-' || character == '_');
    head_chars(trimmed, 32).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_use_the_swarm_aliases_and_short_names() {
        for (raw, expected) in [
            ("Planificador", "plan"),
            ("  QA  ", "qa"),
            ("desarrollador", "dev"),
            ("Back-End", "back"),
            ("Interfaz Web", "front"),
            ("zz", "zz"),
            ("", ""),
            ("!!!", ""),
        ] {
            assert_eq!(normalize_role(raw), expected, "{raw}");
        }
        assert_eq!(slug("Migración Ñandú"), "migracion-nandu");
    }

    #[test]
    fn the_correction_goes_to_dev_then_to_any_other_role() {
        let role = |name: &str| TeamRole {
            role: name.to_string(),
            agent: "claude".to_string(),
            model: None,
        };
        assert_eq!(
            assign_role(&[role("plan"), role("dev"), role("qa")]),
            Some("dev".to_string())
        );
        assert_eq!(
            assign_role(&[role("plan"), role("arch"), role("qa")]),
            Some("arch".to_string())
        );
        assert_eq!(assign_role(&[role("plan"), role("qa")]), None);
        assert_eq!(assign_role(&[]), None);
    }
}

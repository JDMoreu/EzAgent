//! Port of `src/ezagent/swarm/service.py` (F3-12).
//!
//! Swarm service: identity by token and the agents' commands. A member token
//! tells the service who is speaking (swarm, name and role) without the agent
//! saying it, and nobody can impersonate another. Commands return short text
//! instead of JSON to spend fewer tokens.

use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex};

use sha2::{Digest, Sha256};
use tracing::{error, info};

use crate::db::{Clock, Db, DbResult};
use crate::models::{clip, Member, SwarmError, MAX_TITLE, SYSTEM};
use crate::protocol;
use crate::view::{board, health, summary, Health};

mod commands;
mod support;

use support::{base64_url, known_command};

/// Most tasks one swarm may hold.
pub const MAX_TASKS: usize = 60;
/// Longest wait of the `wait` command.
pub const MAX_WAIT_SECS: f64 = 110.0;
/// Messages allowed per burst.
pub const BURST_MESSAGES: usize = 10;
/// Window of the message burst.
pub const BURST_SECS: f64 = 60.0;

const VALUE_OPTIONS: [&str; 6] = ["ref", "role", "to", "after", "in", "min"];
const FLAG_OPTIONS: [&str; 1] = ["all"];
const QUIET: [&str; 5] = ["me", "help", "board", "inbox", "wait"];

/// Answer to an agent command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// Whether the command succeeded.
    pub ok: bool,
    /// Text shown to the agent.
    pub text: String,
}

/// A member that is wanted in the swarm.
#[derive(Debug, Clone, Default)]
pub struct MemberSpec {
    /// Member name.
    pub name: String,
    /// Member role.
    pub role: String,
    /// Agent CLI.
    pub agent: String,
    /// Model chosen for the agent.
    pub model: String,
    /// Effort chosen for the agent.
    pub effort: String,
    /// Its first task; without it, the role's task.
    pub task: String,
    /// Works at the same time as the previous member.
    pub parallel: bool,
}

impl MemberSpec {
    /// Spec with the three required fields.
    pub fn new(name: &str, role: &str, agent: &str) -> Self {
        Self {
            name: name.to_string(),
            role: role.to_string(),
            agent: agent.to_string(),
            ..Self::default()
        }
    }
}

/// What EzAgent needs to seat an agent in the swarm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seat {
    /// Member name.
    pub name: String,
    /// Secret token; only its digest is stored.
    pub token: String,
    /// First instructions.
    pub briefing: String,
}

type Listener = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;
type ActivityListener = Arc<dyn Fn(&str, &str) + Send + Sync>;

struct Call {
    swarm: String,
    me: Member,
    words: Vec<String>,
    options: BTreeMap<String, String>,
}

impl Call {
    fn text(&self, skip: usize) -> String {
        self.words
            .iter()
            .skip(skip)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// `sha256(token).hexdigest()`.
fn digest(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Separates words and options (`--ref x`, `--ref=x`, `--all`).
fn parse(args: &[String]) -> Result<(Vec<String>, BTreeMap<String, String>), SwarmError> {
    let mut words = Vec::new();
    let mut options = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let item = args[index].as_str();
        index += 1;
        if !item.starts_with("--") || item == "--" {
            words.push(item.to_string());
            continue;
        }
        let (key, mut value) = match item[2..].split_once('=') {
            Some((key, value)) => (key, value.to_string()),
            None => (&item[2..], String::new()),
        };
        if FLAG_OPTIONS.contains(&key) {
            options.insert(key.to_string(), "1".to_string());
        } else if VALUE_OPTIONS.contains(&key) {
            if value.is_empty() && index < args.len() {
                value = args[index].clone();
                index += 1;
            }
            options.insert(key.to_string(), value);
        } else {
            return Err(SwarmError::new(format!("Opción desconocida: --{key}.")));
        }
    }
    Ok((words, options))
}

/// Initial tasks by stages: (title, role, owner).
fn stages_for(specs: &[MemberSpec]) -> Vec<Vec<(String, String, String)>> {
    let mut stages: Vec<Vec<(String, String, String)>> = Vec::new();
    for spec in specs {
        if stages.is_empty() || !spec.parallel {
            stages.push(Vec::new());
        }
        let stage = stages.last_mut().expect("una etapa recién creada");
        if !spec.task.is_empty() {
            let title = clip(&spec.task, MAX_TITLE).0;
            stage.push((title, spec.role.clone(), spec.name.clone()));
        } else if !stage
            .iter()
            .any(|(_, role, owner)| *role == spec.role && owner.is_empty())
        {
            stage.push((
                protocol::role_task(&spec.role),
                spec.role.clone(),
                String::new(),
            ));
        }
    }
    stages
        .into_iter()
        .filter(|stage| !stage.is_empty())
        .collect()
}

/// Single entry point for EzAgent (in-process) and the agents (by token).
pub struct SwarmService {
    /// Board used by the service.
    pub store: Db,
    /// Command name pasted into the agents.
    pub command: String,
    clock: Clock,
    news: Mutex<bool>,
    news_cv: Condvar,
    listeners: Mutex<Vec<Listener>>,
    activity: Mutex<Vec<ActivityListener>>,
}

impl SwarmService {
    /// Builds the service with an explicit clock.
    pub fn new(store: Db, command: &str, clock: Clock) -> Self {
        Self {
            store,
            command: command.to_string(),
            clock,
            news: Mutex::new(false),
            news_cv: Condvar::new(),
            listeners: Mutex::new(Vec::new()),
            activity: Mutex::new(Vec::new()),
        }
    }

    /// Builds the service with the default command and the system clock.
    pub fn with_defaults(store: Db) -> Self {
        Self::new(store, "ezs", Arc::new(ez_core::pytime::timestamp))
    }

    // ------------------------------------------------------ lado de EzAgent
    /// Creates a swarm. `tasks` are `(title, role)` and chain in order.
    pub fn start(
        &self,
        goal: &str,
        members: &[MemberSpec],
        cwd: &str,
        tasks: Option<&[(String, String)]>,
    ) -> DbResult<(String, Vec<Seat>)> {
        if members.is_empty() {
            return Err(SwarmError::new("Un enjambre necesita al menos un miembro.").into());
        }
        // In «setup» the supervisor neither opens agents nor issues tokens until
        // the swarm is complete.
        let swarm = self.store.create(goal, cwd, "setup")?;
        let seats = match self.populate(&swarm, members, tasks) {
            Ok(seats) => seats,
            Err(error) => {
                let _ = self.store.set_status(&swarm, "stopped");
                return Err(error);
            }
        };
        self.store.set_status(&swarm, "active")?;
        info!("Enjambre {} creado con {} miembros.", swarm, seats.len());
        Ok((swarm, seats))
    }

    fn populate(
        &self,
        swarm: &str,
        specs: &[MemberSpec],
        tasks: Option<&[(String, String)]>,
    ) -> DbResult<Vec<Seat>> {
        for spec in specs {
            self.store.join(
                swarm,
                &spec.name,
                &spec.role,
                &spec.agent,
                "",
                &spec.model,
                &spec.effort,
            )?;
        }
        let stages: Vec<Vec<(String, String, String)>> = match tasks {
            Some(tasks) => tasks
                .iter()
                .map(|(title, role)| vec![(title.clone(), role.clone(), String::new())])
                .collect(),
            None => stages_for(specs),
        };
        let mut previous: Vec<i64> = Vec::new();
        for stage in stages {
            let mut ids = Vec::new();
            for (title, role, owner) in stage {
                let task = self
                    .store
                    .add_task(swarm, &title, &role, &owner, &previous, 0)?;
                ids.push(task.id);
            }
            previous = ids;
        }
        let mut seats = Vec::new();
        for member in self.store.members(swarm)? {
            seats.push(Seat {
                name: member.name.clone(),
                token: self.issue_token(swarm, &member.name)?,
                briefing: self.briefing(swarm, &member.name)?,
            });
        }
        Ok(seats)
    }

    /// New token for a member; the previous one stops working.
    pub fn issue_token(&self, swarm: &str, member: &str) -> DbResult<String> {
        // DECISION: `secrets.token_urlsafe(24)` cannot be called without a new
        // dependency; two v4 UUIDs (OS randomness) give the same 32 bytes and
        // the same 43-character URL-safe format.
        let mut bytes = [0u8; 32];
        bytes[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        bytes[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        let token = base64_url(&bytes);
        self.store.set_token(swarm, member, &digest(&token))?;
        Ok(token)
    }

    /// Instructions of a member.
    pub fn briefing(&self, swarm: &str, member: &str) -> DbResult<String> {
        let info = self.store.get(swarm)?;
        let members = self.store.members(swarm)?;
        let me = members.iter().find(|item| item.name == member);
        match (info, me) {
            (Some(info), Some(me)) => Ok(protocol::briefing(&info, me, &members, &self.command)),
            _ => Err(SwarmError::new(format!(
                "{} no es miembro del enjambre {swarm}.",
                ez_core::pyvalue::repr_str(member)
            ))
            .into()),
        }
    }

    /// Health of the swarm.
    pub fn status(&self, swarm: &str) -> DbResult<Health> {
        health(&self.store, swarm, (self.clock)())
    }

    /// (messages that need attention, ready tasks) of a member, to wake it.
    pub fn pending(&self, swarm: &str, member: &str) -> DbResult<(i64, usize)> {
        let (unread, _, ready) = self.store.pending(swarm, member)?;
        Ok((unread, ready.len()))
    }

    /// User message (through EzAgent) to one member or everyone.
    pub fn tell(&self, swarm: &str, text: &str, to: &str) -> DbResult<()> {
        self.store.post(swarm, SYSTEM, text, to, "info", "")?;
        self.changed(swarm, SYSTEM, "tell");
        Ok(())
    }

    /// Most recent active swarm, or `""`.
    pub fn latest(&self) -> DbResult<String> {
        Ok(self
            .store
            .active()?
            .last()
            .map_or(String::new(), |info| info.id.clone()))
    }

    /// Final summary for voice.
    pub fn summary(&self, swarm: &str) -> DbResult<String> {
        summary(&self.store, swarm, (self.clock)())
    }

    /// Line to paste into the member's terminal, or `""` if there is nothing.
    pub fn wake_line(&self, swarm: &str, member: &str) -> DbResult<String> {
        let (unread, ready) = self.pending(swarm, member)?;
        if unread == 0 && ready == 0 {
            return Ok(String::new());
        }
        Ok(protocol::wake_line(unread, ready as i64, &self.command))
    }

    /// Line that seats a freshly opened agent.
    pub fn join_line(&self, swarm: &str, member: &str) -> DbResult<String> {
        let info = self.store.get(swarm)?;
        let members = self.store.members(swarm)?;
        let me = members.iter().find(|item| item.name == member);
        match (info, me) {
            (Some(info), Some(me)) => Ok(protocol::join_line(&info, me, &self.command)),
            _ => Err(SwarmError::new(format!(
                "{} no es miembro del enjambre {swarm}.",
                ez_core::pyvalue::repr_str(member)
            ))
            .into()),
        }
    }

    /// Stops everyone without losing anything.
    pub fn pause(&self, swarm: &str) -> DbResult<()> {
        self.store.set_status(swarm, "paused")?;
        self.changed(swarm, SYSTEM, "pause");
        Ok(())
    }

    /// Resumes a paused swarm.
    pub fn resume(&self, swarm: &str) -> DbResult<()> {
        if self
            .store
            .get(swarm)?
            .is_none_or(|info| info.status != "paused")
        {
            return Err(SwarmError::new("Ese enjambre no está en pausa.").into());
        }
        self.store.set_status(swarm, "active")?;
        self.changed(swarm, SYSTEM, "resume");
        Ok(())
    }

    /// `listener(swarm, member)` every time a member uses `ezs`.
    pub fn on_activity(&self, listener: impl Fn(&str, &str) + Send + Sync + 'static) {
        self.activity
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(Arc::new(listener));
    }

    fn seen(&self, swarm: &str, member: &str) {
        let listeners: Vec<ActivityListener> = self
            .activity
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        for listener in listeners {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| listener(swarm, member)));
            if result.is_err() {
                error!("Fallo en un oyente de actividad del enjambre");
            }
        }
    }

    /// Stops the swarm.
    pub fn stop(&self, swarm: &str) -> DbResult<()> {
        self.store.set_status(swarm, "stopped")?;
        self.changed(swarm, SYSTEM, "stop");
        Ok(())
    }

    /// `listener(swarm, member, command)` after every change. Must be fast.
    pub fn subscribe(&self, listener: impl Fn(&str, &str, &str) + Send + Sync + 'static) {
        self.listeners
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(Arc::new(listener));
    }

    /// Reopens the service.
    pub fn open(&self) {
        *self.news.lock().unwrap_or_else(|error| error.into_inner()) = false;
    }

    /// Frees the agents waiting in `wait`.
    pub fn close(&self) {
        *self.news.lock().unwrap_or_else(|error| error.into_inner()) = true;
        self.news_cv.notify_all();
    }

    // ------------------------------------------------- lado de los agentes
    /// `(swarm, member)` behind a token, or `None`.
    pub fn authenticate(&self, token: &str) -> DbResult<Option<(String, Member)>> {
        if token.is_empty() {
            return Ok(None);
        }
        self.store.by_token(&digest(token))
    }

    /// Runs a command of an agent identified by its token.
    pub fn handle(&self, token: &str, argv: &[String]) -> DbResult<Reply> {
        // Authentication happens outside the guarded block, as in Python.
        let Some((swarm, me)) = self.authenticate(token)? else {
            return Ok(Reply {
                ok: false,
                text: protocol::NO_SEAT.to_string(),
            });
        };
        let op = argv
            .first()
            .map_or_else(|| "next".to_string(), |word| word.to_lowercase());
        if !known_command(&op) {
            return Ok(Reply {
                ok: false,
                text: format!("Comando desconocido: {op}. Mira: {} help", self.command),
            });
        }
        let outcome = self.dispatch(&swarm, &me, &op, argv);
        match outcome {
            Ok((reply, notify)) => {
                if notify && !QUIET.contains(&op.as_str()) {
                    self.changed(&swarm, &me.name, &op);
                }
                Ok(reply)
            }
            Err(error) => Ok(match error.as_swarm() {
                Some(swarm_error) => Reply {
                    ok: false,
                    text: swarm_error.to_string(),
                },
                None => {
                    error!("Fallo en el comando {} de {} ({})", op, me.name, swarm);
                    Reply {
                        ok: false,
                        text: "Error interno del tablero. Reintenta una vez.".to_string(),
                    }
                }
            }),
        }
    }

    fn dispatch(
        &self,
        swarm: &str,
        me: &Member,
        op: &str,
        argv: &[String],
    ) -> DbResult<(Reply, bool)> {
        if let Some(info) = self.store.get(swarm)? {
            if info.status == "stopped" {
                return Ok((
                    Reply {
                        ok: true,
                        text: protocol::STOPPED.to_string(),
                    },
                    false,
                ));
            }
            if info.status == "paused" {
                return Ok((
                    Reply {
                        ok: true,
                        text: protocol::PAUSED.to_string(),
                    },
                    false,
                ));
            }
        }
        self.store.touch(swarm, &me.name)?;
        self.seen(swarm, &me.name);
        let (words, options) = parse(&argv[1..])?;
        let call = Call {
            swarm: swarm.to_string(),
            me: me.clone(),
            words,
            options,
        };
        let text = match op {
            "next" => self.next(&call)?,
            "done" => self.done(&call)?,
            "fail" => self.fail(&call)?,
            "fix" => self.fix(&call)?,
            "add" => self.add(&call)?,
            "say" => self.say(&call, "info")?,
            "ask" => self.say(&call, "ask")?,
            "answer" => self.say(&call, "answer")?,
            "inbox" => self.inbox(&call)?,
            "lock" => self.lock_paths(&call)?,
            "unlock" => self.unlock(&call)?,
            "board" => board(&self.store, &call.swarm, &call.me.name)?,
            "me" => self.briefing(&call.swarm, &call.me.name)?,
            "help" => protocol::help_text(&self.command),
            "wait" => self.wait(&call)?,
            _ => unreachable!("known_command covers every handler"),
        };
        Ok((Reply { ok: true, text }, true))
    }

    // ------------------------------------------------------------- eventos
    fn changed(&self, swarm: &str, member: &str, op: &str) {
        {
            let _guard = self.news.lock().unwrap_or_else(|error| error.into_inner());
            self.news_cv.notify_all();
        }
        let listeners: Vec<Listener> = self
            .listeners
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        for listener in listeners {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listener(swarm, member, op)
            }));
            if result.is_err() {
                error!("Fallo en un oyente del enjambre {swarm}");
            }
        }
    }
}

#[cfg(test)]
mod tests;

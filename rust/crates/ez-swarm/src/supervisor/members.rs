//! Pasada por enjambre: despertar, actividad, perdidos y avisos.

use std::collections::HashMap;

use ez_core::pyvalue::{py_strip, text_or_empty, truthy};
use serde_json::Value;
use tracing::{info, warn};

use super::{
    agent_state, Key, Nudge, Signature, Supervisor, BLOCKED, BRIEF_RETRY_SECS, ERROR, EXITED, IDLE,
    MAX_LAUNCHES, MAX_NUDGES, MISSING, MISSING_TICKS, OPEN_TASK_SECS, RENUDGE_SECS, SHELLS,
    UNKNOWN, WORKING,
};
use crate::db::DbResult;
use crate::models::{Member, SwarmInfo};
use crate::protocol;

impl Supervisor {
    pub(super) fn tick_swarm(
        &self,
        info: &SwarmInfo,
        states: Option<&HashMap<String, String>>,
    ) -> DbResult<()> {
        for member in self.service.store.members(&info.id)? {
            self.tick_member(info, &member, states)?;
        }
        let tasks = self.service.store.tasks(&info.id)?;
        for text in self.progress.check(&info.id, &tasks) {
            (self.announce)(&text);
        }
        self.announce_issues(&info.id)
    }

    fn tick_member(
        &self,
        info: &SwarmInfo,
        member: &Member,
        states: Option<&HashMap<String, String>>,
    ) -> DbResult<()> {
        let key: Key = (info.id.clone(), member.name.clone());
        if member.state == "gone" {
            return Ok(());
        }
        if member.address.is_empty() {
            return self.launch_if_needed(info, member, &key);
        }
        let Some(states) = states else {
            return Ok(());
        };
        let state = match states.get(&member.address) {
            Some(state) => state.clone(),
            None => self.probe(&member.address),
        };
        if self.recovery.hold(info, member, state == WORKING)? {
            return Ok(()); // bajo un incidente: lo atiende la recuperación
        }
        if state != MISSING && state != EXITED {
            self.book.lock().missing.remove(&key);
        }
        if state == MISSING || state == EXITED {
            self.count_missing(info, member, &key, state == EXITED)
        } else {
            self.visible_state(info, member, &key, &state)
        }
    }

    /// Miembro sin terminal: se abre cuando tiene algo que hacer.
    fn launch_if_needed(&self, info: &SwarmInfo, member: &Member, key: &Key) -> DbResult<()> {
        let waiting = self.book.lock().launching.contains(key);
        let pending = self.service.store.pending(&info.id, &member.name)?;
        let has_work =
            !pending.2.is_empty() || !self.service.store.mine(&info.id, &member.name)?.is_empty();
        if !waiting && has_work {
            self.launch(info, member);
        }
        Ok(())
    }

    /// Una sola lectura no basta: el estado perdido se confirma en varias pasadas.
    fn count_missing(
        &self,
        info: &SwarmInfo,
        member: &Member,
        key: &Key,
        exited: bool,
    ) -> DbResult<()> {
        let count = {
            let mut state = self.book.lock();
            let count = state.missing.entry(key.clone()).or_insert(0);
            *count += 1;
            *count
        };
        if count >= MISSING_TICKS {
            self.book.lock().missing.remove(key);
            self.lost(info, member, exited)?;
        }
        Ok(())
    }

    fn visible_state(
        &self,
        info: &SwarmInfo,
        member: &Member,
        key: &Key,
        state: &str,
    ) -> DbResult<()> {
        if state == WORKING {
            self.service.store.touch(&info.id, &member.name)?;
            self.book.lock().idle_since.remove(key);
            if member.state == "blocked" {
                self.service
                    .store
                    .set_member_state(&info.id, &member.name, "working")?;
            }
        } else if state == BLOCKED || state == ERROR {
            if self.recovery.inspect(info, member, state == ERROR, true)? {
                return Ok(());
            }
            if member.state != "blocked" {
                self.service
                    .store
                    .set_member_state(&info.id, &member.name, "blocked")?;
            }
        } else if state == IDLE {
            // Libre con trabajo pendiente: antes de despertarlo, ¿se le acabó algo?
            let stuck = !self.service.store.mine(&info.id, &member.name)?.is_empty()
                && !self.book.lock().unbriefed.contains(key);
            if self.recovery.inspect(info, member, false, stuck)? {
                return Ok(());
            }
            self.nudge(info, member)?;
        }
        Ok(())
    }

    /// Estado de una terminal que no salió en el listado de agentes.
    fn probe(&self, address: &str) -> String {
        let found = match self.host.get_agent(address) {
            Ok(found) => found,
            Err(_) => return MISSING.to_string(), // no existe o Herdr no responde
        };
        let state = agent_state(found.get("state"));
        let mut app = text_or_empty(found.get("agent"));
        if app.is_empty() {
            app = text_or_empty(found.get("app"));
        }
        let app = py_strip(&app).to_lowercase();
        if state == UNKNOWN && SHELLS.contains(&app.strip_suffix(".exe").unwrap_or(&app)) {
            return EXITED.to_string(); // la terminal sigue, pero el programa salió
        }
        state.to_string()
    }

    /// El agente ya no está: se reabre una vez; si vuelve a pasar, se avisa.
    fn lost(&self, info: &SwarmInfo, member: &Member, exited: bool) -> DbResult<()> {
        let key: Key = (info.id.clone(), member.name.clone());
        self.book.lock().nudged.remove(&key);
        // Solo si el programa salió solo. Si el usuario cerró la pestaña, se respeta.
        let launches = self.book.lock().launches.get(&key).copied().unwrap_or(0);
        if exited && launches < MAX_LAUNCHES {
            warn!("Enjambre {}: {} se cerró; lo reabro.", info.id, member.name);
            self.service.store.set_address(&info.id, &member.name, "")?;
            return Ok(());
        }
        self.recovery.closed(info, member, exited)
    }

    /// Dice una vez cada aviso nuevo; si se resuelve y vuelve, lo repite.
    fn announce_issues(&self, swarm: &str) -> DbResult<()> {
        let found = self.service.status(swarm)?.found;
        let current: Vec<&str> = found.iter().map(|issue| issue.key.as_str()).collect();
        let mut fresh = Vec::new();
        {
            let mut state = self.book.lock();
            let said = state.announced.entry(swarm.to_string()).or_default();
            said.retain(|key| current.contains(&key.as_str()));
            for issue in &found {
                if issue.key.starts_with("incident:") || issue.key.starts_with("paused") {
                    continue; // la recuperación ya lo anunció con sus opciones
                }
                if said.insert(issue.key.clone()) {
                    fresh.push(issue.text.clone());
                }
            }
        }
        for text in fresh {
            (self.announce)(&format!("Enjambre: {text}"));
        }
        Ok(())
    }

    // ------------------------------------------------------------ despertar

    fn nudge(&self, info: &SwarmInfo, member: &Member) -> DbResult<()> {
        let key: Key = (info.id.clone(), member.name.clone());
        let now = (self.clock)();
        let Some((signature, line, wait)) = self.nudge_line(info, member, &key, now)? else {
            self.idle_again(info, member, &key)?;
            return Ok(());
        };
        let last = self.book.lock().nudged.get(&key).cloned();
        let count = match last {
            Some(last) if last.signature == signature => {
                if last.count >= MAX_NUDGES || now - last.at < wait {
                    return Ok(());
                }
                last.count + 1
            }
            _ => 1,
        };
        let sent = match self.host.send_task(&member.address, &line) {
            Ok(sent) => sent,
            Err(error) => {
                warn!("No pude avisar a {}: {}", member.name, error);
                return Ok(());
            }
        };
        if truthy(sent.get("ok").unwrap_or(&Value::Null)) {
            self.book.lock().nudged.insert(
                key,
                Nudge {
                    signature,
                    count,
                    at: now,
                },
            );
            info!("Enjambre {}: aviso {} a {}.", info.id, count, member.name);
        }
        Ok(())
    }

    /// Qué aviso toca y cada cuánto repetirlo; `None` si no hay nada que decir.
    fn nudge_line(
        &self,
        info: &SwarmInfo,
        member: &Member,
        key: &Key,
        now: f64,
    ) -> DbResult<Option<(Signature, String, f64)>> {
        let (unread, last_id, ready) = self.service.store.pending(&info.id, &member.name)?;
        let mut signature = Signature::Pending {
            last_id,
            ready: ready.clone(),
        };
        let mut line = String::new();
        let mut wait = RENUDGE_SECS;
        if self.book.lock().unbriefed.contains(key) {
            // Libre y sin haber usado `ezs`: se le dice (o repite) cómo empezar.
            signature = Signature::Brief;
            wait = BRIEF_RETRY_SECS;
            line = self.service.join_line(&info.id, &member.name)?;
        } else if unread != 0 || !ready.is_empty() {
            line = protocol::wake_line(unread, ready.len() as i64, &self.service.command);
        } else {
            let mine = self.task_ids(&info.id, &member.name)?;
            let since = {
                let mut state = self.book.lock();
                *state.idle_since.entry(key.clone()).or_insert(now)
            };
            if !mine.is_empty() && now - since >= OPEN_TASK_SECS {
                signature = Signature::Open(mine.clone());
                line = protocol::open_task_line(&mine, &self.service.command);
            }
        }
        if line.is_empty() {
            return Ok(None);
        }
        Ok(Some((signature, line, wait)))
    }

    /// Sin línea que enviar: olvida el aviso y, si ya no tiene tareas, libéralo.
    fn idle_again(&self, info: &SwarmInfo, member: &Member, key: &Key) -> DbResult<()> {
        if self.service.store.mine(&info.id, &member.name)?.is_empty() {
            self.book.lock().idle_since.remove(key);
            if member.state == "blocked" && !self.has_failed(&info.id, &member.name)? {
                self.service
                    .store
                    .set_member_state(&info.id, &member.name, "idle")?;
            }
        }
        self.book.lock().nudged.remove(key);
        Ok(())
    }

    fn task_ids(&self, swarm: &str, member: &str) -> DbResult<Vec<i64>> {
        Ok(self
            .service
            .store
            .mine(swarm, member)?
            .iter()
            .map(|task| task.id)
            .collect())
    }

    fn has_failed(&self, swarm: &str, member: &str) -> DbResult<bool> {
        Ok(self
            .service
            .store
            .tasks(swarm)?
            .iter()
            .any(|task| task.state == "failed" && task.owner == member))
    }
}

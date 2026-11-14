//! Port of `src/ezagent/swarm/store.py` (F3-12).
//!
//! Persistent board of one or more swarms: swarms, members, tasks, cursors and
//! file reservations. Every text written by an agent is validated and clipped
//! here; nothing is interpreted.

mod tasks;

use ez_core::pyvalue::repr_str;
use rusqlite::{params, OptionalExtension};

use crate::db::{member_from_row, Db, DbResult};
use crate::models::{
    clip, member_name, required, role_name, Member, SwarmError, SwarmInfo, BROADCAST, MAX_GOAL,
    MEMBER_STATES, SWARM_STATES, SYSTEM,
};

/// Swarm id length (`uuid4().hex[:8]`).
const SWARM_ID_LEN: usize = 8;

fn swarm_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SwarmInfo> {
    Ok(SwarmInfo {
        id: row.get("id")?,
        goal: row.get("goal")?,
        cwd: row.get("cwd")?,
        status: row.get("status")?,
        created: row.get("created")?,
        updated: row.get("updated")?,
    })
}

fn state_valid(state: &str) -> bool {
    SWARM_STATES.iter().any(|(name, _)| *name == state)
}

fn member_state_valid(state: &str) -> bool {
    MEMBER_STATES.iter().any(|(name, _)| *name == state)
}

impl Db {
    // ---------------------------------------------------------- enjambres
    /// Creates a swarm and returns its id.
    pub fn create(&self, goal: &str, cwd: &str, status: &str) -> DbResult<String> {
        let text = required(goal, MAX_GOAL, "el objetivo")?;
        if !state_valid(status) {
            return Err(SwarmError::new(format!(
                "Estado de enjambre desconocido: {}.",
                repr_str(status)
            ))
            .into());
        }
        let swarm_id = uuid::Uuid::new_v4().simple().to_string()[..SWARM_ID_LEN].to_string();
        let now = self.now();
        self.write(|db| {
            db.execute(
                "INSERT INTO swarm (id, goal, cwd, status, created, updated) \
                 VALUES (?, ?, ?, ?, ?, ?)",
                params![swarm_id, text, cwd, status, now, now],
            )?;
            Ok(())
        })?;
        Ok(swarm_id)
    }

    /// Swarm by id, or `None`.
    pub fn get(&self, swarm: &str) -> DbResult<Option<SwarmInfo>> {
        self.read(|db| {
            Ok(db
                .query_row(
                    "SELECT * FROM swarm WHERE id = ?",
                    params![swarm],
                    swarm_from_row,
                )
                .optional()?)
        })
    }

    /// Active swarms, oldest first.
    pub fn active(&self) -> DbResult<Vec<SwarmInfo>> {
        self.swarms("SELECT * FROM swarm WHERE status = 'active' ORDER BY created")
    }

    /// Active or paused swarms, oldest first.
    pub fn live(&self) -> DbResult<Vec<SwarmInfo>> {
        self.swarms("SELECT * FROM swarm WHERE status IN ('active', 'paused') ORDER BY created")
    }

    fn swarms(&self, query: &str) -> DbResult<Vec<SwarmInfo>> {
        self.read(|db| {
            let mut statement = db.prepare(query)?;
            let rows = statement
                .query_map([], swarm_from_row)?
                .collect::<rusqlite::Result<Vec<SwarmInfo>>>()?;
            Ok(rows)
        })
    }

    /// Id of the most recent swarm (active or not), or `""`.
    pub fn latest(&self) -> DbResult<String> {
        self.read(|db| {
            let id: Option<String> = db
                .query_row(
                    "SELECT id FROM swarm ORDER BY created DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(id.unwrap_or_default())
        })
    }

    /// Changes the swarm state.
    pub fn set_status(&self, swarm: &str, status: &str) -> DbResult<()> {
        if !state_valid(status) {
            return Err(SwarmError::new(format!(
                "Estado de enjambre desconocido: {}.",
                repr_str(status)
            ))
            .into());
        }
        let now = self.now();
        self.write(|db| {
            self.require_swarm(db, swarm)?;
            db.execute(
                "UPDATE swarm SET status = ?, updated = ? WHERE id = ?",
                params![status, now, swarm],
            )?;
            Ok(())
        })
    }

    // ----------------------------------------------------------- miembros
    /// Registers or updates a member (idempotent).
    #[allow(clippy::too_many_arguments)]
    pub fn join(
        &self,
        swarm: &str,
        name: &str,
        role: &str,
        agent: &str,
        address: &str,
        model: &str,
        effort: &str,
    ) -> DbResult<Member> {
        let member = member_name(name)?;
        if [SYSTEM, "all", "todos"].contains(&member.as_str()) {
            return Err(SwarmError::new(format!("«{member}» es un nombre reservado.")).into());
        }
        let agent_text = clip(agent, 40).0;
        let address_text = clip(address, 96).0;
        let model_text = clip(model, 80).0;
        let effort_text = clip(effort, 20).0;
        let now = self.now();
        self.write(|db| {
            self.require_swarm(db, swarm)?;
            let role_text = role_name(role)?;
            db.execute(
                "INSERT INTO member (swarm, name, role, agent, address, state, seen, model, \
                 effort) VALUES (?, ?, ?, ?, ?, 'idle', ?, ?, ?) \
                 ON CONFLICT(swarm, name) DO UPDATE SET role = excluded.role, \
                 agent = excluded.agent, address = excluded.address, seen = excluded.seen, \
                 model = excluded.model, effort = excluded.effort",
                params![
                    swarm,
                    member,
                    role_text,
                    agent_text,
                    address_text,
                    now,
                    model_text,
                    effort_text
                ],
            )?;
            self.require_member(db, swarm, &member)
        })
    }

    /// Members of the swarm, in join order.
    pub fn members(&self, swarm: &str) -> DbResult<Vec<Member>> {
        self.read(|db| {
            let mut statement =
                db.prepare("SELECT * FROM member WHERE swarm = ? ORDER BY rowid")?;
            let rows = statement
                .query_map(params![swarm], member_from_row)?
                .collect::<rusqlite::Result<Vec<Member>>>()?;
            Ok(rows)
        })
    }

    /// Sets the member state.
    pub fn set_member_state(&self, swarm: &str, name: &str, state: &str) -> DbResult<()> {
        if !member_state_valid(state) {
            return Err(SwarmError::new(format!(
                "Estado de miembro desconocido: {}.",
                repr_str(state)
            ))
            .into());
        }
        let now = self.now();
        self.write(|db| {
            self.require_member(db, swarm, name)?;
            db.execute(
                "UPDATE member SET state = ?, seen = ? WHERE swarm = ? AND name = ?",
                params![state, now, swarm, name],
            )?;
            Ok(())
        })
    }

    /// Changes the agent behind a member; its tasks stay with its name.
    pub fn update_member(
        &self,
        swarm: &str,
        name: &str,
        agent: &str,
        model: &str,
        effort: &str,
    ) -> DbResult<Member> {
        let agent_text = clip(agent, 40).0;
        let model_text = clip(model, 80).0;
        let effort_text = clip(effort, 20).0;
        let now = self.now();
        self.write(|db| {
            self.require_member(db, swarm, name)?;
            db.execute(
                "UPDATE member SET agent = ?, model = ?, effort = ?, address = '', \
                 state = 'idle', seen = ? WHERE swarm = ? AND name = ?",
                params![agent_text, model_text, effort_text, now, swarm, name],
            )?;
            self.require_member(db, swarm, name)
        })
    }

    /// Releases the member's claimed or pending tasks back to the role.
    pub fn release_tasks(&self, swarm: &str, name: &str) -> DbResult<Vec<i64>> {
        let now = self.now();
        self.write(|db| {
            self.require_member(db, swarm, name)?;
            let ids = {
                let mut statement = db.prepare(
                    "SELECT id FROM task WHERE swarm = ? AND owner = ? \
                     AND state IN ('claimed', 'pending')",
                )?;
                let rows = statement
                    .query_map(params![swarm, name], |row| row.get(0))?
                    .collect::<rusqlite::Result<Vec<i64>>>()?;
                rows
            };
            db.execute(
                "UPDATE task SET state = 'pending', owner = '', updated = ? \
                 WHERE swarm = ? AND owner = ? AND state IN ('claimed', 'pending')",
                params![now, swarm, name],
            )?;
            db.execute(
                "DELETE FROM lease WHERE swarm = ? AND member = ?",
                params![swarm, name],
            )?;
            Ok(ids)
        })
    }

    /// Terminal address of the member (`""` until it is opened).
    pub fn set_address(&self, swarm: &str, name: &str, address: &str) -> DbResult<()> {
        let address_text = clip(address, 96).0;
        let now = self.now();
        self.write(|db| {
            self.require_member(db, swarm, name)?;
            db.execute(
                "UPDATE member SET address = ?, seen = ? WHERE swarm = ? AND name = ?",
                params![address_text, now, swarm, name],
            )?;
            Ok(())
        })
    }

    /// What justifies waking a member: (messages, last id, ready tasks).
    pub fn pending(&self, swarm: &str, name: &str) -> DbResult<(i64, i64, Vec<i64>)> {
        self.read(|db| {
            let me = self.require_member(db, swarm, name)?;
            let (count, last): (i64, i64) = db.query_row(
                "SELECT COUNT(*), COALESCE(MAX(id), 0) FROM message WHERE swarm = ? \
                 AND sender != ? AND kind != 'sys' \
                 AND (recipient = ? OR (recipient = ? AND sender = ?)) \
                 AND id > COALESCE(\
                 (SELECT last FROM cursor WHERE swarm = ? AND member = ?), 0)",
                params![swarm, name, name, BROADCAST, SYSTEM, swarm, name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let ready = self
                .ready_tasks(db, swarm, &me)?
                .into_iter()
                .map(|task| task.id)
                .collect();
            Ok((count, last, ready))
        })
    }

    /// Heartbeat: the member gave signs of life.
    pub fn touch(&self, swarm: &str, name: &str) -> DbResult<()> {
        let now = self.now();
        self.write(|db| {
            db.execute(
                "UPDATE member SET seen = ? WHERE swarm = ? AND name = ?",
                params![now, swarm, name],
            )?;
            Ok(())
        })
    }

    /// Stores the digest (never the token) that identifies the member.
    pub fn set_token(&self, swarm: &str, name: &str, digest: &str) -> DbResult<()> {
        self.write(|db| {
            self.require_member(db, swarm, name)?;
            db.execute(
                "UPDATE member SET token = ?, briefed = 0 WHERE swarm = ? AND name = ?",
                params![digest, swarm, name],
            )?;
            Ok(())
        })
    }

    /// Marks that the member already received its instructions; true the first time.
    pub fn mark_briefed(&self, swarm: &str, name: &str) -> DbResult<bool> {
        self.write(|db| {
            let changed = db.execute(
                "UPDATE member SET briefed = 1 WHERE swarm = ? AND name = ? AND briefed = 0",
                params![swarm, name],
            )?;
            Ok(changed > 0)
        })
    }

    /// `(swarm, member)` behind a token digest, or `None`.
    pub fn by_token(&self, digest: &str) -> DbResult<Option<(String, Member)>> {
        if digest.is_empty() {
            return Ok(None);
        }
        self.read(|db| {
            let row: Option<(String, Member)> = db
                .query_row(
                    "SELECT * FROM member WHERE token = ?",
                    params![digest],
                    |row| {
                        let swarm: String = row.get("swarm")?;
                        Ok((swarm, member_from_row(row)?))
                    },
                )
                .optional()?;
            Ok(row)
        })
    }
}

#[cfg(test)]
mod tests;

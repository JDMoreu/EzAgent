//! Task operations of the board (`store.py`, tasks section).

use std::collections::HashSet;

use ez_core::pyjson::dumps;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use super::Db;
use crate::db::{task_from_row, DbResult};
use crate::models::{
    clip, required, role_name, SwarmError, Task, BROADCAST, MAX_BODY, MAX_REF, MAX_RESULT,
    MAX_ROUNDS, MAX_TITLE, SYSTEM,
};

impl Db {
    /// Creates a task; with `parent` it is a part of another task.
    #[allow(clippy::too_many_arguments)]
    pub fn add_task(
        &self,
        swarm: &str,
        title: &str,
        role: &str,
        owner: &str,
        deps: &[i64],
        parent: i64,
    ) -> DbResult<Task> {
        let text = required(title, MAX_TITLE, "el título de la tarea")?;
        let mut role_key = role_name(role)?;
        let mut owner = owner.to_string();
        let mut wanted: Vec<i64> = deps.to_vec();
        wanted.sort_unstable();
        wanted.dedup();
        let now = self.now();
        self.write(|db| {
            self.require_swarm(db, swarm)?;
            if !owner.is_empty() {
                self.require_member(db, swarm, &owner)?;
            }
            for dep in &wanted {
                self.task_row(db, swarm, *dep)?;
            }
            let above = if parent != 0 {
                Some(self.task_row(db, swarm, parent)?)
            } else {
                None
            };
            if let Some(above) = &above {
                resolve_part(above, &mut role_key, &mut owner)?;
            }
            db.execute(
                "INSERT INTO task (swarm, title, role, owner, deps, parent, created, updated) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    swarm,
                    text,
                    role_key,
                    owner,
                    dumps(&json!(wanted)),
                    parent,
                    now,
                    now
                ],
            )?;
            let created = db.last_insert_rowid();
            if let Some(above) = &above {
                if above.state != "pending" {
                    self.to_pending(db, swarm, above, Some(""))?;
                    if above.state == "done" {
                        self.reset_dependents(db, swarm, &HashSet::from([above.id]))?;
                    }
                }
            }
            self.task_row(db, swarm, created)
        })
    }

    /// All tasks of the swarm, by number.
    pub fn tasks(&self, swarm: &str) -> DbResult<Vec<Task>> {
        self.read(|db| {
            let mut statement = db.prepare("SELECT * FROM task WHERE swarm = ? ORDER BY id")?;
            let rows = statement
                .query_map(params![swarm], task_from_row)?
                .collect::<rusqlite::Result<Vec<Task>>>()?;
            Ok(rows)
        })
    }

    /// Tasks `member` could take right now (without taking them).
    pub fn ready(&self, swarm: &str, member: &str) -> DbResult<Vec<Task>> {
        self.read(|db| {
            let me = self.require_member(db, swarm, member)?;
            self.ready_tasks(db, swarm, &me)
        })
    }

    /// Tasks in progress under `member`.
    pub fn mine(&self, swarm: &str, member: &str) -> DbResult<Vec<Task>> {
        self.read(|db| {
            let mut statement = db.prepare(
                "SELECT * FROM task WHERE swarm = ? AND owner = ? AND state = 'claimed' \
                 ORDER BY id",
            )?;
            let rows = statement
                .query_map(params![swarm, member], task_from_row)?
                .collect::<rusqlite::Result<Vec<Task>>>()?;
            Ok(rows)
        })
    }

    /// Takes the next ready task (or every one with `every`).
    pub fn claim(
        &self,
        swarm: &str,
        member: &str,
        task_id: Option<i64>,
        every: bool,
    ) -> DbResult<Vec<Task>> {
        let now = self.now();
        self.write(|db| {
            let me = self.require_member(db, swarm, member)?;
            let mut ready = self.ready_tasks(db, swarm, &me)?;
            if let Some(wanted) = task_id {
                ready.retain(|task| task.id == wanted);
            }
            if !every {
                ready.truncate(1);
            }
            let chosen: Vec<i64> = ready.iter().map(|task| task.id).collect();
            for chosen_id in &chosen {
                db.execute(
                    "UPDATE task SET state = 'claimed', owner = ?, updated = ? WHERE id = ?",
                    params![member, now, chosen_id],
                )?;
            }
            if !chosen.is_empty() {
                self.sync_member(db, swarm, member, false)?;
            }
            chosen
                .into_iter()
                .map(|chosen_id| self.task_row(db, swarm, chosen_id))
                .collect()
        })
    }

    /// Closes one of the member's tasks and tells everyone in one line.
    pub fn finish(
        &self,
        swarm: &str,
        member: &str,
        task_id: i64,
        result: &str,
        ok: bool,
        reference: &str,
    ) -> DbResult<Task> {
        let summary = clip(result, MAX_RESULT).0;
        let ref_text = clip(reference, MAX_REF).0;
        let now = self.now();
        self.write(|db| {
            if member != SYSTEM {
                self.require_member(db, swarm, member)?;
            }
            let row = self.task_row(db, swarm, task_id)?;
            if member != SYSTEM {
                if ok && row.state == "done" && row.owner == member {
                    return Ok(row);
                }
                if row.state != "claimed" || row.owner != member {
                    return Err(SwarmError::new(format!(
                        "T{} no está en curso a tu nombre.",
                        row.id
                    ))
                    .into());
                }
            }
            db.execute(
                "UPDATE task SET state = ?, result = ?, updated = ? WHERE id = ?",
                params![if ok { "done" } else { "failed" }, summary, now, row.id],
            )?;
            let verb = if ok { "hecha" } else { "falló" };
            let fallback = if summary.is_empty() {
                row.title.clone()
            } else {
                summary.clone()
            };
            let body = clip(&format!("T{} {verb}: {fallback}", row.id), MAX_BODY).0;
            if ok && row.parent != 0 {
                self.rollup(db, swarm, row.parent, member)?;
            } else {
                self.insert_message(
                    db,
                    swarm,
                    member,
                    BROADCAST,
                    if ok { "done" } else { "block" },
                    &body,
                    &ref_text,
                )?;
            }
            if !row.owner.is_empty() {
                self.sync_member(db, swarm, &row.owner, !ok)?;
            }
            self.task_row(db, swarm, row.id)
        })
    }

    /// Returns a finished task to pending; true when it used every round.
    pub fn reopen(
        &self,
        swarm: &str,
        task_id: i64,
        by: &str,
        note: &str,
        reference: &str,
    ) -> DbResult<(Task, bool)> {
        let text = clip(note, MAX_RESULT).0;
        let ref_text = clip(reference, MAX_REF).0;
        let now = self.now();
        self.write(|db| {
            if by != SYSTEM {
                self.require_member(db, swarm, by)?;
            }
            let row = self.task_row(db, swarm, task_id)?;
            if row.state != "done" {
                return Err(SwarmError::new(format!(
                    "T{} no está terminada; escríbele a quien la lleva.",
                    row.id
                ))
                .into());
            }
            let rounds = row.rounds + 1;
            let mut affected = HashSet::from([row.id]);
            if rounds > MAX_ROUNDS {
                return self.exhaust_reopen(db, swarm, &row, &text, &ref_text, now);
            }
            db.execute(
                "UPDATE task SET rounds = ? WHERE id = ?",
                params![rounds, row.id],
            )?;
            let target = self.reopen_target(db, swarm, &row, &text)?;
            if row.parent != 0 {
                let above = self.task_row(db, swarm, row.parent)?;
                if above.state == "done" {
                    self.to_pending(db, swarm, &above, None)?;
                    affected.insert(above.id);
                }
            }
            self.reset_dependents(db, swarm, &affected)?;
            let fallback = if text.is_empty() {
                row.title.clone()
            } else {
                text.clone()
            };
            let body = clip(
                &format!("T{} reabierta ({rounds}/{MAX_ROUNDS}): {fallback}", row.id),
                MAX_BODY,
            )
            .0;
            let recipient = if target.is_empty() {
                BROADCAST.to_string()
            } else {
                target
            };
            self.insert_message(db, swarm, by, &recipient, "fix", &body, &ref_text)?;
            Ok((self.task_row(db, swarm, row.id)?, false))
        })
    }

    fn exhaust_reopen(
        &self,
        db: &Connection,
        swarm: &str,
        row: &Task,
        text: &str,
        ref_text: &str,
        now: f64,
    ) -> DbResult<(Task, bool)> {
        let reason = clip(
            &format!("Superó {MAX_ROUNDS} rondas de corrección: {text}"),
            MAX_RESULT,
        )
        .0;
        db.execute(
            "UPDATE task SET state = 'failed', result = ?, updated = ? WHERE id = ?",
            params![reason, now, row.id],
        )?;
        let body = format!(
            "T{} superó {MAX_ROUNDS} rondas de corrección; hace falta el usuario.",
            row.id
        );
        self.insert_message(db, swarm, SYSTEM, BROADCAST, "block", &body, ref_text)?;
        self.reset_dependents(db, swarm, &HashSet::from([row.id]))?;
        Ok((self.task_row(db, swarm, row.id)?, true))
    }

    /// Pending tasks `me` may take: its own first, then by number.
    pub(super) fn ready_tasks(
        &self,
        db: &Connection,
        swarm: &str,
        me: &crate::models::Member,
    ) -> DbResult<Vec<Task>> {
        let mut statement = db.prepare("SELECT * FROM task WHERE swarm = ? ORDER BY id")?;
        let rows = statement
            .query_map(params![swarm], task_from_row)?
            .collect::<rusqlite::Result<Vec<Task>>>()?;
        let done: HashSet<i64> = rows
            .iter()
            .filter(|task| task.state == "done")
            .map(|task| task.id)
            .collect();
        let parents: HashSet<i64> = rows
            .iter()
            .filter(|task| task.parent != 0)
            .map(|task| task.parent)
            .collect();
        let by_id: std::collections::HashMap<i64, &Task> =
            rows.iter().map(|task| (task.id, task)).collect();
        let mut ready: Vec<Task> = rows
            .iter()
            .filter(|task| {
                if task.state != "pending" || parents.contains(&task.id) {
                    return false;
                }
                if !task.owner.is_empty() {
                    if task.owner != me.name {
                        return false;
                    }
                } else if !task.role.is_empty() && task.role != me.role {
                    return false;
                }
                let mut deps: HashSet<i64> = task.deps.iter().copied().collect();
                if let Some(above) = by_id.get(&task.parent) {
                    deps.extend(above.deps.iter().copied());
                }
                deps.iter().all(|dep| done.contains(dep))
            })
            .cloned()
            .collect();
        ready.sort_by_key(|task| (task.owner != me.name, task.id));
        Ok(ready)
    }

    fn to_pending(
        &self,
        db: &Connection,
        swarm: &str,
        row: &Task,
        owner: Option<&str>,
    ) -> DbResult<()> {
        let target = owner.unwrap_or(&row.owner);
        db.execute(
            "UPDATE task SET state = 'pending', owner = ?, updated = ? WHERE id = ?",
            params![target, self.now(), row.id],
        )?;
        if !row.owner.is_empty() && row.state == "claimed" {
            self.sync_member(db, swarm, &row.owner, false)?;
        }
        Ok(())
    }

    /// Reopens the task (or adds a correction part); returns whom to tell.
    fn reopen_target(
        &self,
        db: &Connection,
        swarm: &str,
        row: &Task,
        note: &str,
    ) -> DbResult<String> {
        let has_parts = db
            .query_row(
                "SELECT 1 FROM task WHERE parent = ?",
                params![row.id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        let now = self.now();
        if has_parts {
            let fallback = if note.is_empty() {
                row.title.clone()
            } else {
                note.to_string()
            };
            let title = clip(&format!("Corregir: {fallback}"), MAX_TITLE).0;
            db.execute(
                "INSERT INTO task (swarm, title, role, deps, parent, created, updated) \
                 VALUES (?, ?, ?, '[]', ?, ?, ?)",
                params![swarm, title, row.role, row.id, now, now],
            )?;
            db.execute(
                "UPDATE task SET state = 'pending', owner = '', updated = ? WHERE id = ?",
                params![now, row.id],
            )?;
            return Ok(String::new());
        }
        db.execute(
            "UPDATE task SET state = 'pending', result = ?, updated = ? WHERE id = ?",
            params![note, now, row.id],
        )?;
        Ok(row.owner.clone())
    }

    /// What depended on reopened tasks goes back to pending (with its parts).
    fn reset_dependents(
        &self,
        db: &Connection,
        swarm: &str,
        changed: &HashSet<i64>,
    ) -> DbResult<()> {
        let mut statement = db.prepare("SELECT * FROM task WHERE swarm = ?")?;
        let rows = statement
            .query_map(params![swarm], task_from_row)?
            .collect::<rusqlite::Result<Vec<Task>>>()?;
        let mut seen = changed.clone();
        let mut queue: Vec<i64> = changed.iter().copied().collect();
        while let Some(current) = queue.pop() {
            for row in &rows {
                if seen.contains(&row.id) || !["claimed", "done"].contains(&row.state.as_str()) {
                    continue;
                }
                if !row.deps.contains(&current) {
                    continue;
                }
                self.to_pending(db, swarm, row, None)?;
                seen.insert(row.id);
                queue.push(row.id);
                for part in &rows {
                    if part.parent == row.id && !seen.contains(&part.id) {
                        if ["claimed", "done"].contains(&part.state.as_str()) {
                            self.to_pending(db, swarm, part, None)?;
                        }
                        seen.insert(part.id);
                    }
                }
            }
        }
        Ok(())
    }

    /// Closes the parent task when every part is done.
    fn rollup(&self, db: &Connection, swarm: &str, parent: i64, closer: &str) -> DbResult<()> {
        let mut statement = db.prepare("SELECT state FROM task WHERE parent = ?")?;
        let states = statement
            .query_map(params![parent], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        if states.is_empty() || states.iter().any(|state| state != "done") {
            return Ok(());
        }
        let result = format!("{} partes hechas", states.len());
        db.execute(
            "UPDATE task SET state = 'done', result = ?, updated = ? WHERE id = ?",
            params![result, self.now(), parent],
        )?;
        self.insert_message(
            db,
            swarm,
            closer,
            BROADCAST,
            "done",
            &format!("T{parent} hecha: {result}"),
            "",
        )?;
        Ok(())
    }

    /// Adjusts the member state depending on whether it has work in progress.
    fn sync_member(&self, db: &Connection, swarm: &str, name: &str, blocked: bool) -> DbResult<()> {
        let busy = db
            .query_row(
                "SELECT 1 FROM task WHERE swarm = ? AND owner = ? AND state = 'claimed'",
                params![swarm, name],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        let state = if busy {
            "working"
        } else if blocked {
            "blocked"
        } else {
            "idle"
        };
        if !busy {
            db.execute(
                "DELETE FROM lease WHERE swarm = ? AND member = ?",
                params![swarm, name],
            )?;
        }
        db.execute(
            "UPDATE member SET state = ?, seen = ? WHERE swarm = ? AND name = ?",
            params![state, self.now(), swarm, name],
        )?;
        Ok(())
    }
}

fn resolve_part(above: &Task, role_key: &mut String, owner: &mut String) -> DbResult<()> {
    if above.parent != 0 {
        return Err(SwarmError::new("Las partes solo tienen un nivel.").into());
    }
    if role_key.is_empty() {
        *role_key = above.role.clone();
    }
    if role_key.is_empty() && owner.is_empty() && above.state == "pending" {
        *owner = above.owner.clone();
    }
    Ok(())
}

//! Port of `src/ezagent/swarm/incident_db.py` (F3-11).
//!
//! Swarm incidents stored in the board (`incident` table). At most one active
//! incident per member: if the same agent fails again, the existing one is
//! updated instead of opening another.

use ez_core::pyvalue::repr_str;
use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::db::{Db, DbResult};
use crate::models::{clip, Incident, SwarmError, MAX_RESULT};

/// States that count as active (`ACTIVE`).
pub const ACTIVE: [&str; 2] = ["open", "waiting"];

/// Every incident state (`INCIDENT_STATES`).
pub const INCIDENT_STATES: [&str; 3] = ["open", "waiting", "resolved"];

fn incident_from_row(row: &Row<'_>) -> rusqlite::Result<Incident> {
    Ok(Incident {
        id: row.get("id")?,
        swarm: row.get("swarm")?,
        member: row.get("member")?,
        kind: row.get("kind")?,
        detail: row.get("detail")?,
        resets: row.get("resets")?,
        state: row.get("state")?,
        action: row.get("action")?,
        created: row.get("created")?,
        updated: row.get("updated")?,
    })
}

impl Db {
    /// Opens (or updates) the member's active incident; `true` when it is new.
    pub fn open_incident(
        &self,
        swarm: &str,
        member: &str,
        kind: &str,
        detail: &str,
        resets: f64,
    ) -> DbResult<(Incident, bool)> {
        let (text, _) = clip(detail, MAX_RESULT);
        let now = self.now();
        let state = if resets > 0.0 { "waiting" } else { "open" };
        self.write(|db| {
            self.require_member(db, swarm, member)?;
            let row = db
                .query_row(
                    "SELECT * FROM incident WHERE swarm = ? AND member = ? AND state IN (?, ?) \
                     ORDER BY id DESC LIMIT 1",
                    params![swarm, member, ACTIVE[0], ACTIVE[1]],
                    incident_from_row,
                )
                .optional()?;
            if let Some(row) = row {
                let same = row.kind == kind;
                let kept_resets = if resets != 0.0 {
                    resets
                } else if same {
                    row.resets
                } else {
                    0.0
                };
                db.execute(
                    "UPDATE incident SET kind = ?, detail = ?, resets = ?, updated = ? \
                     WHERE id = ?",
                    params![kind, text, kept_resets, now, row.id],
                )?;
                return Ok((self.incident_row(db, row.id)?, !same));
            }
            db.execute(
                "INSERT INTO incident (swarm, member, kind, detail, resets, state, created, updated) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![swarm, member, kind, text, resets, state, now, now],
            )?;
            Ok((self.incident_row(db, db.last_insert_rowid())?, true))
        })
    }

    /// Incident by id, or `None`.
    pub fn incident(&self, incident_id: i64) -> DbResult<Option<Incident>> {
        self.read(|db| {
            Ok(db
                .query_row(
                    "SELECT * FROM incident WHERE id = ?",
                    params![incident_id],
                    incident_from_row,
                )
                .optional()?)
        })
    }

    /// Incidents of the swarm; only the active ones by default.
    pub fn incidents(&self, swarm: &str, active: bool) -> DbResult<Vec<Incident>> {
        self.read(|db| {
            let incidents = if active {
                let mut statement = db.prepare(
                    "SELECT * FROM incident WHERE swarm = ? AND state IN (?, ?) ORDER BY id",
                )?;
                let incidents = statement
                    .query_map(params![swarm, ACTIVE[0], ACTIVE[1]], |row| {
                        incident_from_row(row)
                    })?
                    .collect::<rusqlite::Result<Vec<Incident>>>()?;
                incidents
            } else {
                let mut statement =
                    db.prepare("SELECT * FROM incident WHERE swarm = ? ORDER BY id")?;
                let incidents = statement
                    .query_map(params![swarm], incident_from_row)?
                    .collect::<rusqlite::Result<Vec<Incident>>>()?;
                incidents
            };
            Ok(incidents)
        })
    }

    /// Active incident of the member, or `None`.
    pub fn member_incident(&self, swarm: &str, member: &str) -> DbResult<Option<Incident>> {
        self.read(|db| {
            Ok(db
                .query_row(
                    "SELECT * FROM incident WHERE swarm = ? AND member = ? AND state IN (?, ?) \
                     ORDER BY id DESC LIMIT 1",
                    params![swarm, member, ACTIVE[0], ACTIVE[1]],
                    incident_from_row,
                )
                .optional()?)
        })
    }

    /// Changes the incident state, keeping the previous action when empty.
    pub fn set_incident(&self, incident_id: i64, state: &str, action: &str) -> DbResult<Incident> {
        if !INCIDENT_STATES.contains(&state) {
            return Err(SwarmError::new(format!(
                "Estado de incidente desconocido: {}.",
                repr_str(state)
            ))
            .into());
        }
        let now = self.now();
        self.write(|db| {
            let row = self.incident_row(db, incident_id)?;
            let (text, _) = clip(action, 40);
            let action = if text.is_empty() { row.action } else { text };
            db.execute(
                "UPDATE incident SET state = ?, action = ?, updated = ? WHERE id = ?",
                params![state, action, now, row.id],
            )?;
            self.incident_row(db, row.id)
        })
    }

    fn incident_row(&self, db: &Connection, incident_id: i64) -> DbResult<Incident> {
        match db
            .query_row(
                "SELECT * FROM incident WHERE id = ?",
                params![incident_id],
                incident_from_row,
            )
            .optional()?
        {
            Some(incident) => Ok(incident),
            None => Err(SwarmError::new(format!("No existe el incidente I{incident_id}.")).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::{seed, temp_board};

    #[test]
    fn opens_updates_and_preserves_the_active_incident() {
        let board = temp_board(10.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        let (first, is_new) = board
            .db
            .open_incident("s1", "ana", "quota", "sin saldo", 0.0)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(is_new);
        assert_eq!(first.state, "open");
        assert_eq!(first.id, 1);
        assert_eq!(first.created, 10.0);
        assert_eq!(
            board
                .db
                .open_incident("s1", "nadie", "quota", "", 0.0)
                .unwrap_err()
                .to_string(),
            "'nadie' no es miembro del enjambre s1."
        );

        let (same, is_new) = board
            .db
            .open_incident("s1", "ana", "quota", "otra vez", 0.0)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(!is_new);
        assert_eq!(same.id, 1);
        assert_eq!(same.detail, "otra vez");

        let (different, is_new) = board
            .db
            .open_incident("s1", "ana", "context", "compactó", 0.0)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(is_new);
        assert_eq!(different.id, 1);
        assert_eq!(different.resets, 0.0);
    }

    #[test]
    fn resets_choose_waiting_and_are_kept_for_the_same_kind() {
        let board = temp_board(100.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        let (waiting, _) = board
            .db
            .open_incident("s1", "bo", "quota", "x".repeat(500).as_str(), 120.0)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(waiting.state, "waiting");
        assert_eq!(waiting.resets, 120.0);
        assert_eq!(waiting.detail.chars().count(), MAX_RESULT);

        let (kept, is_new) = board
            .db
            .open_incident("s1", "bo", "quota", "", 0.0)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(!is_new);
        assert_eq!(kept.resets, 120.0);
        assert_eq!(kept.detail, "");
    }

    #[test]
    fn queries_filter_active_and_member() {
        let board = temp_board(0.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        let (first, _) = board
            .db
            .open_incident("s1", "ana", "quota", "a", 0.0)
            .unwrap_or_else(|error| panic!("{error}"));
        board
            .db
            .open_incident("s1", "bo", "context", "b", 0.0)
            .unwrap_or_else(|error| panic!("{error}"));
        board
            .db
            .set_incident(first.id, "resolved", "reintentar")
            .unwrap_or_else(|error| panic!("{error}"));

        assert_eq!(
            board
                .db
                .incidents("s1", true)
                .unwrap_or_else(|error| panic!("{error}"))
                .len(),
            1
        );
        assert_eq!(
            board
                .db
                .incidents("s1", false)
                .unwrap_or_else(|error| panic!("{error}"))
                .len(),
            2
        );
        assert!(board
            .db
            .member_incident("s1", "ana")
            .unwrap_or_else(|error| panic!("{error}"))
            .is_none());
        assert_eq!(
            board
                .db
                .member_incident("s1", "bo")
                .unwrap_or_else(|error| panic!("{error}"))
                .unwrap_or_else(|| panic!("falta el incidente"))
                .kind,
            "context"
        );
        assert_eq!(
            board
                .db
                .incident(99)
                .unwrap_or_else(|error| panic!("{error}")),
            None
        );
    }

    #[test]
    fn set_incident_validates_state_and_keeps_action() {
        let board = temp_board(0.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        let (incident, _) = board
            .db
            .open_incident("s1", "ana", "quota", "", 0.0)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            board
                .db
                .set_incident(incident.id, "raro", "x")
                .unwrap_err()
                .to_string(),
            "Estado de incidente desconocido: 'raro'."
        );
        assert_eq!(
            board
                .db
                .set_incident(99, "open", "")
                .unwrap_err()
                .to_string(),
            "No existe el incidente I99."
        );
        let resolved = board
            .db
            .set_incident(incident.id, "resolved", "esperar al usuario")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(resolved.state, "resolved");
        assert_eq!(resolved.action, "esperar al usuario");
        let reopened = board
            .db
            .set_incident(incident.id, "waiting", "")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(reopened.action, "esperar al usuario");
        assert_eq!(
            board
                .db
                .open_incident("s1", "ana", "quota", "nueva", 0.0)
                .unwrap_or_else(|error| panic!("{error}"))
                .0
                .id,
            1
        );
    }
}

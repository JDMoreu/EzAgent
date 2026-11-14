//! Port of `src/ezagent/swarm/mail.py` (F3-11).
//!
//! Short messages with a per-member cursor, and expiring file reservations.

use ez_core::pyvalue::repr_str;
use rusqlite::{params, OptionalExtension, Row};

use crate::db::{message_from_row, Db, DbResult};
use crate::models::{
    clip, lease_path, member_name_as, overlaps, Lease, Message, SwarmError, BROADCAST, MAX_BODY,
    MAX_REF, MESSAGE_KINDS, SYSTEM,
};

/// Converts a `lease` row.
fn lease_from_row(row: &Row<'_>) -> rusqlite::Result<Lease> {
    Ok(Lease {
        path: row.get("path")?,
        member: row.get("member")?,
        expires: row.get("expires")?,
    })
}

impl Db {
    // ----------------------------------------------------------- mensajes
    /// Publishes a short message; returns it and whether it was clipped.
    ///
    /// `to`, `kind` and `reference` are the keyword-only arguments of Python.
    pub fn post(
        &self,
        swarm: &str,
        sender: &str,
        body: &str,
        to: &str,
        kind: &str,
        reference: &str,
    ) -> DbResult<(Message, bool)> {
        if !MESSAGE_KINDS.contains(&kind) {
            return Err(SwarmError::new(format!(
                "Tipo de mensaje desconocido: {}.",
                repr_str(kind)
            ))
            .into());
        }
        // DECISION: a long message is clipped instead of rejected; sending it
        // again would cost more tokens than losing the tail.
        let (text, truncated) = clip(body, MAX_BODY);
        if text.is_empty() {
            return Err(SwarmError::new("El mensaje está vacío.").into());
        }
        let (reference_text, _) = clip(reference, MAX_REF);
        self.write(|db| {
            if sender != SYSTEM {
                self.require_member(db, swarm, sender)?;
            } else {
                self.require_swarm(db, swarm)?;
            }
            let target = if to == BROADCAST {
                BROADCAST.to_string()
            } else {
                member_name_as(to, "destinatario")?
            };
            if target != BROADCAST {
                self.require_member(db, swarm, &target)?;
            }
            let message =
                self.insert_message(db, swarm, sender, &target, kind, &text, &reference_text)?;
            Ok((message, truncated))
        })
    }

    /// Unread messages for `member` (direct and broadcast), excluding its own.
    pub fn inbox(
        &self,
        swarm: &str,
        member: &str,
        mark: bool,
        limit: i64,
    ) -> DbResult<Vec<Message>> {
        self.write(|db| {
            self.require_member(db, swarm, member)?;
            let last: i64 = db
                .query_row(
                    "SELECT last FROM cursor WHERE swarm = ? AND member = ?",
                    params![swarm, member],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or(0);
            let mut statement = db.prepare(
                "SELECT * FROM message WHERE swarm = ? AND id > ? AND sender != ? \
                 AND recipient IN (?, ?) ORDER BY id LIMIT ?",
            )?;
            let messages = statement
                .query_map(
                    params![swarm, last, member, member, BROADCAST, limit.clamp(1, 100)],
                    message_from_row,
                )?
                .collect::<rusqlite::Result<Vec<Message>>>()?;
            if mark {
                if let Some(last_message) = messages.last() {
                    db.execute(
                        "INSERT INTO cursor (swarm, member, last) VALUES (?, ?, ?) \
                         ON CONFLICT(swarm, member) DO UPDATE SET last = excluded.last",
                        params![swarm, member, last_message.id],
                    )?;
                }
            }
            Ok(messages)
        })
    }

    /// Number of unread messages for `member`.
    pub fn unread(&self, swarm: &str, member: &str) -> DbResult<i64> {
        self.read(|db| {
            Ok(db.query_row(
                "SELECT COUNT(*) FROM message WHERE swarm = ? AND sender != ? \
                 AND recipient IN (?, ?) AND id > COALESCE(\
                 (SELECT last FROM cursor WHERE swarm = ? AND member = ?), 0)",
                params![swarm, member, member, BROADCAST, swarm, member],
                |row| row.get(0),
            )?)
        })
    }

    /// Last messages of the swarm, oldest first.
    pub fn history(&self, swarm: &str, since: i64, limit: i64) -> DbResult<Vec<Message>> {
        self.read(|db| {
            let mut statement = db.prepare(
                "SELECT * FROM (SELECT * FROM message WHERE swarm = ? AND id > ? \
                 ORDER BY id DESC LIMIT ?) ORDER BY id",
            )?;
            let messages = statement
                .query_map(params![swarm, since, limit.clamp(1, 500)], |row| {
                    message_from_row(row)
                })?
                .collect::<rusqlite::Result<Vec<Message>>>()?;
            Ok(messages)
        })
    }

    // ----------------------------------------------------------- reservas
    /// Reserves paths (all or none); returns the collisions with other members.
    pub fn reserve(
        &self,
        swarm: &str,
        member: &str,
        paths: &[String],
        ttl: f64,
    ) -> DbResult<Vec<Lease>> {
        let mut wanted: Vec<String> = paths
            .iter()
            .map(|path| lease_path(path.as_str()))
            .collect::<Result<_, SwarmError>>()?;
        wanted.sort();
        wanted.dedup();
        if wanted.is_empty() {
            return Err(SwarmError::new("No indicaste rutas para reservar.").into());
        }
        let now = self.now();
        self.write(|db| {
            self.require_member(db, swarm, member)?;
            db.execute(
                "DELETE FROM lease WHERE swarm = ? AND expires <= ?",
                params![swarm, now],
            )?;
            let others = {
                let mut statement =
                    db.prepare("SELECT * FROM lease WHERE swarm = ? AND member != ?")?;
                let leases = statement
                    .query_map(params![swarm, member], lease_from_row)?
                    .collect::<rusqlite::Result<Vec<Lease>>>()?;
                leases
            };
            let conflicts: Vec<Lease> = others
                .into_iter()
                .filter(|lease| wanted.iter().any(|path| overlaps(&lease.path, path)))
                .collect();
            if !conflicts.is_empty() {
                return Ok(conflicts);
            }
            let expires = now + ttl.max(30.0);
            for path in &wanted {
                db.execute(
                    "INSERT INTO lease (swarm, path, member, expires) VALUES (?, ?, ?, ?) \
                     ON CONFLICT(swarm, path) DO UPDATE SET member = excluded.member, \
                     expires = excluded.expires",
                    params![swarm, path, member, expires],
                )?;
            }
            Ok(Vec::new())
        })
    }

    /// Releases every lease of `member`, or only `paths` when given.
    pub fn release(&self, swarm: &str, member: &str, paths: Option<&[String]>) -> DbResult<i64> {
        let normalized = match paths {
            Some(paths) => Some(
                paths
                    .iter()
                    .map(|path| lease_path(path.as_str()))
                    .collect::<Result<Vec<String>, SwarmError>>()?,
            ),
            None => None,
        };
        self.write(|db| {
            let mut removed: i64 = 0;
            match &normalized {
                None => {
                    removed = db.execute(
                        "DELETE FROM lease WHERE swarm = ? AND member = ?",
                        params![swarm, member],
                    )? as i64;
                }
                Some(paths) => {
                    for path in paths {
                        removed += db.execute(
                            "DELETE FROM lease WHERE swarm = ? AND member = ? AND path = ?",
                            params![swarm, member, path],
                        )? as i64;
                    }
                }
            }
            Ok(removed.max(0))
        })
    }

    /// Leases of the swarm that have not expired, ordered by path.
    pub fn leases(&self, swarm: &str) -> DbResult<Vec<Lease>> {
        let now = self.now();
        self.read(|db| {
            let mut statement =
                db.prepare("SELECT * FROM lease WHERE swarm = ? AND expires > ? ORDER BY path")?;
            let leases = statement
                .query_map(params![swarm, now], lease_from_row)?
                .collect::<rusqlite::Result<Vec<Lease>>>()?;
            Ok(leases)
        })
    }
}

#[cfg(test)]
mod tests {
    use ez_core::pyvalue::char_len;

    use super::*;
    use crate::db::test_support::{seed, temp_board};
    use crate::models::{DEFAULT_LEASE_SECS, MAX_INBOX};

    #[test]
    fn post_clips_and_validates() {
        let board = temp_board(10.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        let (message, clipped) = board
            .db
            .post("s1", "ana", "hola a todos", "*", "info", "")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(message.body, "hola a todos");
        assert_eq!(message.recipient, "*");
        assert_eq!(message.ts, 10.0);
        assert!(!clipped);

        let (direct, _) = board
            .db
            .post("s1", "ana", "para bo", "bo", "ask", "a.md")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(direct.recipient, "bo");
        assert_eq!(direct.ref_, "a.md");

        let long = "x".repeat(400);
        let (clipped_message, clipped) = board
            .db
            .post("s1", "ana", &long, "*", "info", "")
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(clipped);
        assert_eq!(char_len(&clipped_message.body), MAX_BODY);
        assert!(clipped_message.body.ends_with('…'));

        let (system, _) = board
            .db
            .post("s1", SYSTEM, "aviso", "*", "sys", "")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(system.sender, SYSTEM);
    }

    #[test]
    fn post_validates_kind_body_sender_and_target() {
        let board = temp_board(0.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        for kind in ["raro", "", "INFO"] {
            let error = board.db.post("s1", "ana", "x", "*", kind, "").unwrap_err();
            assert!(error.is_swarm());
            assert_eq!(
                error.to_string(),
                format!("Tipo de mensaje desconocido: '{}'.", kind)
            );
        }
        assert_eq!(
            board
                .db
                .post("s1", "ana", "", "*", "info", "")
                .unwrap_err()
                .to_string(),
            "El mensaje está vacío."
        );
        assert_eq!(
            board
                .db
                .post("s1", "nadie", "x", "*", "info", "")
                .unwrap_err()
                .to_string(),
            "'nadie' no es miembro del enjambre s1."
        );
        assert_eq!(
            board
                .db
                .post("s1", "ana", "x", "a b", "info", "")
                .unwrap_err()
                .to_string(),
            "El destinatario 'a b' no es válido: usa a-z, 0-9, «-» o «_»."
        );
    }

    #[test]
    fn inbox_advances_cursor_only_when_marking() {
        let board = temp_board(0.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        board
            .db
            .post("s1", "ana", "para bo", "bo", "info", "")
            .unwrap_or_else(|error| panic!("{error}"));
        board
            .db
            .post("s1", "ana", "a todos", "*", "info", "")
            .unwrap_or_else(|error| panic!("{error}"));
        board
            .db
            .post("s1", "bo", "propio", "*", "info", "")
            .unwrap_or_else(|error| panic!("{error}"));

        let peek = board
            .db
            .inbox("s1", "bo", false, MAX_INBOX as i64)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(peek.len(), 2);
        assert_eq!(
            board
                .db
                .unread("s1", "bo")
                .unwrap_or_else(|error| panic!("{error}")),
            2
        );
        let marked = board
            .db
            .inbox("s1", "bo", true, 1)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(marked.len(), 1);
        assert_eq!(marked[0].body, "para bo");
        assert_eq!(
            board
                .db
                .unread("s1", "bo")
                .unwrap_or_else(|error| panic!("{error}")),
            1
        );
        let rest = board
            .db
            .inbox("s1", "bo", true, MAX_INBOX as i64)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(rest.len(), 1);
        assert!(board
            .db
            .inbox("s1", "bo", true, MAX_INBOX as i64)
            .unwrap_or_else(|error| panic!("{error}"))
            .is_empty());
    }

    #[test]
    fn history_returns_oldest_first() {
        let board = temp_board(0.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        for body in ["m0", "m1", "m2", "m3"] {
            board
                .db
                .post("s1", "ana", body, "*", "info", "")
                .unwrap_or_else(|error| panic!("{error}"));
        }
        let last_two = board
            .db
            .history("s1", 0, 2)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            last_two
                .iter()
                .map(|message| message.body.as_str())
                .collect::<Vec<_>>(),
            ["m2", "m3"]
        );
        let since = board
            .db
            .history("s1", 2, 50)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            since
                .iter()
                .map(|message| message.body.as_str())
                .collect::<Vec<_>>(),
            ["m2", "m3"]
        );
    }

    #[test]
    fn reserve_conflicts_ttl_and_expiry() {
        let board = temp_board(100.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            board
                .db
                .reserve("s1", "bo", &[], DEFAULT_LEASE_SECS)
                .unwrap_err()
                .to_string(),
            "No indicaste rutas para reservar."
        );
        let many = [
            "./src/a.rs".to_string(),
            "SRC\\A.RS".to_string(),
            "src/b.rs".to_string(),
        ];
        assert!(board
            .db
            .reserve("s1", "bo", &many, DEFAULT_LEASE_SECS)
            .unwrap_or_else(|error| panic!("{error}"))
            .is_empty());
        let leases = board
            .db
            .leases("s1")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(leases.len(), 2);
        assert_eq!(leases[0].path, "src/a.rs");
        assert_eq!(leases[0].expires, 1000.0);

        let conflict = board
            .db
            .reserve("s1", "ana", &["src/a.rs".to_string()], DEFAULT_LEASE_SECS)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(conflict.len(), 1);
        assert_eq!(conflict[0].member, "bo");

        let directory = board
            .db
            .reserve("s1", "ana", &["src".to_string()], DEFAULT_LEASE_SECS)
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(directory.len(), 2);

        let short = board
            .db
            .reserve("s1", "ana", &["docs/x.md".to_string()], 5.0)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(short.is_empty());
        let lease = board
            .db
            .leases("s1")
            .unwrap_or_else(|error| panic!("{error}"))
            .into_iter()
            .find(|lease| lease.path == "docs/x.md")
            .unwrap_or_else(|| panic!("falta la reserva"));
        assert_eq!(lease.expires, 130.0);
    }

    #[test]
    fn expired_leases_are_deleted_before_reserving_and_released() {
        let board = temp_board(500.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        board
            .db
            .write(|db| {
                for path in ["src/a.rs", "src/b.rs"] {
                    db.execute(
                        "INSERT INTO lease (swarm, path, member, expires) VALUES ('s1', ?1, 'ana', 100.0)",
                        params![path],
                    )?;
                }
                Ok(())
            })
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(board
            .db
            .reserve("s1", "bo", &["src/a.rs".to_string()], DEFAULT_LEASE_SECS)
            .unwrap_or_else(|error| panic!("{error}"))
            .is_empty());
        assert_eq!(
            board
                .db
                .leases("s1")
                .unwrap_or_else(|error| panic!("{error}"))
                .len(),
            1
        );
        assert_eq!(
            board
                .db
                .release("s1", "bo", None)
                .unwrap_or_else(|error| panic!("{error}")),
            1
        );
        assert_eq!(
            board
                .db
                .release("s1", "bo", None)
                .unwrap_or_else(|error| panic!("{error}")),
            0
        );
    }

    #[test]
    fn release_by_path_normalizes_and_counts() {
        let board = temp_board(0.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        board
            .db
            .reserve(
                "s1",
                "bo",
                &["src/a.rs".to_string(), "src/b.rs".to_string()],
                DEFAULT_LEASE_SECS,
            )
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            board
                .db
                .release("s1", "bo", Some(&["./SRC/A.RS".to_string()]))
                .unwrap_or_else(|error| panic!("{error}")),
            1
        );
        assert_eq!(
            board
                .db
                .release("s1", "bo", Some(&["src/z.rs".to_string()]))
                .unwrap_or_else(|error| panic!("{error}")),
            0
        );
        assert_eq!(
            board
                .db
                .leases("s1")
                .unwrap_or_else(|error| panic!("{error}"))
                .len(),
            1
        );
    }
}

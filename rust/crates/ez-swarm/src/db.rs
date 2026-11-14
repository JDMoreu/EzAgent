//! Port of `src/ezagent/swarm/db.py` (F3-11).
//!
//! SQLite (WAL) persistence for the board. Writes use `BEGIN IMMEDIATE` plus a
//! process lock, so they stay atomic even when another process opens the same
//! database. The schema and PRAGMAs are byte-identical to the Python ones.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ez_core::config::python_int;
use ez_core::pytime;
use ez_core::pyvalue::repr_str;
use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::Value;

use crate::models::{Member, Message, SwarmError, Task};

/// Schema shared with Python (`_DDL`).
pub const DDL: &str = "
CREATE TABLE IF NOT EXISTS swarm (
    id TEXT PRIMARY KEY, goal TEXT NOT NULL, cwd TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'active', created REAL NOT NULL, updated REAL NOT NULL);
CREATE TABLE IF NOT EXISTS member (
    swarm TEXT NOT NULL, name TEXT NOT NULL, role TEXT NOT NULL DEFAULT '',
    agent TEXT NOT NULL DEFAULT '', address TEXT NOT NULL DEFAULT '',
    state TEXT NOT NULL DEFAULT 'idle', seen REAL NOT NULL,
    token TEXT NOT NULL DEFAULT '', model TEXT NOT NULL DEFAULT '',
    effort TEXT NOT NULL DEFAULT '', briefed INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (swarm, name));
CREATE INDEX IF NOT EXISTS idx_member_token ON member(token);
CREATE TABLE IF NOT EXISTS task (
    id INTEGER PRIMARY KEY AUTOINCREMENT, swarm TEXT NOT NULL, title TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT '', owner TEXT NOT NULL DEFAULT '',
    state TEXT NOT NULL DEFAULT 'pending', deps TEXT NOT NULL DEFAULT '[]',
    parent INTEGER NOT NULL DEFAULT 0, result TEXT NOT NULL DEFAULT '',
    rounds INTEGER NOT NULL DEFAULT 0, created REAL NOT NULL, updated REAL NOT NULL);
CREATE INDEX IF NOT EXISTS idx_task_swarm ON task(swarm, state);
CREATE TABLE IF NOT EXISTS message (
    id INTEGER PRIMARY KEY AUTOINCREMENT, swarm TEXT NOT NULL, sender TEXT NOT NULL,
    recipient TEXT NOT NULL, kind TEXT NOT NULL, body TEXT NOT NULL,
    ref TEXT NOT NULL DEFAULT '', ts REAL NOT NULL);
CREATE INDEX IF NOT EXISTS idx_message_swarm ON message(swarm, id);
CREATE TABLE IF NOT EXISTS cursor (
    swarm TEXT NOT NULL, member TEXT NOT NULL, last INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (swarm, member));
CREATE TABLE IF NOT EXISTS lease (
    swarm TEXT NOT NULL, path TEXT NOT NULL, member TEXT NOT NULL, expires REAL NOT NULL,
    PRIMARY KEY (swarm, path));
CREATE TABLE IF NOT EXISTS incident (
    id INTEGER PRIMARY KEY AUTOINCREMENT, swarm TEXT NOT NULL, member TEXT NOT NULL,
    kind TEXT NOT NULL, detail TEXT NOT NULL DEFAULT '', resets REAL NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'open', action TEXT NOT NULL DEFAULT '',
    created REAL NOT NULL, updated REAL NOT NULL);
CREATE INDEX IF NOT EXISTS idx_incident_swarm ON incident(swarm, state);
";

/// Clock used by the board (`time.time` in Python).
pub type Clock = Arc<dyn Fn() -> f64 + Send + Sync>;

/// Board failure: either a request error (`SwarmError`) or an internal one.
#[derive(Debug)]
pub enum DbError {
    /// Invalid request; its message is readable by the agent.
    Swarm(SwarmError),
    /// SQLite failure (`sqlite3.Error`).
    Sql(rusqlite::Error),
    /// Filesystem failure while creating the database directory.
    Io(std::io::Error),
}

impl DbError {
    /// True for the errors Python raises as `SwarmError`.
    pub fn is_swarm(&self) -> bool {
        matches!(self, Self::Swarm(_))
    }

    /// The `SwarmError` behind this failure, when it is a request error.
    pub fn as_swarm(&self) -> Option<&SwarmError> {
        match self {
            Self::Swarm(error) => Some(error),
            _ => None,
        }
    }
}

impl fmt::Display for DbError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Swarm(error) => error.fmt(formatter),
            Self::Sql(error) => error.fmt(formatter),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Swarm(error) => Some(error),
            Self::Sql(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

impl From<SwarmError> for DbError {
    fn from(error: SwarmError) -> Self {
        Self::Swarm(error)
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}

impl From<std::io::Error> for DbError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Result of a board operation.
pub type DbResult<T> = Result<T, DbError>;

/// Connections and common helpers of the board.
///
/// This is the base of `SwarmStore`: the store, mail and incident methods are
/// `impl Db` blocks in their own modules, so one type holds the whole board.
pub struct Db {
    path: PathBuf,
    clock: Clock,
    lock: Mutex<()>,
}

impl Db {
    /// Opens (or creates) the board at `path` with an explicit clock.
    pub fn new(path: impl Into<PathBuf>, clock: Clock) -> DbResult<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let connection = Connection::open(&path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA journal_mode=WAL")?;
        connection.execute_batch(DDL)?;
        drop(connection);
        Ok(Self {
            path,
            clock,
            lock: Mutex::new(()),
        })
    }

    /// Opens (or creates) the board with the system clock.
    pub fn open(path: impl Into<PathBuf>) -> DbResult<Self> {
        Self::new(path, Arc::new(pytime::timestamp))
    }

    /// Database file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Current time according to the board clock.
    pub fn now(&self) -> f64 {
        (self.clock)()
    }

    /// Runs a read closure on a fresh connection.
    pub fn read<T>(&self, run: impl FnOnce(&Connection) -> DbResult<T>) -> DbResult<T> {
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        run(&connection)
    }

    /// Runs a write closure inside `BEGIN IMMEDIATE`.
    pub fn write<T>(&self, run: impl FnOnce(&Connection) -> DbResult<T>) -> DbResult<T> {
        let guard = self
            .lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let connection = Connection::open(&self.path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = run(&connection);
        let result = match result {
            Ok(value) => match connection.execute_batch("COMMIT") {
                Ok(()) => Ok(value),
                Err(error) => Err(error.into()),
            },
            Err(error) => {
                // Python ignores a failing ROLLBACK.
                let _ = connection.execute_batch("ROLLBACK");
                Err(error)
            }
        };
        drop(guard);
        result
    }

    // ------------------------------------------------------- comprobaciones
    /// Fails with `SwarmError` when the swarm does not exist.
    pub fn require_swarm(&self, db: &Connection, swarm: &str) -> DbResult<()> {
        match db.query_row("SELECT 1 FROM swarm WHERE id = ?", params![swarm], |_| {
            Ok(())
        }) {
            Ok(()) => Ok(()),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(SwarmError::new(format!("No existe el enjambre {}.", repr_str(swarm))).into())
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Fails with `SwarmError` when the member does not belong to the swarm.
    pub fn require_member(&self, db: &Connection, swarm: &str, name: &str) -> DbResult<Member> {
        self.require_swarm(db, swarm)?;
        let row = db
            .query_row(
                "SELECT * FROM member WHERE swarm = ? AND name = ?",
                params![swarm, name],
                member_from_row,
            )
            .optional()?;
        match row {
            Some(member) => Ok(member),
            None => Err(SwarmError::new(format!(
                "{} no es miembro del enjambre {swarm}.",
                repr_str(name)
            ))
            .into()),
        }
    }

    /// Loads a task, failing with `SwarmError` when it does not exist.
    pub fn task_row(&self, db: &Connection, swarm: &str, task_id: i64) -> DbResult<Task> {
        let row = db
            .query_row(
                "SELECT * FROM task WHERE swarm = ? AND id = ?",
                params![swarm, task_id],
                task_from_row,
            )
            .optional()?;
        match row {
            Some(task) => Ok(task),
            None => Err(SwarmError::new(format!("No existe la tarea T{task_id}.")).into()),
        }
    }

    /// Inserts a message and bumps the swarm's `updated` timestamp.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_message(
        &self,
        db: &Connection,
        swarm: &str,
        sender: &str,
        recipient: &str,
        kind: &str,
        body: &str,
        reference: &str,
    ) -> DbResult<Message> {
        let now = self.now();
        db.execute(
            "INSERT INTO message (swarm, sender, recipient, kind, body, ref, ts) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![swarm, sender, recipient, kind, body, reference, now],
        )?;
        let id = db.last_insert_rowid();
        db.execute(
            "UPDATE swarm SET updated = ? WHERE id = ?",
            params![now, swarm],
        )?;
        Ok(Message {
            id,
            sender: sender.to_string(),
            recipient: recipient.to_string(),
            kind: kind.to_string(),
            body: body.to_string(),
            ref_: reference.to_string(),
            ts: now,
        })
    }
}

/// Converts a `member` row.
pub fn member_from_row(row: &Row<'_>) -> rusqlite::Result<Member> {
    Ok(Member {
        name: row.get("name")?,
        role: row.get("role")?,
        agent: row.get("agent")?,
        address: row.get("address")?,
        state: row.get("state")?,
        seen: row.get("seen")?,
        model: row.get("model")?,
        effort: row.get("effort")?,
    })
}

/// `json.loads` of a stored `deps` column, with SQLite's error channel.
fn decode_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
}

fn deps_error(message: &str) -> rusqlite::Error {
    let error = std::io::Error::new(std::io::ErrorKind::InvalidData, message.to_string());
    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
}

/// `int(dep)` for an element of the stored `deps` list.
fn dep_number(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(flag) => Some(i64::from(*flag)),
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|float| float.trunc() as i64)),
        Value::String(text) => python_int(text),
        _ => None,
    }
}

/// Converts a `task` row, decoding `deps` as Python does.
pub fn task_from_row(row: &Row<'_>) -> rusqlite::Result<Task> {
    let raw: String = row.get("deps")?;
    let text = if raw.is_empty() { "[]" } else { &raw };
    let deps = match serde_json::from_str::<Value>(text).map_err(decode_error)? {
        Value::Array(items) => items
            .iter()
            .map(dep_number)
            .collect::<Option<Vec<i64>>>()
            .ok_or_else(|| deps_error("deps con un número no válido"))?,
        _ => return Err(deps_error("deps no es una lista")),
    };
    Ok(Task {
        id: row.get("id")?,
        title: row.get("title")?,
        role: row.get("role")?,
        owner: row.get("owner")?,
        state: row.get("state")?,
        deps,
        parent: row.get("parent")?,
        result: row.get("result")?,
        rounds: row.get("rounds")?,
        updated: row.get("updated")?,
    })
}

/// Converts a `message` row.
pub fn message_from_row(row: &Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        id: row.get("id")?,
        sender: row.get("sender")?,
        recipient: row.get("recipient")?,
        kind: row.get("kind")?,
        body: row.get("body")?,
        ref_: row.get("ref")?,
        ts: row.get("ts")?,
    })
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;
    use std::sync::Arc;

    use rusqlite::params;

    use super::{Db, DbResult};

    /// Temporary board file removed when the test ends.
    pub struct TempBoard {
        pub db: Db,
        pub path: PathBuf,
    }

    impl Drop for TempBoard {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm"] {
                let mut name = self.path.as_os_str().to_os_string();
                name.push(suffix);
                let _ = std::fs::remove_file(PathBuf::from(name));
            }
        }
    }

    /// Creates a board in the system temp directory with a frozen clock.
    pub fn temp_board(now: f64) -> TempBoard {
        let path = std::env::temp_dir().join(format!(
            "ez-swarm-test-{}.db",
            uuid::Uuid::new_v4().simple()
        ));
        let db = Db::new(path.clone(), Arc::new(move || now)).expect("temporary board");
        TempBoard { db, path }
    }

    /// Inserts the swarm `s1` and the members `ana` and `bo`.
    pub fn seed(board: &Db) -> DbResult<()> {
        board.write(|db| {
            db.execute(
                "INSERT INTO swarm (id, goal, cwd, status, created, updated) \
                 VALUES ('s1', 'Migrar', '', 'active', 0.0, 0.0)",
                [],
            )?;
            for (name, role) in [("ana", "plan"), ("bo", "dev")] {
                db.execute(
                    "INSERT INTO member (swarm, name, role, state, seen) \
                     VALUES ('s1', ?1, ?2, 'idle', 0.0)",
                    params![name, role],
                )?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{seed, temp_board};
    use super::*;

    #[test]
    fn db_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Db>();
    }

    #[test]
    fn creates_the_python_schema_in_wal_mode() {
        let board = temp_board(0.0);
        let (mode, tables): (String, Vec<String>) = board
            .db
            .read(|db| {
                let mode: String = db.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
                let mut statement = db.prepare(
                    "SELECT name FROM sqlite_master WHERE type IN ('table', 'index') ORDER BY name",
                )?;
                let names = statement
                    .query_map([], |row| row.get(0))?
                    .collect::<rusqlite::Result<Vec<String>>>()?;
                Ok((mode, names))
            })
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(mode, "wal");
        for name in [
            "idx_incident_swarm",
            "idx_member_token",
            "idx_message_swarm",
            "idx_task_swarm",
            "cursor",
            "incident",
            "lease",
            "member",
            "message",
            "swarm",
            "task",
        ] {
            assert!(tables.iter().any(|item| item == name), "falta {name}");
        }
        let columns: Vec<String> = board
            .db
            .read(|db| {
                let mut statement = db.prepare("PRAGMA table_info(member)")?;
                let names = statement
                    .query_map([], |row| row.get(1))?
                    .collect::<rusqlite::Result<Vec<String>>>()?;
                Ok(names)
            })
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            columns,
            [
                "swarm", "name", "role", "agent", "address", "state", "seen", "token", "model",
                "effort", "briefed"
            ]
        );
    }

    #[test]
    fn require_helpers_use_the_python_messages() {
        let board = temp_board(0.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        let missing = board
            .db
            .read(|db| board.db.require_swarm(db, "nope"))
            .unwrap_err();
        assert_eq!(missing.to_string(), "No existe el enjambre 'nope'.");
        assert!(missing.is_swarm());
        let outsider = board
            .db
            .read(|db| board.db.require_member(db, "s1", "zz"))
            .unwrap_err();
        assert_eq!(outsider.to_string(), "'zz' no es miembro del enjambre s1.");
        let member = board
            .db
            .read(|db| board.db.require_member(db, "s1", "ana"))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(member.role, "plan");
        let task = board
            .db
            .read(|db| board.db.task_row(db, "s1", 9))
            .unwrap_err();
        assert_eq!(task.to_string(), "No existe la tarea T9.");
    }

    #[test]
    fn insert_message_bumps_swarm_and_converts_rows() {
        let board = temp_board(12.5);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        let message = board
            .db
            .write(|db| {
                let message = board
                    .db
                    .insert_message(db, "s1", "ana", "*", "info", "hola", "")?;
                Ok(message)
            })
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(message.id, 1);
        assert_eq!(message.ts, 12.5);
        let updated: f64 = board
            .db
            .read(|db| {
                Ok(
                    db.query_row("SELECT updated FROM swarm WHERE id = 's1'", [], |row| {
                        row.get(0)
                    })?,
                )
            })
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(updated, 12.5);
        let row = board
            .db
            .read(|db| {
                Ok(
                    db.query_row("SELECT * FROM message WHERE id = 1", [], |row| {
                        message_from_row(row)
                    })?,
                )
            })
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(row, message);
    }

    #[test]
    fn task_deps_are_decoded_as_python_ints() {
        let board = temp_board(0.0);
        seed(&board.db).unwrap_or_else(|error| panic!("{error}"));
        board
            .db
            .write(|db| {
                db.execute(
                    "INSERT INTO task (swarm, title, role, state, deps, created, updated) \
                     VALUES ('s1', 'Lexer', 'dev', 'pending', '[1, \"2\", true, 1.9]', 0.0, 0.0)",
                    [],
                )?;
                Ok(())
            })
            .unwrap_or_else(|error| panic!("{error}"));
        let task = board
            .db
            .read(|db| board.db.task_row(db, "s1", 1))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(task.deps, vec![1, 2, 1, 1]);
        assert_eq!(task.line(""), "T1 · Lexer [dev] tras T1,T2,T1,T1");

        board
            .db
            .write(|db| {
                db.execute(
                    "INSERT INTO task (swarm, title, role, state, deps, created, updated) \
                     VALUES ('s1', 'Rota', 'dev', 'pending', '[\"x\"]', 0.0, 0.0)",
                    [],
                )?;
                Ok(())
            })
            .unwrap_or_else(|error| panic!("{error}"));
        let broken = board
            .db
            .read(|db| board.db.task_row(db, "s1", 2))
            .unwrap_err();
        assert!(!broken.is_swarm());
    }
}

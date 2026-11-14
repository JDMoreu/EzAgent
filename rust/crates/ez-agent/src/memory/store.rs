//! Port of `src/ezagent/agent/memory.py 563-1102` (F3-09b): SQLite and history.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, FixedOffset, Local, Utc};
use ez_core::agent_config::AgentConfig;
use ez_core::pyvalue::py_strip;
use rusqlite::Connection;

use super::schema::RETENTION_DAYS;

const MESSAGES_DDL: &str = "
CREATE TABLE IF NOT EXISTS messages (
  id INTEGER PRIMARY KEY,
  ts TEXT NOT NULL,
  session TEXT NOT NULL,
  role TEXT NOT NULL,
  content TEXT NOT NULL
)";

const FTS_DDL: [&str; 3] = [
    "
    CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
      content, content='messages', content_rowid='id',
      tokenize='unicode61 remove_diacritics 2'
    )
    ",
    "
    CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
      INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
    END
    ",
    "
    CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
      INSERT INTO messages_fts(messages_fts, rowid, content)
      VALUES ('delete', old.id, old.content);
    END
    ",
];

/// `Callable[[], datetime]`: reloj inyectable para pruebas.
pub type Clock = Arc<dyn Fn() -> DateTime<FixedOffset> + Send + Sync>;

#[derive(Clone)]
pub struct MemoryOptions {
    pub session: String,
    pub prefer_fts: bool,
    pub retention_days: i64,
    pub clock: Option<Clock>,
}

impl Default for MemoryOptions {
    fn default() -> Self {
        Self {
            session: "default".into(),
            prefer_fts: true,
            retention_days: RETENTION_DAYS,
            clock: None,
        }
    }
}

impl MemoryOptions {
    pub fn clock(mut self, clock: Clock) -> Self {
        self.clock = Some(clock);
        self
    }
}

/// `ts` de `record`: instante, texto ya formateado o el reloj del almacén.
#[derive(Debug, Clone, Default)]
pub enum MemoryTimestamp {
    #[default]
    Now,
    DateTime(DateTime<FixedOffset>),
    Text(String),
}

fn db_error(error: rusqlite::Error) -> String {
    error.to_string()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn utc_iso(moment: DateTime<FixedOffset>) -> String {
    moment
        .with_timezone(&Utc)
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string()
}

fn object_exists(connection: &Connection, kind: &str, name: &str) -> Result<bool, String> {
    let mut statement = connection
        .prepare("SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2")
        .map_err(db_error)?;
    statement.exists([kind, name]).map_err(db_error)
}

/// `Path.read_text(encoding="utf-8", errors="replace")` con los fines de línea
/// traducidos como los traduce Python.
pub(crate) fn read_text(path: &std::path::Path) -> String {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return String::new(),
        Err(error) => {
            tracing::warn!("No se pudo leer {}: {error}", path.display());
            return String::new();
        }
    };
    String::from_utf8_lossy(&bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// Acceso a `USER.md`, `MEMORY.md` y `history.db`.
///
/// Cada operación abre una conexión SQLite efímera para no compartir estado
/// entre hilos. La comprobación de FTS5 se hace al construir la instancia.
pub struct MemoryStore {
    pub memory_dir: PathBuf,
    pub history_path: PathBuf,
    pub session: String,
    pub retention_days: i64,
    pub user_path: PathBuf,
    pub memory_path: PathBuf,
    pub fts_enabled: bool,
    pub fts_error: Option<String>,
    backend: Mutex<String>,
    clock: Clock,
}

impl MemoryStore {
    pub fn new(config: &AgentConfig) -> Result<Self, String> {
        Self::from_paths(
            &config.memory_dir,
            &config.history_path,
            MemoryOptions::default(),
        )
    }

    /// Constructor para pruebas y para el comando de consola.
    pub fn from_paths(
        memory_dir: impl Into<PathBuf>,
        history_path: impl Into<PathBuf>,
        options: MemoryOptions,
    ) -> Result<Self, String> {
        let memory_dir = memory_dir.into();
        let history_path = history_path.into();
        std::fs::create_dir_all(&memory_dir).map_err(|error| error.to_string())?;
        if let Some(parent) = history_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
        }
        let user_path = memory_dir.join("USER.md");
        let memory_path = memory_dir.join("MEMORY.md");
        for path in [&user_path, &memory_path] {
            if !path.exists() {
                std::fs::write(path, "").map_err(|error| error.to_string())?;
            }
        }
        let clock = options
            .clock
            .unwrap_or_else(|| Arc::new(|| Local::now().fixed_offset()));
        let mut store = Self {
            memory_dir,
            history_path,
            session: if options.session.is_empty() {
                "default".into()
            } else {
                options.session
            },
            retention_days: options.retention_days.max(1),
            user_path,
            memory_path,
            fts_enabled: false,
            fts_error: None,
            backend: Mutex::new("none".into()),
            clock,
        };
        store.init_db(options.prefer_fts)?;
        store.purge_old(None)?;
        Ok(store)
    }

    pub(crate) fn connect(&self) -> Result<Connection, String> {
        let connection = Connection::open(&self.history_path).map_err(db_error)?;
        connection
            .busy_timeout(Duration::from_secs_f64(5.0))
            .map_err(db_error)?;
        Ok(connection)
    }

    fn init_db(&mut self, prefer_fts: bool) -> Result<(), String> {
        let connection = self.connect()?;
        connection.execute_batch(MESSAGES_DDL).map_err(db_error)?;
        connection
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_messages_ts ON messages(ts)",
                [],
            )
            .map_err(db_error)?;
        let had_fts = object_exists(&connection, "table", "messages_fts")?;
        let had_triggers = object_exists(&connection, "trigger", "messages_ai")?
            && object_exists(&connection, "trigger", "messages_ad")?;
        if !prefer_fts {
            // DECISION: el RuntimeWarning de Python se registra en el log.
            tracing::warn!("FTS5 desactivado; recall usará LIKE como respaldo.");
            return Ok(());
        }
        for statement in FTS_DDL {
            if let Err(error) = connection.execute_batch(statement) {
                // DECISION: sin FTS5 se retiran los triggers para que los INSERT
                // sigan funcionando; recall cae a LIKE (igual que el Python).
                let message = error.to_string();
                self.fts_error = Some(message.clone());
                tracing::warn!("SQLite no ofrece FTS5 ({message}); recall usará LIKE.");
                let _ = connection.execute_batch(
                    "DROP TRIGGER IF EXISTS messages_ai; DROP TRIGGER IF EXISTS messages_ad;",
                );
                return Ok(());
            }
        }
        self.fts_enabled = true;
        if !had_fts || !had_triggers {
            connection
                .execute(
                    "INSERT INTO messages_fts(messages_fts) VALUES('rebuild')",
                    [],
                )
                .map_err(db_error)?;
        }
        Ok(())
    }

    pub fn last_backend(&self) -> String {
        lock(&self.backend).clone()
    }

    pub(crate) fn set_last_backend(&self, value: &str) {
        *lock(&self.backend) = value.to_string();
    }

    pub(crate) fn clock_now(&self) -> DateTime<FixedOffset> {
        (self.clock)()
    }

    fn timestamp_iso(&self, ts: &MemoryTimestamp) -> String {
        match ts {
            MemoryTimestamp::Now => utc_iso(self.clock_now()),
            MemoryTimestamp::DateTime(moment) => utc_iso(*moment),
            MemoryTimestamp::Text(text) => py_strip(text).to_string(),
        }
    }

    /// Guarda un mensaje de usuario o asistente. Otros roles se ignoran.
    pub fn record(
        &self,
        role: &str,
        content: &str,
        session: Option<&str>,
        ts: &MemoryTimestamp,
    ) -> Result<bool, String> {
        let normalized_role = match py_strip(role).to_lowercase().as_str() {
            "user" | "usuario" => "user",
            "assistant" | "asistente" | "ai" | "bot" => "assistant",
            _ => {
                tracing::debug!("No se guarda en el historial el rol {role}");
                return Ok(false);
            }
        };
        if py_strip(content).is_empty() {
            return Ok(false);
        }
        let timestamp = self.timestamp_iso(ts);
        let session = session
            .filter(|value| !value.is_empty())
            .unwrap_or(&self.session);
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO messages (ts, session, role, content) VALUES (?1, ?2, ?3, ?4)",
                [
                    timestamp,
                    session.to_string(),
                    normalized_role.to_string(),
                    content.to_string(),
                ],
            )
            .map_err(db_error)?;
        // DECISION (del Python): se purga después de insertar para que ni
        // siquiera un mensaje importado con fecha caducada quede en el historial.
        self.purge_old(None)?;
        Ok(true)
    }

    /// Guarda un turno completo: primero el usuario y después el asistente.
    pub fn record_exchange(
        &self,
        user_text: &str,
        assistant_text: &str,
        session: Option<&str>,
        ts: &MemoryTimestamp,
    ) -> Result<(bool, bool), String> {
        let first = self.record("user", user_text, session, ts)?;
        let second = self.record("assistant", assistant_text, session, ts)?;
        Ok((first, second))
    }

    /// Borra mensajes anteriores al periodo de retención y devuelve cuántos.
    pub fn purge_old(&self, now: Option<DateTime<FixedOffset>>) -> Result<i64, String> {
        let current = now.unwrap_or_else(|| self.clock_now()).with_timezone(&Utc);
        let cutoff = (current - ChronoDuration::days(self.retention_days))
            .format("%Y-%m-%dT%H:%M:%S+00:00")
            .to_string();
        let connection = self.connect()?;
        let deleted = connection
            .execute("DELETE FROM messages WHERE ts < ?1", [cutoff])
            .map_err(db_error)?;
        Ok(deleted as i64)
    }

    pub fn count_messages(&self) -> Result<i64, String> {
        let connection = self.connect()?;
        connection
            .query_row("SELECT COUNT(*) AS n FROM messages", [], |row| row.get(0))
            .map_err(db_error)
    }

    /// Borra todo el historial de conversaciones. No toca USER.md ni MEMORY.md.
    pub fn wipe(&self) -> Result<i64, String> {
        let connection = self.connect()?;
        let deleted = connection
            .execute("DELETE FROM messages", [])
            .map_err(db_error)?;
        if self.fts_enabled {
            connection
                .execute(
                    "INSERT INTO messages_fts(messages_fts) VALUES('rebuild')",
                    [],
                )
                .map_err(db_error)?;
        }
        Ok(deleted as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn fixed_clock(text: &str) -> Clock {
        let moment = DateTime::parse_from_rfc3339(text).expect("fecha válida");
        Arc::new(move || moment)
    }

    fn scratch(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("ez-memory-{}-{name}-{nanos}", std::process::id()))
    }

    fn store(name: &str) -> MemoryStore {
        let dir = scratch(name);
        let options = MemoryOptions::default().clock(fixed_clock("2026-11-14T05:07:23+00:00"));
        MemoryStore::from_paths(&dir, dir.join("history.db"), options).expect("almacén válido")
    }

    #[test]
    fn creates_files_and_tables() {
        let store = store("setup");
        assert!(store.user_path.is_file());
        assert!(store.memory_path.is_file());
        assert!(store.fts_enabled);
        assert_eq!(store.count_messages().expect("cuenta"), 0);
        assert_eq!(store.last_backend(), "none");
    }

    #[test]
    fn records_and_purges_history() {
        let store = store("record");
        assert!(store
            .record(
                "user",
                "hola",
                None,
                &MemoryTimestamp::DateTime(
                    DateTime::parse_from_rfc3339("2026-11-14T05:00:00+00:00")
                        .expect("fecha válida"),
                ),
            )
            .expect("guardado"));
        assert!(!store
            .record("tool", "x", None, &MemoryTimestamp::Now)
            .expect("ignorado"));
        assert_eq!(store.count_messages().expect("cuenta"), 1);
        let deleted = store
            .purge_old(Some(
                DateTime::parse_from_rfc3339("2027-06-01T00:00:00+00:00").expect("fecha válida"),
            ))
            .expect("purga");
        assert_eq!(deleted, 1);
    }

    #[test]
    fn drops_messages_older_than_retention_on_record() {
        let store = store("retention");
        store
            .record(
                "user",
                "viejo",
                None,
                &MemoryTimestamp::Text("2020-01-01T00:00:00+00:00".into()),
            )
            .expect("guardado");
        assert_eq!(store.count_messages().expect("cuenta"), 0);
    }

    #[test]
    fn wipes_the_history() {
        let store = store("wipe");
        store
            .record("assistant", "hola", None, &MemoryTimestamp::Now)
            .expect("guardado");
        assert_eq!(store.wipe().expect("borrado"), 1);
        assert_eq!(store.count_messages().expect("cuenta"), 0);
    }
}

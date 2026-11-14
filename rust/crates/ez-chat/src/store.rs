//! Port of `src/ezagent/voice/chat/store.py` (F3-14).
//!
//! Historial del chat: un archivo JSONL de solo añadir, legible y editable a
//! mano. El Python usa `chat.jsonl`; este port escribe el mismo esquema.
//!
//! DECISION: el brief menciona SQLite (`history.db`) pero el Python del baseline
//! (`voice/chat/store.py`) usa JSONL y así lo comprueban sus pruebas. Se porta
//! el JSONL, que es la especificación real; no se toca `Cargo.toml`.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pytime::timestamp;
use ez_core::pyvalue::{casefold, char_len, head_chars, py_rstrip, py_splitlines, py_strip};
use serde_json::{json, Value};

mod records;

pub use records::as_messages;
use records::{
    append_line, index_by_session, load, py_float, py_int, python_str, rewrite, round3, title,
};

pub const MAX_ENTRY_CHARS: usize = 200_000;
const VERSION: i64 = 1;

/// `Role` del historial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    User,
    Assistant,
    System,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// `Source` del historial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    Chat,
    Voice,
    Agent,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Voice => "voice",
            Self::Agent => "agent",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "chat" => Some(Self::Chat),
            "voice" => Some(Self::Voice),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

/// Un mensaje del historial.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatEntry {
    pub t: f64,
    pub session: i64,
    pub role: Role,
    pub source: Source,
    pub text: String,
    pub seq: i64,
}

/// Resumen de una conversación para la lista del historial.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionInfo {
    pub session: i64,
    pub started: f64,
    pub messages: usize,
    pub title: String,
}

/// Un mensaje en el formato del modelo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelMessage {
    pub role: Role,
    pub content: String,
}

/// `%APPDATA%/EzAgent/history/chat.jsonl`.
pub fn default_chat_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()))
        .map_or_else(|| ez_core::dirs::home().join(".config"), PathBuf::from);
    base.join("EzAgent").join("history").join("chat.jsonl")
}

#[derive(Default)]
struct State {
    entries: Vec<ChatEntry>,
    by_session: Vec<(i64, Vec<ChatEntry>)>,
    starts: BTreeMap<i64, f64>,
    activity: BTreeMap<i64, i64>,
    seq: i64,
    session: i64,
}

/// Historial persistente y seguro entre hilos.
pub struct ChatStore {
    path: PathBuf,
    state: Mutex<State>,
}

impl ChatStore {
    /// `ChatStore(path)`; `None` usa `default_chat_path()`.
    pub fn new(path: Option<PathBuf>) -> Self {
        let path = path.unwrap_or_else(default_chat_path);
        let mut state = State {
            session: 1,
            ..State::default()
        };
        load(&path, &mut state);
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // ---------------------------------------------------------------- carga
    fn ingest(state: &mut State, data: &Value, index: i64) -> Result<(), ()> {
        let session = py_int(data.get("session")).ok_or(())?;
        let seq = match data.get("seq") {
            Some(value) => py_int(Some(value)).unwrap_or(index),
            None => index,
        };
        state.seq = state.seq.max(seq);
        state
            .activity
            .entry(session)
            .and_modify(|current| *current = (*current).max(seq))
            .or_insert(seq);
        let t = match data.get("t") {
            Some(value) => py_float(Some(value)).ok_or(())?,
            None => 0.0,
        };
        state.starts.entry(session).or_insert(t);
        state.session = state.session.max(session);
        match data.get("event").and_then(Value::as_str) {
            Some("resume") => {
                state.session = session;
                return Ok(());
            }
            Some("session") => return Ok(()),
            _ => {}
        }
        let role_text = python_str(data.get("role").ok_or(())?);
        let role = Role::parse(&role_text).ok_or(())?;
        let source = data
            .get("source")
            .map(python_str)
            .as_deref()
            .and_then(Source::parse)
            .unwrap_or_default();
        let text = python_str(data.get("text").ok_or(())?);
        let entry = ChatEntry {
            t,
            session,
            role,
            source,
            text,
            seq,
        };
        state.entries.push(entry.clone());
        match state.by_session.iter_mut().find(|(key, _)| *key == session) {
            Some((_, items)) => items.push(entry),
            None => state.by_session.push((session, vec![entry])),
        }
        Ok(())
    }

    // ------------------------------------------------------------ escritura
    fn write(&self, record: &Value) {
        if let Err(error) = append_line(&self.path, record) {
            tracing::warn!("No pude guardar el historial del chat: {error}");
        }
    }

    fn next_seq(state: &mut State) -> i64 {
        state.seq += 1;
        state.seq
    }

    /// Añade un mensaje a la conversación actual y lo guarda.
    pub fn append(&self, role: Role, text: &str, source: Source, t: Option<f64>) -> ChatEntry {
        let clean = head_chars(text, MAX_ENTRY_CHARS).to_string();
        let mut state = self.lock();
        let entry = ChatEntry {
            t: t.unwrap_or_else(timestamp),
            session: state.session,
            role,
            source,
            text: clean.clone(),
            seq: Self::next_seq(&mut state),
        };
        state.starts.entry(entry.session).or_insert(entry.t);
        state.entries.push(entry.clone());
        match state
            .by_session
            .iter_mut()
            .find(|(key, _)| *key == entry.session)
        {
            Some((_, items)) => items.push(entry.clone()),
            None => state.by_session.push((entry.session, vec![entry.clone()])),
        }
        state.activity.insert(entry.session, entry.seq);
        self.write(&json!({
            "v": VERSION,
            "t": round3(entry.t),
            "session": entry.session,
            "role": role.as_str(),
            "source": source.as_str(),
            "text": clean,
            "seq": entry.seq,
        }));
        entry
    }

    /// Cambia el texto de la última entrada `role` de la conversación actual.
    pub fn replace_last(&self, role: Role, old: &str, new: &str) -> bool {
        let wanted = head_chars(old, MAX_ENTRY_CHARS);
        let clean = head_chars(new, MAX_ENTRY_CHARS);
        let mut state = self.lock();
        for index in (0..state.entries.len()).rev() {
            let entry = &state.entries[index];
            if entry.session != state.session {
                continue;
            }
            if entry.role != role {
                continue;
            }
            if entry.text != wanted {
                break;
            }
            let mut entries = state.entries.clone();
            entries[index].text = clean.to_string();
            let Some((rewritten, activity)) =
                rewrite(&self.path, &entries, &state.starts, state.session)
            else {
                return false;
            };
            state.entries = rewritten;
            state.activity = activity;
            state.by_session = index_by_session(&state.entries);
            state.seq = state.activity.values().copied().max().unwrap_or(0);
            return true;
        }
        false
    }

    /// Empieza otra conversación (las anteriores se conservan).
    pub fn new_session(&self) -> i64 {
        let mut state = self.lock();
        state.session += 1;
        let session = state.session;
        let now = timestamp();
        let seq = Self::next_seq(&mut state);
        state.starts.insert(session, now);
        state.activity.insert(session, seq);
        self.write(&json!({
            "v": VERSION,
            "t": round3(now),
            "event": "session",
            "session": session,
            "seq": seq,
        }));
        session
    }

    /// Retoma una conversación anterior como la actual.
    pub fn resume(&self, session: i64) -> bool {
        let mut state = self.lock();
        if session == state.session {
            return true;
        }
        if !state.by_session.iter().any(|(key, _)| *key == session) {
            return false;
        }
        state.session = session;
        let seq = Self::next_seq(&mut state);
        state.activity.insert(session, seq);
        self.write(&json!({
            "v": VERSION,
            "t": round3(timestamp()),
            "event": "resume",
            "session": session,
            "seq": seq,
        }));
        true
    }

    /// Borra una conversación del historial (y del disco).
    pub fn delete_session(&self, session: i64) -> bool {
        let mut state = self.lock();
        let kept: Vec<ChatEntry> = state
            .entries
            .iter()
            .filter(|entry| entry.session != session)
            .cloned()
            .collect();
        let starts: BTreeMap<i64, f64> = state
            .starts
            .iter()
            .filter(|(key, _)| **key != session || session == state.session)
            .map(|(key, value)| (*key, *value))
            .collect();
        if kept.len() == state.entries.len() && !state.starts.contains_key(&session) {
            return false;
        }
        let Some((entries, activity)) = rewrite(&self.path, &kept, &starts, state.session) else {
            return false;
        };
        state.entries = entries;
        state.starts = starts;
        state.activity = activity;
        state.by_session = index_by_session(&state.entries);
        state.seq = state.activity.values().copied().max().unwrap_or(0);
        true
    }

    // -------------------------------------------------------------- lectura
    /// Conversación actual.
    pub fn session(&self) -> i64 {
        self.lock().session
    }

    /// Ruta del archivo.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Mensajes de una conversación (la actual por defecto), en orden.
    pub fn entries(&self, session: Option<i64>) -> Vec<ChatEntry> {
        let state = self.lock();
        let wanted = session.unwrap_or(state.session);
        state
            .by_session
            .iter()
            .find(|(key, _)| *key == wanted)
            .map_or_else(Vec::new, |(_, items)| items.clone())
    }

    /// Los últimos `limit` mensajes de todas las conversaciones.
    pub fn recent(&self, limit: i64) -> Vec<ChatEntry> {
        let state = self.lock();
        // DECISION: Python hace `entries[-max(0, limit):]`; con 0 o negativo
        // `-0` es 0, así que devuelve todo el historial. Se reproduce.
        if limit <= 0 {
            return state.entries.clone();
        }
        let keep = usize::try_from(limit).unwrap_or(usize::MAX);
        let skip = state.entries.len().saturating_sub(keep);
        state.entries[skip..].to_vec()
    }

    /// Conversaciones con mensajes, de la usada más recientemente a la más antigua.
    pub fn sessions(&self) -> Vec<SessionInfo> {
        let state = self.lock();
        let mut out = Vec::new();
        for (session, items) in &state.by_session {
            if items.is_empty() {
                continue;
            }
            let first_user = items
                .iter()
                .find(|entry| entry.role == Role::User)
                .map_or_else(|| items[0].text.clone(), |entry| entry.text.clone());
            out.push(SessionInfo {
                session: *session,
                started: state.starts.get(session).copied().unwrap_or(items[0].t),
                messages: items.len(),
                title: title(&first_user),
            });
        }
        out.sort_by(|left, right| {
            let left_key = (
                state.activity.get(&left.session).copied().unwrap_or(0),
                left.session,
            );
            let right_key = (
                state.activity.get(&right.session).copied().unwrap_or(0),
                right.session,
            );
            right_key.cmp(&left_key)
        });
        out
    }

    /// Mensajes que contienen `query` (sin distinguir mayúsculas).
    pub fn search(&self, query: &str, limit: i64) -> Vec<ChatEntry> {
        let needle = casefold(py_strip(query));
        if needle.is_empty() {
            return Vec::new();
        }
        let state = self.lock();
        let keep = usize::try_from(limit.max(0)).unwrap_or(usize::MAX);
        state
            .entries
            .iter()
            .rev()
            .filter(|entry| casefold(&entry.text).contains(&needle))
            .take(keep)
            .cloned()
            .collect()
    }

    /// Número de mensajes guardados.
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    /// `True` si no hay mensajes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests;

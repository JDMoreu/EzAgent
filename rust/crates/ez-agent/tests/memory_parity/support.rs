//! Soporte del dominio `memory`: reloj congelado y volcados de archivos y SQLite.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, FixedOffset, Utc};
use ez_agent::memory::{MemoryOptions, MemoryStore, MemoryTimestamp, RETENTION_DAYS};
use serde_json::{json, Map, Value};

pub(super) fn frozen() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2025-01-01T00:00:00+00:00").expect("fecha")
}

pub(super) fn options(input: &Value) -> MemoryOptions {
    MemoryOptions {
        session: input["session"].as_str().unwrap_or("default").to_string(),
        prefer_fts: input["prefer_fts"] != json!(false),
        retention_days: input["retention_days"].as_i64().unwrap_or(RETENTION_DAYS),
        clock: Some(Arc::new(frozen)),
    }
}

pub(super) fn store_at(dir: &Path, input: &Value) -> MemoryStore {
    MemoryStore::from_paths(
        dir.join("memory"),
        dir.join("history.db"),
        options(&input_at(input)),
    )
    .expect("almacén")
}

fn input_at(input: &Value) -> Value {
    input.clone()
}

pub(super) fn normalize(value: &Value, temp: &str) -> Value {
    match value {
        Value::String(text) => Value::String(
            text.replace('\\', "/")
                .replace(&temp.replace('\\', "/"), "<tmp>")
                .replace(temp, "<tmp>"),
        ),
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| normalize(item, temp)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), normalize(item, temp)))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub(super) fn dump_files(dir: &Path) -> Value {
    let mut files = Map::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Value::Object(files);
    };
    let mut items: Vec<(String, String)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let text = std::fs::read(entry.path())
                .map(|bytes| {
                    String::from_utf8_lossy(&bytes)
                        .replace("\r\n", "\n")
                        .replace('\r', "\n")
                })
                .unwrap_or_default();
            (name, text)
        })
        .collect();
    items.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, text) in items {
        files.insert(name, Value::String(text));
    }
    Value::Object(files)
}

pub(super) fn dump_db(path: &Path) -> Value {
    if !path.is_file() {
        return Value::Null;
    }
    let Ok(connection) = rusqlite::Connection::open(path) else {
        return Value::Null;
    };
    let mut schema = Vec::new();
    if let Ok(mut statement) = connection.prepare(
        "SELECT type, name, sql FROM sqlite_master WHERE sql IS NOT NULL \
         AND name NOT LIKE 'messages_fts_%' ORDER BY type, name",
    ) {
        if let Ok(rows) = statement.query_map([], |row| {
            Ok(
                json!({"type": row.get::<_, String>(0)?, "name": row.get::<_, String>(1)?,
                "sql": row.get::<_, String>(2)?}),
            )
        }) {
            schema = rows.filter_map(Result::ok).collect();
        }
    }
    let mut messages = Vec::new();
    if let Ok(mut statement) =
        connection.prepare("SELECT id, ts, session, role, content FROM messages ORDER BY id")
    {
        if let Ok(rows) = statement.query_map([], |row| {
            Ok(
                json!({"id": row.get::<_, i64>(0)?, "ts": row.get::<_, String>(1)?,
                "session": row.get::<_, String>(2)?, "role": row.get::<_, String>(3)?,
                "content": row.get::<_, String>(4)?}),
            )
        }) {
            messages = rows.filter_map(Result::ok).collect();
        }
    }
    let has_fts = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'messages_fts'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    let mut fts = Vec::new();
    if has_fts {
        if let Ok(mut statement) =
            connection.prepare("SELECT rowid, content FROM messages_fts ORDER BY rowid")
        {
            if let Ok(rows) = statement.query_map([], |row| {
                Ok(json!({"rowid": row.get::<_, i64>(0)?, "content": row.get::<_, String>(1)?}))
            }) {
                fts = rows.filter_map(Result::ok).collect();
            }
        }
    }
    json!({"schema": schema, "messages": messages, "fts": fts})
}

pub(super) fn info(store: &MemoryStore) -> Value {
    json!({"fts_enabled": store.fts_enabled, "fts_error": store.fts_error,
        "last_backend": store.last_backend(), "session": store.session})
}

pub(super) fn timestamp(value: &Value) -> Result<MemoryTimestamp, String> {
    match value.get("dt").and_then(Value::as_str) {
        Some(text) => Ok(MemoryTimestamp::DateTime(parse_instant(text)?)),
        None => match value {
            Value::Null => Ok(MemoryTimestamp::Now),
            Value::String(text) => Ok(MemoryTimestamp::Text(text.clone())),
            _ => Err("TypeError: ts debe ser datetime, str o None".to_string()),
        },
    }
}

pub(super) fn parse_instant(text: &str) -> Result<DateTime<FixedOffset>, String> {
    if let Ok(value) = DateTime::parse_from_rfc3339(text) {
        return Ok(value);
    }
    let naive = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S")
        .map_err(|_| "TypeError: ts debe ser datetime, str o None".to_string())?;
    Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc).fixed_offset())
}

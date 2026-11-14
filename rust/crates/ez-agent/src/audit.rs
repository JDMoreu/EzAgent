//! Port of `src/ezagent/agent/audit.py` (F3-04).
//!
//! Registro de auditoría JSONL append-only con redacción de secretos. Una línea
//! por llamada a herramienta. Nunca se guardan contenidos de archivos: las
//! claves `content`, `text`, `data` y `output` se sustituyen por su longitud.
//! Las claves de API y los tokens se redactan.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pytime::{isoformat_millis_utc, timestamp};
use ez_core::pyvalue::{char_len, python_str};
use ez_core::redact::redact;
use serde_json::{json, Map, Value};

const CONTENT_KEYS: [&str; 4] = ["content", "text", "data", "output"];
const SECRET_KEY_PARTS: [&str; 6] = ["api_key", "apikey", "token", "password", "passwd", "secret"];

/// Tamaño de un contenido: textos en caracteres; contenedores, los caracteres
/// de su serialización JSON; el resto, su representación.
fn content_size(value: &Value) -> usize {
    // DECISION (del Python): los textos cuentan caracteres; los contenedores,
    // los de su serialización JSON.
    match value {
        Value::Null => 0,
        Value::String(text) => char_len(text),
        Value::Array(_) | Value::Object(_) => {
            let options = DumpOptions::utf8();
            char_len(&dumps_with(value, &options))
        }
        other => char_len(&python_str(other)),
    }
}

/// Datos de una línea de auditoría.
#[derive(Debug, Clone)]
pub struct AuditEntry<'a> {
    pub turn_id: &'a str,
    pub tool: &'a str,
    pub level: i64,
    pub args: &'a Value,
    pub decision: &'a str,
    pub taint: bool,
    pub result_ok: bool,
    pub ms: f64,
    pub error_kind: Option<&'a str>,
}

/// Registro JSONL con cerrojo de hilos y rotación por tamaño.
pub struct AuditLog {
    pub path: PathBuf,
    pub max_bytes: u64,
    pub backups: usize,
    // DECISION (del Python): EzAgent es un único proceso; el cerrojo de hilos
    // evita líneas entrelazadas sin añadir bloqueos de archivo.
    lock: Mutex<()>,
}

impl AuditLog {
    pub fn new(path: &Path) -> Self {
        Self::with_limits(path, 5_000_000, 5)
    }

    pub fn with_limits(path: &Path, max_bytes: u64, backups: usize) -> Self {
        Self {
            path: path.to_path_buf(),
            max_bytes,
            backups,
            lock: Mutex::new(()),
        }
    }

    pub fn log(&self, entry: &AuditEntry<'_>) {
        let ts = isoformat_millis_utc(timestamp()).unwrap_or_default();
        let rounded = format!("{:.1}", entry.ms)
            .parse::<f64>()
            .unwrap_or(entry.ms);
        let mut record = Map::new();
        record.insert("ts".into(), json!(ts));
        record.insert("turn_id".into(), json!(entry.turn_id));
        record.insert("tool".into(), json!(entry.tool));
        record.insert("level".into(), json!(entry.level));
        record.insert("args".into(), self.redact(entry.args));
        record.insert("decision".into(), json!(entry.decision));
        record.insert("taint".into(), json!(entry.taint));
        record.insert("result_ok".into(), json!(entry.result_ok));
        record.insert("ms".into(), json!(rounded));
        if let Some(kind) = entry.error_kind {
            record.insert("error_kind".into(), json!(redact(kind)));
        }
        let options = DumpOptions::utf8();
        let line = format!("{}\n", dumps_with(&Value::Object(record), &options));
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.rotate_if_needed(line.len() as u64);
        if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) {
            let _ = std::fs::create_dir_all(parent);
        }
        let written = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut handle| handle.write_all(line.as_bytes()));
        if let Err(error) = written {
            tracing::warn!(
                "No se pudo escribir la auditoría en {}: {error}",
                self.path.display()
            );
        }
    }

    /// Copia `value` sustituyendo contenidos y secretos.
    pub fn redact(&self, value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut redacted = Map::new();
                for (key, item) in map {
                    let normalized = key.to_lowercase().replace('-', "_");
                    if CONTENT_KEYS.contains(&normalized.as_str()) {
                        redacted.insert(format!("{normalized}_chars"), json!(content_size(item)));
                    } else if SECRET_KEY_PARTS
                        .iter()
                        .any(|part| normalized.contains(part))
                    {
                        redacted.insert(key.clone(), json!("[REDACTED]"));
                    } else {
                        redacted.insert(key.clone(), self.redact(item));
                    }
                }
                Value::Object(redacted)
            }
            Value::Array(items) => Value::Array(items.iter().map(|i| self.redact(i)).collect()),
            Value::String(text) => Value::String(redact(text)),
            other => other.clone(),
        }
    }

    fn backup_path(&self, index: usize) -> PathBuf {
        let mut name = self.path.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".{index}"));
        self.path.with_file_name(name)
    }

    fn rotate_if_needed(&self, incoming_bytes: u64) {
        if self.backups == 0 {
            return;
        }
        let Ok(meta) = std::fs::metadata(&self.path) else {
            return;
        };
        if meta.len() + incoming_bytes <= self.max_bytes {
            return;
        }
        // Los backups se desplazan con rename; el más antiguo se descarta al
        // ser sobrescrito por el siguiente.
        for index in (1..self.backups).rev() {
            let source = self.backup_path(index);
            if !source.exists() {
                continue;
            }
            if let Err(error) = std::fs::rename(&source, self.backup_path(index + 1)) {
                tracing::warn!("No se pudo rotar {}: {error}", source.display());
            }
        }
        if self.path.exists() {
            if let Err(error) = std::fs::rename(&self.path, self.backup_path(1)) {
                tracing::warn!("No se pudo rotar {}: {error}", self.path.display());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("ez-audit-{}-{name}", std::process::id()))
            .join("audit.jsonl")
    }

    #[test]
    fn redacts_content_and_secrets() {
        let log = AuditLog::new(&temp_file("redact"));
        let args = json!({"path": "a.txt", "Content": "hola", "api-key": "sk-123", "data": [1, 2],
            "nested": {"token_x": "t", "ok": 1}, "list": [{"text": "xy"}]});
        let redacted = log.redact(&args);
        assert_eq!(redacted["content_chars"], 4);
        assert_eq!(redacted["data_chars"], 6);
        assert_eq!(redacted["api-key"], "[REDACTED]");
        assert_eq!(redacted["nested"]["token_x"], "[REDACTED]");
        assert_eq!(redacted["list"][0]["text_chars"], 2);
        assert_eq!(redacted["nested"]["ok"], 1);
    }

    #[test]
    fn appends_lines_and_rotates() {
        let path = temp_file("rotate");
        let log = AuditLog::with_limits(&path, 400, 2);
        let args = json!({"path": "x"});
        for index in 0..6 {
            log.log(&AuditEntry {
                turn_id: "t1",
                tool: "read_file",
                level: 0,
                args: &args,
                decision: "allow",
                taint: false,
                result_ok: index % 2 == 0,
                ms: 12.345,
                error_kind: Some("os"),
            });
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let first: Value =
            serde_json::from_str(text.lines().next().unwrap_or("{}")).unwrap_or(Value::Null);
        assert_eq!(first["tool"], "read_file");
        assert_eq!(first["ms"], 12.3);
        assert!(log.backup_path(1).exists());
        assert!(!log.backup_path(3).exists());
        for file in [path.clone(), log.backup_path(1), log.backup_path(2)] {
            let _ = std::fs::remove_file(file);
        }
    }
}

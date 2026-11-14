//! Helpers de disco y valores del historial.

use super::*;

pub(super) fn load(path: &Path, state: &mut State) {
    let raw = match fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            tracing::warn!("No pude leer el historial del chat: {error}");
            return;
        }
    };
    let text = String::from_utf8_lossy(&raw)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let mut bad = 0usize;
    for (index, line) in py_splitlines(&text).into_iter().enumerate() {
        if py_strip(line).is_empty() {
            continue;
        }
        let Ok(data) = serde_json::from_str::<Value>(line) else {
            bad += 1;
            continue;
        };
        if ChatStore::ingest(state, &data, index as i64 + 1).is_err() {
            bad += 1;
        }
    }
    if bad > 0 {
        tracing::warn!("Historial del chat: {bad} línea(s) ilegibles omitidas.");
    }
}

pub(super) fn append_line(path: &Path, record: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut handle = OpenOptions::new().create(true).append(true).open(path)?;
    let line = dumps_with(record, &DumpOptions::utf8());
    handle.write_all(format!("{line}\n").as_bytes())
}

pub(super) fn rewrite(
    path: &Path,
    entries: &[ChatEntry],
    starts: &BTreeMap<i64, f64>,
    session: i64,
) -> Option<(Vec<ChatEntry>, BTreeMap<i64, i64>)> {
    let mut lines: Vec<String> = Vec::new();
    let mut rewritten: Vec<ChatEntry> = Vec::new();
    let mut activity: BTreeMap<i64, i64> = BTreeMap::new();
    let mut seq = 0i64;
    for (number, start) in starts {
        seq += 1;
        activity.insert(*number, seq);
        lines.push(dumps_with(
            &json!({
                "v": VERSION,
                "t": round3(*start),
                "event": "session",
                "session": number,
                "seq": seq,
            }),
            &DumpOptions::utf8(),
        ));
    }
    for entry in entries {
        seq += 1;
        activity.insert(entry.session, seq);
        let mut entry = entry.clone();
        entry.seq = seq;
        rewritten.push(entry.clone());
        lines.push(dumps_with(
            &json!({
                "v": VERSION,
                "t": round3(entry.t),
                "session": entry.session,
                "role": entry.role.as_str(),
                "source": entry.source.as_str(),
                "text": entry.text,
                "seq": seq,
            }),
            &DumpOptions::utf8(),
        ));
    }
    if session != starts.keys().copied().max().unwrap_or(session) {
        seq += 1;
        activity.insert(session, seq);
        lines.push(dumps_with(
            &json!({
                "v": VERSION,
                "t": round3(timestamp()),
                "event": "resume",
                "session": session,
                "seq": seq,
            }),
            &DumpOptions::utf8(),
        ));
    }
    let temporary = path.with_file_name(format!("{}.tmp", path.file_name()?.to_string_lossy()));
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            tracing::warn!("No pude borrar la conversación del historial: {error}");
            return None;
        }
    }
    let body = lines.join("\n") + if lines.is_empty() { "" } else { "\n" };
    // DECISION: Python usa `write_text(newline="\n")` y `os.replace`; en Rust
    // se escriben los bytes y se renombra con `fs::rename` (atómico en Windows).
    if let Err(error) = fs::write(&temporary, body.as_bytes()) {
        tracing::warn!("No pude borrar la conversación del historial: {error}");
        return None;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        tracing::warn!("No pude borrar la conversación del historial: {error}");
        return None;
    }
    Some((rewritten, activity))
}

pub(super) fn index_by_session(entries: &[ChatEntry]) -> Vec<(i64, Vec<ChatEntry>)> {
    let mut by_session: Vec<(i64, Vec<ChatEntry>)> = Vec::new();
    for entry in entries {
        match by_session.iter_mut().find(|(key, _)| *key == entry.session) {
            Some((_, items)) => items.push(entry.clone()),
            None => by_session.push((entry.session, vec![entry.clone()])),
        }
    }
    by_session
}

pub(super) fn title(text: &str) -> String {
    let flat = ez_core::pyvalue::collapse_spaces(text);
    if char_len(&flat) <= 60 {
        return flat;
    }
    format!("{}…", py_rstrip(head_chars(&flat, 59)))
}

/// `round(value, 3)` con los flotantes de Python.
pub(super) fn round3(value: f64) -> f64 {
    format!("{value:.3}").parse::<f64>().unwrap_or(value)
}

pub(super) fn python_str(value: &Value) -> String {
    ez_core::pyvalue::python_str(value)
}

/// `int(value)` para los tipos que aparecen en el JSONL.
pub(super) fn py_int(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Bool(flag) => Some(i64::from(*flag)),
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value.trunc() as i64)),
        Value::String(text) => ez_core::pyvalue::python_int(text),
        _ => None,
    }
}

/// `float(value)` para los tipos que aparecen en el JSONL.
pub(super) fn py_float(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Mensajes en el formato del modelo (solo lo escrito/hablado).
pub fn as_messages(entries: &[ChatEntry]) -> Vec<ModelMessage> {
    entries
        .iter()
        .filter(|entry| matches!(entry.role, Role::User | Role::Assistant))
        .map(|entry| ModelMessage {
            role: entry.role,
            content: entry.text.clone(),
        })
        .collect()
}

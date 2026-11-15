//! Utilidades del arnés de paridad de `swarm`: directorio temporal, reloj fijo
//! del tablero, siembra de tableros con el lenguaje de `parity/gen/swarm_board.py`
//! y volcado normalizado de `board.db`.
//!
//! El oráculo normaliza el id del enjambre, los tokens, la carpeta del perfil
//! temporal y los sellos de fecha; aquí se hace lo mismo con el directorio de
//! esta corrida.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{Local, TimeZone};
use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_swarm::db::{Db, DbResult};
use ez_swarm::models::{Incident, Lease, Member, Message, SwarmInfo, Task};
use rusqlite::types::ValueRef;
use serde_json::{json, Map, Value};

/// Tablas del tablero, en el orden del oráculo.
pub const TABLES: [&str; 7] = [
    "cursor", "incident", "lease", "member", "message", "swarm", "task",
];

/// Columnas de tiempo: se comparan como segundos desde [`base`].
const TIME_COLUMNS: [&str; 6] = ["created", "updated", "ts", "seen", "expires", "resets"];

/// `datetime(2025, 1, 1, 20, 0).timestamp()` en hora local.
pub fn base() -> f64 {
    static BASE: OnceLock<f64> = OnceLock::new();
    *BASE.get_or_init(|| at(1, 20, 0))
}

/// Instante local del día fijo del tablero (época).
pub fn at(day: u32, hour: u32, minute: u32) -> f64 {
    Local
        .with_ymd_and_hms(2025, 1, day, hour, minute, 0)
        .single()
        .or_else(|| {
            Local
                .with_ymd_and_hms(2025, 1, day, hour, minute, 0)
                .earliest()
        })
        .expect("instante local válido")
        .timestamp() as f64
}

/// `datetime.timestamp()` de un naive: lo interpreta en hora local.
pub fn naive_timestamp(value: chrono::NaiveDateTime) -> f64 {
    Local
        .from_local_datetime(&value)
        .earliest()
        .expect("hora local válida")
        .timestamp() as f64
}

/// Carpeta temporal de esta corrida, borrada al terminar.
pub struct TempHome {
    path: PathBuf,
}

impl TempHome {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!("ez-swarm-parity-{}", std::process::id()));
        clean(&path);
        std::fs::create_dir_all(&path).expect("carpeta temporal del arnés");
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Subcarpeta `<home>/cases/<name>` con su `board.db` ya abierto.
    pub fn board(&self, name: &str) -> Db {
        let folder = self.path.join("cases").join(name);
        std::fs::create_dir_all(&folder).expect("carpeta del caso");
        Db::new(folder.join("board.db"), std::sync::Arc::new(base))
            .unwrap_or_else(|error| panic!("tablero de {name}: {error}"))
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        clean(&self.path);
    }
}

fn clean(path: &Path) {
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        if child.is_dir() {
            clean(&child);
            let _ = std::fs::remove_dir(&child);
        } else {
            let _ = std::fs::remove_file(&child);
        }
    }
    let _ = std::fs::remove_dir(path);
}

// ------------------------------------------------------------------ siembra

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn number(value: &Value, key: &str, default: i64) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(default)
}

fn real(value: &Value, key: &str, default: f64) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(default)
}

fn flag(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn id_list(value: Option<&Value>) -> Vec<i64> {
    value
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

fn string_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Crea el enjambre del caso y aplica sus operaciones.
pub fn seed(store: &Db, spec: &Value) -> DbResult<String> {
    let swarm = store.create(
        text(spec, "goal"),
        text(spec, "cwd"),
        spec.get("status")
            .and_then(Value::as_str)
            .unwrap_or("active"),
    )?;
    for op in spec
        .get("ops")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        apply(store, &swarm, op)?;
    }
    Ok(swarm)
}

fn apply(store: &Db, swarm: &str, op: &Value) -> DbResult<()> {
    match text(op, "op") {
        "join" => apply_join(store, swarm, op)?,
        "task" | "claim" | "finish" | "reopen" | "fill" => apply_task(store, swarm, op)?,
        "incident" => apply_incident(store, swarm, op)?,
        _ => apply_mail(store, swarm, op)?,
    }
    Ok(())
}

fn apply_join(store: &Db, swarm: &str, op: &Value) -> DbResult<()> {
    store.join(
        swarm,
        text(op, "name"),
        text(op, "role"),
        text(op, "agent"),
        text(op, "address"),
        text(op, "model"),
        text(op, "effort"),
    )?;
    Ok(())
}

fn apply_task(store: &Db, swarm: &str, op: &Value) -> DbResult<()> {
    match text(op, "op") {
        "task" => {
            store.add_task(
                swarm,
                text(op, "title"),
                text(op, "role"),
                text(op, "owner"),
                &id_list(op.get("deps")),
                number(op, "parent", 0),
            )?;
        }
        "claim" => {
            store.claim(
                swarm,
                text(op, "member"),
                op.get("task").and_then(Value::as_i64),
                false,
            )?;
        }
        "finish" => {
            store.finish(
                swarm,
                text(op, "member"),
                number(op, "task", 0),
                text(op, "result"),
                flag(op, "ok", true),
                text(op, "ref"),
            )?;
        }
        "reopen" => {
            store.reopen(
                swarm,
                number(op, "task", 0),
                op.get("by").and_then(Value::as_str).unwrap_or("ez"),
                text(op, "note"),
                text(op, "ref"),
            )?;
        }
        _ => {
            for index in 0..number(op, "count", 0) {
                let prefix = op.get("prefix").and_then(Value::as_str).unwrap_or("Tarea");
                store.add_task(
                    swarm,
                    &format!("{prefix} {}", index + 1),
                    text(op, "role"),
                    "",
                    &[],
                    0,
                )?;
            }
        }
    }
    Ok(())
}

fn apply_incident(store: &Db, swarm: &str, op: &Value) -> DbResult<()> {
    let resets = match op.get("resets") {
        Some(resets) if resets.is_object() => at(
            number(resets, "day", 1) as u32,
            number(resets, "hour", 0) as u32,
            number(resets, "minute", 0) as u32,
        ),
        _ => 0.0,
    };
    store.open_incident(
        swarm,
        text(op, "member"),
        text(op, "kind"),
        text(op, "detail"),
        resets,
    )?;
    Ok(())
}

fn apply_mail(store: &Db, swarm: &str, op: &Value) -> DbResult<()> {
    match text(op, "op") {
        "post" => {
            store.post(
                swarm,
                op.get("sender").and_then(Value::as_str).unwrap_or("ez"),
                text(op, "body"),
                op.get("to").and_then(Value::as_str).unwrap_or("*"),
                op.get("kind").and_then(Value::as_str).unwrap_or("info"),
                text(op, "ref"),
            )?;
        }
        "reserve" => {
            store.reserve(
                swarm,
                text(op, "member"),
                &string_list(&op["paths"]),
                real(op, "ttl", 900.0),
            )?;
        }
        "release" => {
            let paths = op.get("paths").map(string_list);
            store.release(swarm, text(op, "member"), paths.as_deref())?;
        }
        "incident_state" => {
            store.set_incident(number(op, "id", 0), text(op, "state"), text(op, "action"))?;
        }
        "member_state" => {
            store.set_member_state(swarm, text(op, "member"), text(op, "state"))?;
        }
        "address" => {
            store.set_address(swarm, text(op, "member"), text(op, "address"))?;
        }
        "status" => store.set_status(swarm, text(op, "status"))?,
        other => panic!("operación desconocida: {other}"),
    }
    Ok(())
}

// --------------------------------------------------------------- volcado

/// `round(value - BASE, 3) if value else 0.0`.
pub fn time_delta(value: f64) -> Value {
    if value == 0.0 {
        return json!(0.0);
    }
    let scaled = (value - base()) * 1000.0;
    json!(scaled.round_ties_even() / 1000.0)
}

fn is_time_column(key: &str) -> bool {
    TIME_COLUMNS.contains(&key)
}

fn row_value(table: &str, column: &str, raw: ValueRef<'_>, swarm: &str, home: &Path) -> Value {
    match raw {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => json!(number),
        ValueRef::Real(number) => {
            if is_time_column(column) {
                time_delta(number)
            } else {
                json!(number)
            }
        }
        ValueRef::Text(bytes) => {
            let value = String::from_utf8_lossy(bytes);
            let mut text = normalize_text(&value, swarm, home);
            if column == "token" && !text.is_empty() {
                text = "<token>".to_string();
            } else if column == "swarm" || (table == "swarm" && column == "id") {
                text = "<swarm>".to_string();
            }
            Value::String(text)
        }
        ValueRef::Blob(_) => Value::Null,
    }
}

fn sort_key(row: &Value) -> String {
    dumps_with(
        row,
        &DumpOptions {
            sort_keys: true,
            ..DumpOptions::default()
        },
    )
}

/// Filas de todas las tablas, normalizadas y ordenadas como el oráculo.
pub fn dump_board(store: &Db, swarm: &str, home: &Path) -> Value {
    store
        .read(|db| {
            let mut tables = Map::new();
            for table in TABLES {
                let mut statement = db.prepare(&format!("SELECT * FROM {table}"))?;
                let columns: Vec<String> = statement.column_names().map(str::to_string).collect();
                let mut rows = Vec::new();
                let mut query = statement.query([])?;
                while let Some(row) = query.next()? {
                    let mut object = Map::new();
                    for (index, column) in columns.iter().enumerate() {
                        object.insert(
                            column.clone(),
                            row_value(table, column, row.get_ref(index)?, swarm, home),
                        );
                    }
                    rows.push(Value::Object(object));
                }
                rows.sort_by_key(sort_key);
                tables.insert(table.to_string(), Value::Array(rows));
            }
            Ok(Value::Object(tables))
        })
        .unwrap_or_else(|error| panic!("volcado del tablero: {error}"))
}

/// Sustituye el id, la carpeta temporal y los sellos, y unifica «/».
pub fn normalize_text(value: &str, swarm: &str, home: &Path) -> String {
    let mut text = if swarm.is_empty() {
        value.to_string()
    } else {
        value.replace(swarm, "<swarm>")
    };
    let native = home.to_string_lossy().into_owned();
    let posix = native.replace('\\', "/");
    for prefix in [native, posix] {
        text = text.replace(&prefix, "<tmp>");
    }
    let text = replace_stamps(&text);
    text.replace('\\', "/")
}

/// `re.sub(r"\d{8}-\d{6}", "<stamp>", text)`.
fn replace_stamps(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < characters.len() {
        if matches(&characters, index, 8, &|c| c.is_ascii_digit())
            && characters.get(index + 8) == Some(&'-')
            && matches(&characters, index + 9, 6, &|c| c.is_ascii_digit())
        {
            out.push_str("<stamp>");
            index += 15;
        } else {
            out.push(characters[index]);
            index += 1;
        }
    }
    out
}

fn matches(characters: &[char], start: usize, count: usize, accept: &dyn Fn(char) -> bool) -> bool {
    characters.len() >= start + count && characters[start..start + count].iter().all(|c| accept(*c))
}

// ------------------------------------------------------- conversión a JSON

/// Objeto JSON de un `SwarmInfo` como `dataclasses.asdict` + `_jsonable`.
pub fn swarm_json(info: &SwarmInfo) -> Value {
    json!({
        "id": info.id,
        "goal": info.goal,
        "cwd": info.cwd,
        "status": info.status,
        "created": time_delta(info.created),
        "updated": time_delta(info.updated),
    })
}

pub fn member_json(member: &Member) -> Value {
    json!({
        "name": member.name,
        "role": member.role,
        "agent": member.agent,
        "address": member.address,
        "state": member.state,
        "seen": time_delta(member.seen),
        "model": member.model,
        "effort": member.effort,
    })
}

pub fn task_json(task: &Task) -> Value {
    json!({
        "id": task.id,
        "title": task.title,
        "role": task.role,
        "owner": task.owner,
        "state": task.state,
        "deps": task.deps,
        "parent": task.parent,
        "result": task.result,
        "rounds": task.rounds,
        "updated": time_delta(task.updated),
    })
}

pub fn message_json(message: &Message) -> Value {
    json!({
        "id": message.id,
        "sender": message.sender,
        "recipient": message.recipient,
        "kind": message.kind,
        "body": message.body,
        "ref": message.ref_,
        "ts": time_delta(message.ts),
    })
}

pub fn lease_json(lease: &Lease) -> Value {
    json!({
        "path": lease.path,
        "member": lease.member,
        "expires": time_delta(lease.expires),
    })
}

pub fn incident_json(incident: &Incident) -> Value {
    json!({
        "id": incident.id,
        "swarm": incident.swarm,
        "member": incident.member,
        "kind": incident.kind,
        "detail": incident.detail,
        "resets": time_delta(incident.resets),
        "state": incident.state,
        "action": incident.action,
        "created": time_delta(incident.created),
        "updated": time_delta(incident.updated),
    })
}

/// Diferencias hoja a hoja entre el oráculo y lo producido.
pub fn diff_json(expected: &Value, produced: &Value, path: &str) -> Vec<String> {
    if expected == produced {
        return Vec::new();
    }
    match (expected, produced) {
        (Value::Object(left), Value::Object(right)) => {
            let mut out = Vec::new();
            for (key, value) in left {
                match right.get(key) {
                    Some(other) => out.extend(diff_json(value, other, &format!("{path}.{key}"))),
                    None => out.push(format!("{path}.{key}: falta en el Rust")),
                }
            }
            for key in right.keys() {
                if !left.contains_key(key) {
                    out.push(format!("{path}.{key}: sobra en el Rust"));
                }
            }
            out
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            let mut out = Vec::new();
            for (index, (value, other)) in left.iter().zip(right).enumerate() {
                out.extend(diff_json(value, other, &format!("{path}[{index}]")));
            }
            out
        }
        _ => vec![format!(
            "{path}: esperado {} · obtenido {}",
            ez_core::pyjson::dumps(expected),
            ez_core::pyjson::dumps(produced)
        )],
    }
}

pub fn input_str<'a>(input: &'a Value, key: &str) -> &'a str {
    input.get(key).and_then(Value::as_str).unwrap_or("")
}

pub fn input_i64(input: &Value, key: &str, default: i64) -> i64 {
    input.get(key).and_then(Value::as_i64).unwrap_or(default)
}

pub fn input_array<'a>(input: &'a Value, key: &str) -> &'a [Value] {
    input
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

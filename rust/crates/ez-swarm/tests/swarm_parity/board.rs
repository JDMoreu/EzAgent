//! Casos de `store.*` (tablero directo, volcado SQLite) y `view.*`.

use std::path::Path;

use ez_parity::fixtures::FixtureCase;
use ez_swarm::db::{Db, DbError};
use ez_swarm::view::{self, Health};
use serde_json::{json, Value};

use super::support::{
    base, dump_board, incident_json, input_array, input_i64, input_str, lease_json, member_json,
    message_json, normalize_text, swarm_json, task_json, TempHome,
};

fn arg(input: &Value, index: usize) -> &Value {
    input_array(input, "args")
        .get(index)
        .unwrap_or(&Value::Null)
}

fn arg_text(input: &Value, index: usize, swarm: &str) -> String {
    match arg(input, index).as_str() {
        Some("$swarm") => swarm.to_string(),
        Some(text) => text.to_string(),
        None => String::new(),
    }
}

fn arg_i64(input: &Value, index: usize, default: i64) -> i64 {
    arg(input, index).as_i64().unwrap_or(default)
}

fn arg_opt_i64(input: &Value, index: usize) -> Option<i64> {
    arg(input, index).as_i64()
}

fn arg_paths(input: &Value, index: usize) -> Vec<String> {
    input_array_of(input, index)
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn input_array_of(input: &Value, index: usize) -> &[Value] {
    arg(input, index).as_array().map_or(&[], Vec::as_slice)
}

fn kw<'a>(input: &'a Value, key: &str) -> Option<&'a Value> {
    input.get("kwargs").and_then(|kwargs| kwargs.get(key))
}

fn kw_text(input: &Value, key: &str) -> String {
    kw(input, key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn kw_or<'a>(input: &'a Value, key: &str, default: &'a str) -> &'a str {
    kw(input, key).and_then(Value::as_str).unwrap_or(default)
}

fn kw_i64(input: &Value, key: &str, default: i64) -> i64 {
    kw(input, key).and_then(Value::as_i64).unwrap_or(default)
}

fn kw_f64(input: &Value, key: &str, default: f64) -> f64 {
    kw(input, key).and_then(Value::as_f64).unwrap_or(default)
}

fn kw_bool(input: &Value, key: &str, default: bool) -> bool {
    kw(input, key).and_then(Value::as_bool).unwrap_or(default)
}

fn kw_ids(input: &Value, key: &str) -> Vec<i64> {
    kw(input, key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default()
}

fn normalize_error(error: &DbError, swarm: &str, home: &Path) -> String {
    normalize_text(&error.to_string(), swarm, home)
}

pub fn store_case(case: &FixtureCase, home: &TempHome) -> Result<Value, String> {
    let input = &case.input;
    let spec = input.get("board").cloned().unwrap_or_else(|| json!({}));
    let store = home.board(&case.case);
    let swarm = super::support::seed(&store, &spec)
        .unwrap_or_else(|error| panic!("siembra de {}: {error}", case.case));
    let result = dispatch(&store, case, &swarm);
    match result {
        Ok(value) => {
            let mut shown =
                normalize_strings(render(value, &swarm, home.path()), &swarm, home.path());
            if input_str(input, "method") == "create" {
                // El oráculo muestra «<swarm>» para el id recién creado (no es el del caso).
                shown = json!("<swarm>");
            }
            Ok(json!({
                "result": shown,
                "board": dump_board(&store, &swarm, home.path()),
            }))
        }
        Err(error) => Err(normalize_error(&error, &swarm, home.path())),
    }
}

enum Shown {
    Member(ez_swarm::models::Member),
    Task(ez_swarm::models::Task),
    Incident(ez_swarm::models::Incident),
    Swarms(Vec<ez_swarm::models::SwarmInfo>),
    Tasks(Vec<ez_swarm::models::Task>),
    Messages(Vec<ez_swarm::models::Message>),
    Leases(Vec<ez_swarm::models::Lease>),
    Incidents(Vec<ez_swarm::models::Incident>),
    Text(String),
    Number(i64),
    Nothing,
    MaybeSwarm(Option<ez_swarm::models::SwarmInfo>),
    MaybeIncident(Option<ez_swarm::models::Incident>),
    TaskResult(ez_swarm::models::Task, bool),
    IncidentResult(ez_swarm::models::Incident, bool),
    MessageResult(ez_swarm::models::Message, bool),
    Pending(i64, i64, Vec<i64>),
    Members(Vec<ez_swarm::models::Member>),
}

fn render(value: Shown, swarm: &str, home: &Path) -> Value {
    match value {
        Shown::Member(member) => member_json(&member),
        Shown::Task(task) => task_json(&task),
        Shown::Incident(incident) => incident_json(&incident),
        Shown::Swarms(items) => Value::Array(items.iter().map(swarm_json).collect()),
        Shown::Tasks(items) => Value::Array(items.iter().map(task_json).collect()),
        Shown::Messages(items) => Value::Array(items.iter().map(message_json).collect()),
        Shown::Leases(items) => Value::Array(items.iter().map(lease_json).collect()),
        Shown::Incidents(items) => Value::Array(items.iter().map(incident_json).collect()),
        Shown::Text(text) => Value::String(normalize_text(&text, swarm, home)),
        Shown::Number(number) => json!(number),
        Shown::Nothing => Value::Null,
        Shown::MaybeSwarm(info) => info.as_ref().map_or(Value::Null, swarm_json),
        Shown::MaybeIncident(incident) => incident.as_ref().map_or(Value::Null, incident_json),
        Shown::TaskResult(task, flag) => json!([task_json(&task), flag]),
        Shown::IncidentResult(incident, flag) => json!([incident_json(&incident), flag]),
        Shown::MessageResult(message, flag) => json!([message_json(&message), flag]),
        Shown::Pending(unread, last, ready) => json!([unread, last, ready]),
        Shown::Members(items) => Value::Array(items.iter().map(member_json).collect()),
    }
}

fn dispatch(store: &Db, case: &FixtureCase, swarm: &str) -> Result<Shown, DbError> {
    let input = &case.input;
    let method = input_str(input, "method");
    let shown = match method {
        "create" | "get" | "latest" | "active" | "live" | "set_status" => {
            dispatch_swarm(store, method, input, swarm)?
        }
        "join" | "members" | "pending" => dispatch_member(store, method, input, swarm)?,
        "add_task" | "claim" | "finish" | "reopen" => dispatch_task(store, method, input, swarm)?,
        "post" | "inbox" | "unread" | "history" | "reserve" | "release" | "leases" => {
            dispatch_mail(store, method, input, swarm)?
        }
        "open_incident" | "incident" | "incidents" | "member_incident" | "set_incident" => {
            dispatch_incident(store, method, input, swarm)?
        }
        other => panic!("método del tablero sin arnés: {other}"),
    };
    Ok(shown)
}

fn dispatch_swarm(store: &Db, method: &str, input: &Value, swarm: &str) -> Result<Shown, DbError> {
    Ok(match method {
        "create" => Shown::Text(store.create(
            &arg_text(input, 0, swarm),
            kw_or(input, "cwd", ""),
            kw_or(input, "status", "active"),
        )?),
        "get" => Shown::MaybeSwarm(store.get(&arg_text(input, 0, swarm))?),
        "latest" => Shown::Text(store.latest()?),
        "active" => Shown::Swarms(store.active()?),
        "live" => Shown::Swarms(store.live()?),
        _ => {
            store.set_status(&arg_text(input, 0, swarm), &arg_text(input, 1, swarm))?;
            Shown::Nothing
        }
    })
}

fn dispatch_member(store: &Db, method: &str, input: &Value, swarm: &str) -> Result<Shown, DbError> {
    Ok(match method {
        "join" => Shown::Member(store.join(
            &arg_text(input, 0, swarm),
            &arg_text(input, 1, swarm),
            &kw_text(input, "role"),
            &kw_text(input, "agent"),
            &kw_text(input, "address"),
            &kw_text(input, "model"),
            &kw_text(input, "effort"),
        )?),
        "members" => Shown::Members(store.members(&arg_text(input, 0, swarm))?),
        _ => {
            let (unread, last, ready) =
                store.pending(&arg_text(input, 0, swarm), &arg_text(input, 1, swarm))?;
            Shown::Pending(unread, last, ready)
        }
    })
}

fn dispatch_task(store: &Db, method: &str, input: &Value, swarm: &str) -> Result<Shown, DbError> {
    Ok(match method {
        "add_task" => Shown::Task(store.add_task(
            &arg_text(input, 0, swarm),
            &arg_text(input, 1, swarm),
            &kw_text(input, "role"),
            &kw_text(input, "owner"),
            &kw_ids(input, "deps"),
            kw_i64(input, "parent", 0),
        )?),
        "claim" => Shown::Tasks(store.claim(
            &arg_text(input, 0, swarm),
            &arg_text(input, 1, swarm),
            arg_opt_i64(input, 2),
            kw_bool(input, "every", false),
        )?),
        "finish" => Shown::Task(store.finish(
            &arg_text(input, 0, swarm),
            &arg_text(input, 1, swarm),
            arg_i64(input, 2, 0),
            &arg_text(input, 3, swarm),
            kw_bool(input, "ok", true),
            &kw_text(input, "ref"),
        )?),
        _ => {
            let (task, exhausted) = store.reopen(
                &arg_text(input, 0, swarm),
                arg_i64(input, 1, 0),
                kw_or(input, "by", "ez"),
                &kw_text(input, "note"),
                &kw_text(input, "ref"),
            )?;
            Shown::TaskResult(task, exhausted)
        }
    })
}

fn dispatch_mail(store: &Db, method: &str, input: &Value, swarm: &str) -> Result<Shown, DbError> {
    Ok(match method {
        "post" => {
            let (message, clipped) = store.post(
                &arg_text(input, 0, swarm),
                &arg_text(input, 1, swarm),
                &arg_text(input, 2, swarm),
                kw_or(input, "to", "*"),
                kw_or(input, "kind", "info"),
                &kw_text(input, "ref"),
            )?;
            Shown::MessageResult(message, clipped)
        }
        "inbox" => Shown::Messages(store.inbox(
            &arg_text(input, 0, swarm),
            &arg_text(input, 1, swarm),
            kw_bool(input, "mark", true),
            kw_i64(input, "limit", 20),
        )?),
        "unread" => {
            Shown::Number(store.unread(&arg_text(input, 0, swarm), &arg_text(input, 1, swarm))?)
        }
        "history" => Shown::Messages(store.history(
            &arg_text(input, 0, swarm),
            kw_i64(input, "since", 0),
            kw_i64(input, "limit", 50),
        )?),
        "reserve" => Shown::Leases(store.reserve(
            &arg_text(input, 0, swarm),
            &arg_text(input, 1, swarm),
            &arg_paths(input, 2),
            kw_f64(input, "ttl", 900.0),
        )?),
        "release" => {
            let paths = arg(input, 2).as_array().map(|_| arg_paths(input, 2));
            Shown::Number(store.release(
                &arg_text(input, 0, swarm),
                &arg_text(input, 1, swarm),
                paths.as_deref(),
            )?)
        }
        _ => Shown::Leases(store.leases(&arg_text(input, 0, swarm))?),
    })
}

fn dispatch_incident(
    store: &Db,
    method: &str,
    input: &Value,
    swarm: &str,
) -> Result<Shown, DbError> {
    Ok(match method {
        "open_incident" => {
            let (incident, is_new) = store.open_incident(
                &arg_text(input, 0, swarm),
                &arg_text(input, 1, swarm),
                &arg_text(input, 2, swarm),
                &arg_text(input, 3, swarm),
                kw_f64(input, "resets", 0.0),
            )?;
            Shown::IncidentResult(incident, is_new)
        }
        "incident" => Shown::MaybeIncident(store.incident(arg_i64(input, 0, 0))?),
        "incidents" => Shown::Incidents(
            store.incidents(&arg_text(input, 0, swarm), kw_bool(input, "active", true))?,
        ),
        "member_incident" => Shown::MaybeIncident(
            store.member_incident(&arg_text(input, 0, swarm), &arg_text(input, 1, swarm))?,
        ),
        _ => Shown::Incident(store.set_incident(
            arg_i64(input, 0, 0),
            &arg_text(input, 1, swarm),
            &arg_text(input, 2, swarm),
        )?),
    })
}

/// `_jsonable` normaliza todo texto del resultado, también dentro de listas.
fn normalize_strings(value: Value, swarm: &str, home: &Path) -> Value {
    match value {
        Value::String(text) => Value::String(normalize_text(&text, swarm, home)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| normalize_strings(item, swarm, home))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, item)| (key, normalize_strings(item, swarm, home)))
                .collect(),
        ),
        other => other,
    }
}

fn health_json(found: &Health) -> Value {
    json!({
        "state": found.state,
        "done": found.done,
        "total": found.total,
        "working": found.working,
        "issues": found.issues(),
        "keys": found.found.iter().map(|issue| issue.key.clone()).collect::<Vec<_>>(),
        "spoken": found.spoken(),
    })
}

pub fn view_case(case: &FixtureCase, home: &TempHome) -> Result<Value, String> {
    let input = &case.input;
    let spec = input.get("board").cloned().unwrap_or_else(|| json!({}));
    let store = home.board(&case.case);
    let swarm = super::support::seed(&store, &spec)
        .unwrap_or_else(|error| panic!("siembra de {}: {error}", case.case));
    let target = if case.case.ends_with("/ajeno") {
        "no-existe".to_string()
    } else {
        swarm.clone()
    };
    let now = base() + input.get("now").and_then(Value::as_f64).unwrap_or(0.0);
    let outcome = match case.function.as_str() {
        "view.board" => view::board_recent(
            &store,
            &target,
            input_str(input, "member"),
            input_i64(input, "recent", 4),
        )
        .map(|text| normalize_text(&text, &swarm, home.path()))
        .map(|text| json!({ "text": text })),
        "view.health" => view::health(&store, &target, now).map(|found| health_json(&found)),
        "view.summary" => view::summary(&store, &target, now)
            .map(|text| normalize_text(&text, &swarm, home.path()))
            .map(|text| json!({ "text": text })),
        other => panic!("vista sin arnés: {other}"),
    };
    outcome.map_err(|error| normalize_error(&error, &swarm, home.path()))
}

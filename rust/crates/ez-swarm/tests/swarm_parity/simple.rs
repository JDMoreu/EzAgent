//! Casos de `protocol`, `models`, `progress` e `incidents`.

use chrono::NaiveDateTime;
use ez_parity::fixtures::FixtureCase;
use ez_swarm::incidents;
use ez_swarm::models::{
    clip, flat, lease_path, member_name, overlaps, required, role_name, task_id, Incident, Member,
    SwarmInfo, Task,
};
use ez_swarm::progress::Progress;
use ez_swarm::protocol;
use serde_json::{json, Map, Value};

use super::support::{at, base, input_array, input_i64, input_str, naive_timestamp};

pub fn protocol_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    match case.function.as_str() {
        "protocol.role_label" => {
            Ok(json!({ "text": protocol::role_label(input_str(input, "role")) }))
        }
        "protocol.stage_name" => Ok(json!({
            "text": protocol::stage_name(input_str(input, "role"), input_str(input, "title")),
        })),
        "protocol.role_task" => Ok(json!({
            "text": protocol::role_task(input_str(input, "role")),
        })),
        "protocol.default_tasks" => default_tasks_case(input),
        "protocol.briefing" => briefing(case),
        "protocol.constants" => Ok(json!({
            "idle": protocol::IDLE,
            "all_done": protocol::ALL_DONE,
            "stopped": protocol::STOPPED,
            "paused": protocol::PAUSED,
            "no_seat": protocol::NO_SEAT,
            "final_offer": protocol::FINAL_OFFER,
            "role_labels": table(&protocol::ROLE_LABELS),
            "stage_names": table(&protocol::STAGE_NAMES),
        })),
        "protocol.help_text" => Ok(json!({
            "text": protocol::help_text(command_of(input)),
        })),
        "protocol.join_line" => join_case(input),
        "protocol.open_task_line" => Ok(json!({
            "text": protocol::open_task_line(&task_ids(input), command_of(input)),
        })),
        "protocol.wake_line" => Ok(json!({
            "text": protocol::wake_line(
                input_i64(input, "unread", 0),
                input_i64(input, "ready", 0),
                command_of(input),
            ),
        })),
        other => Err(format!("caso sin arnés: {other}")),
    }
}

/// El generador siempre usó «ezs» y no lo guardó en estos casos.
fn command_of(input: &Value) -> &str {
    input
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("ezs")
}

fn task_ids(input: &Value) -> Vec<i64> {
    input_array(input, "tasks")
        .iter()
        .filter_map(Value::as_i64)
        .collect()
}

fn default_tasks_case(input: &Value) -> Result<Value, String> {
    let roles: Vec<String> = input_array(input, "roles")
        .iter()
        .map(|role| role.as_str().unwrap_or("").to_string())
        .collect();
    let tasks: Vec<Value> = protocol::default_tasks(&roles)
        .into_iter()
        .map(|(title, role)| json!([title, role]))
        .collect();
    Ok(json!({ "tasks": tasks }))
}

fn join_case(input: &Value) -> Result<Value, String> {
    let member = member(input_str(input, "name"), input_str(input, "role"), "claude");
    Ok(json!({
        "text": protocol::join_line(&info(), &member, input_str(input, "command")),
    }))
}

fn table(rows: &[(&str, &str)]) -> Value {
    let mut map = Map::new();
    for (key, value) in rows {
        map.insert((*key).to_string(), json!(value));
    }
    Value::Object(map)
}

fn info() -> SwarmInfo {
    SwarmInfo {
        id: "abc12345".to_string(),
        goal: "Migrar el núcleo a Rust".to_string(),
        cwd: "C:/proyecto".to_string(),
        status: "active".to_string(),
        created: base(),
        updated: base(),
    }
}

fn member(name: &str, role: &str, agent: &str) -> Member {
    Member {
        name: name.to_string(),
        role: role.to_string(),
        agent: agent.to_string(),
        address: "seat-1".to_string(),
        state: "idle".to_string(),
        seen: base(),
        model: String::new(),
        effort: String::new(),
    }
}

/// Rol y agente de cada miembro de un caso de `protocol.briefing`; el oráculo
/// los arma en `parity/gen/swarm.py` y la entrada solo guarda los nombres.
fn member_specs(case: &str, name: &str) -> (String, String) {
    let picked = match (case, name) {
        ("solo", _) | ("mixto", "bo") => ("", "claude"),
        ("mixto", "ana") | ("mismo-rol", "ana") => ("plan", "claude"),
        ("mismo-rol", "bo") => ("dev", "claude"),
        ("mismo-rol", "ca") => ("dev", "claude"),
        ("otro-rol", _) => ("otro rol", "claude"),
        (_, _) if case.starts_with("rol-") => (&case[4..], "claude"),
        (_, "ana") => ("plan", "claude"),
        (_, "bo") => ("dev", "opencode"),
        (_, "ca") => ("dev", "claude"),
        (_, "cy") => ("qa", "codex"),
        (_, "zz") => ("qa", "codex"),
        (_, "zo") => ("otro rol", "claude"),
        (_, other) => (other, "claude"),
    };
    (picked.0.to_string(), picked.1.to_string())
}

fn briefing(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    let id = case.case.rsplit('/').next().unwrap_or("");
    let members: Vec<Member> = input_array(input, "members")
        .iter()
        .filter_map(Value::as_str)
        .map(|name| {
            let (role, agent) = member_specs(id, name);
            member(name, &role, &agent)
        })
        .collect();
    let me_name = input_str(input, "me");
    let me = members
        .iter()
        .find(|member| member.name == me_name)
        .cloned()
        .unwrap_or_else(|| {
            if id == "ajeno" {
                member(me_name, "qa", "codex")
            } else {
                member(me_name, "", "claude")
            }
        });
    let swarm = &input["swarm"];
    let info = SwarmInfo {
        id: input_str(swarm, "id").to_string(),
        goal: input_str(swarm, "goal").to_string(),
        cwd: input_str(swarm, "cwd").to_string(),
        status: input_str(swarm, "status").to_string(),
        created: base(),
        updated: base(),
    };
    Ok(json!({
        "text": protocol::briefing(&info, &me, &members, input_str(input, "command")),
    }))
}

pub fn models_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    match case.function.as_str() {
        "models.flat" => Ok(json!({ "text": flat(&input["value"]) })),
        "models.clip" => {
            let (text, clipped) = clip(&input["value"], input_i64(input, "limit", 0) as usize);
            Ok(json!({ "text": text, "clipped": clipped }))
        }
        "models.member_name" => member_name(&input["value"])
            .map(|text| json!({ "result": text }))
            .map_err(|error| error.message().to_string()),
        "models.role_name" => role_name(&input["value"])
            .map(|text| json!({ "result": text }))
            .map_err(|error| error.message().to_string()),
        "models.task_id" => task_id(&input["value"])
            .map(|number| json!({ "result": number }))
            .map_err(|error| error.message().to_string()),
        "models.required" => required(
            &input["value"],
            input_i64(input, "limit", 0) as usize,
            input_str(input, "what"),
        )
        .map(|text| json!({ "result": text }))
        .map_err(|error| error.message().to_string()),
        "models.lease_path" => lease_path(&input["value"])
            .map(|text| json!({ "result": text }))
            .map_err(|error| error.message().to_string()),
        "models.overlaps" => Ok(json!({
            "overlaps": overlaps(input_str(input, "first"), input_str(input, "second")),
        })),
        other => Err(format!("caso sin arnés: {other}")),
    }
}

pub fn progress_case(case: &FixtureCase) -> Result<Value, String> {
    let progress = Progress::new();
    let mut news = Vec::new();
    let steps = input_array(&case.input, "steps");
    for step in steps {
        let tasks = tasks_of(step);
        news.push(Value::Array(
            progress
                .check("abc12345", &tasks)
                .into_iter()
                .map(Value::String)
                .collect(),
        ));
    }
    if case
        .input
        .get("then_forget")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        progress.forget("abc12345");
        let tasks = tasks_of(steps.last().expect("al menos un paso"));
        news.push(Value::Array(
            progress
                .check("abc12345", &tasks)
                .into_iter()
                .map(Value::String)
                .collect(),
        ));
    }
    Ok(json!({ "news": news }))
}

fn tasks_of(step: &Value) -> Vec<Task> {
    step.as_array()
        .into_iter()
        .flatten()
        .map(|raw| Task {
            id: input_i64(raw, "id", 0),
            title: input_str(raw, "title").to_string(),
            role: input_str(raw, "role").to_string(),
            owner: input_str(raw, "owner").to_string(),
            state: raw
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("pending")
                .to_string(),
            deps: raw
                .get("deps")
                .and_then(Value::as_array)
                .map(|deps| deps.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default(),
            parent: input_i64(raw, "parent", 0),
            result: String::new(),
            rounds: input_i64(raw, "rounds", 0),
            updated: base(),
        })
        .collect()
}

fn naive(value: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
        .unwrap_or_else(|error| panic!("fecha del fixture {value}: {error}"))
}

pub fn round3(value: f64) -> f64 {
    (value * 1000.0).round_ties_even() / 1000.0
}

fn moment(raw: Option<&Value>, fallback: f64) -> f64 {
    match raw {
        Some(raw) if raw.is_object() => at(
            input_i64(raw, "day", 1) as u32,
            input_i64(raw, "hour", 0) as u32,
            input_i64(raw, "minute", 0) as u32,
        ),
        _ => fallback,
    }
}

fn incident_of(raw: &Value) -> Incident {
    let created = moment(raw.get("created"), base());
    Incident {
        id: 7,
        swarm: "abc12345".to_string(),
        member: "bo".to_string(),
        kind: input_str(raw, "kind").to_string(),
        detail: input_str(raw, "detail").to_string(),
        resets: moment(raw.get("resets"), 0.0),
        state: "open".to_string(),
        action: String::new(),
        created,
        updated: created,
    }
}

fn incident_member(raw: Option<&Value>) -> Member {
    let raw = raw.unwrap_or(&Value::Null);
    let mut found = member(
        "bo",
        raw.get("role").and_then(Value::as_str).unwrap_or("dev"),
        raw.get("agent")
            .and_then(Value::as_str)
            .unwrap_or("opencode"),
    );
    found.model = raw
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    found
}

fn task_names(input: &Value) -> Vec<String> {
    input_array(input, "tasks")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

pub fn incidents_case(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    match case.function.as_str() {
        "incidents.classify" => classify_case(input),
        "incidents.parse_reset" => parse_reset_case(input),
        "incidents.when" => Ok(json!({
            "text": incidents::when(moment(input.get("resets"), 0.0), moment(input.get("ref"), 0.0)),
        })),
        "incidents.describe" | "incidents.board_note" => incident_text(case),
        "incidents.question" => question_case(input),
        "incidents.suggest" => suggest_case(input),
        other => Err(format!("caso sin arnés: {other}")),
    }
}

fn classify_case(input: &Value) -> Result<Value, String> {
    let now = naive(input_str(input, "now"));
    let found = incidents::classify(input_str(input, "screen"), now);
    Ok(json!({
        "signal": found.map(|signal| {
            let after = if signal.resets != 0.0 {
                round3(signal.resets - naive_timestamp(now))
            } else {
                0.0
            };
            json!({ "kind": signal.kind, "detail": signal.detail, "resets_after": after })
        }),
    }))
}

fn parse_reset_case(input: &Value) -> Result<Value, String> {
    let now = naive(input_str(input, "now"));
    let resets = incidents::parse_reset(input_str(input, "text"), now);
    let after = if resets != 0.0 {
        round3(resets - naive_timestamp(now))
    } else {
        0.0
    };
    Ok(json!({ "after": after }))
}

fn incident_text(case: &FixtureCase) -> Result<Value, String> {
    let input = &case.input;
    let incident = incident_of(&input["incident"]);
    let member = incident_member(input.get("member"));
    let tasks = task_names(input);
    let text = if case.function.ends_with("describe") {
        incidents::describe(&incident, &member, &tasks)
    } else {
        incidents::board_note(&incident, &member, &tasks)
    };
    Ok(json!({ "text": text }))
}

fn question_case(input: &Value) -> Result<Value, String> {
    let incident = incident_of(&input["incident"]);
    let failing = incident_member(input.get("member"));
    let alt = input.get("alt").filter(|alt| !alt.is_null()).map(|alt| {
        let mut found = member(
            "bo",
            "dev",
            alt.get("agent").and_then(Value::as_str).unwrap_or(""),
        );
        found.model = alt
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        found
    });
    let tasks = task_names(input);
    Ok(json!({
        "text": incidents::question(&incident, &failing, &tasks, alt.as_ref()),
    }))
}

fn suggest_case(input: &Value) -> Result<Value, String> {
    let gone = input_array(input, "gone");
    let members: Vec<Member> = input_array(input, "members")
        .iter()
        .filter_map(Value::as_str)
        .map(|item| {
            let (name, agent) = item.split_once(':').unwrap_or((item, ""));
            let mut found = member(name, "dev", agent);
            if gone.iter().any(|gone| gone.as_str() == Some(name)) {
                found.state = "gone".to_string();
            }
            found
        })
        .collect();
    let failing_name = input_str(input, "failing");
    let failing = members
        .iter()
        .find(|member| member.name == failing_name)
        .cloned()
        .unwrap_or_else(|| member(failing_name, "", "claude"));
    let busy: std::collections::HashSet<String> = input_array(input, "busy")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    let found = incidents::suggest(&members, &failing, &busy);
    Ok(json!({
        "member": found.map(|member| json!({ "name": member.name, "agent": member.agent })),
    }))
}

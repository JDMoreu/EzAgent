//! Casos de `run_case`: texto, JSON, terminales, contexto y archivos.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use ez_herdr::herdr::{strip_ansi, visible_name, Herdr};
use ez_parity::fixtures::FixtureCase;
use serde_json::{json, Value};

use super::harness::*;

pub(super) fn run_case(case: &FixtureCase, temp: &str) -> Result<Value, String> {
    let input = &case.input;
    let world = World::arc(input);
    let build = || Arc::new(Herdr::with_runtime(config(temp, input), runtime_of(&world)));
    match case.function.as_str() {
        "strip_ansi" => Ok(json!({"text": strip_ansi(&text(input.get("text")))})),
        "visible_name" => {
            let item = input.as_object().cloned().unwrap_or_default();
            Ok(json!({"name": visible_name(&item)}))
        }
        "Herdr._anonymous_env" => {
            for (name, value) in input
                .get("env")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                std::env::set_var(name, value.as_str().unwrap_or(""));
            }
            let env = Herdr::anonymous_env();
            Ok(json!({
                "has_upper": env.contains_key("HC_TERMINAL_ID"),
                "has_lower": env.contains_key("hc_terminal_id"),
                "has_mixed": env.contains_key("Hc_Terminal_Id"),
                "home": env.get("HOME").cloned(),
            }))
        }
        "Herdr._json_command" => {
            let herdr = build();
            let timeout = number_or_zero(input.get("timeout"));
            let value = herdr
                .json_command(&argv(input.get("argv")), timeout)
                .map_err(|error| error.message().to_string())?;
            Ok(json!({"value": value, "calls": world.calls()}))
        }
        "Herdr._spawn" => {
            world.pretend_installed();
            let herdr = build();
            let result = herdr
                .spawn(
                    &argv(input.get("argv")),
                    number_or_zero(input.get("timeout")),
                    false,
                )
                .map_err(|error| error.message().to_string())?;
            Ok(json!({"stdout": result.stdout, "calls": world.calls()}))
        }
        "Herdr._replacement_for" => {
            let herdr = build();
            let executable = text(input.get("executable"));
            let replacement = herdr.replacement_for(&executable);
            let name = Path::new(&executable)
                .file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let installed = usize::from(matches!(
                name.as_str(),
                "hc" | "hc.exe" | "herdr-app" | "herdr-app.exe"
            ));
            Ok(json!({"replacement": replacement, "installed_calls": installed}))
        }
        "Herdr._resolve_paths" => {
            if input.get("programs").is_some() {
                let programs = Path::new(temp).join("programs-case");
                for relative in ["herdr-core/hc.exe", "herdr-app/herdr-app.exe"] {
                    let path = programs.join("Programs").join(relative);
                    std::fs::create_dir_all(path.parent().unwrap_or(&programs)).expect("carpeta");
                    std::fs::write(&path, b"x").expect("binario");
                }
                std::env::set_var("LOCALAPPDATA", &programs);
            }
            let herdr = build();
            Ok(json!({
                "hc_path": herdr.hc_path(),
                "herdr_app_path": herdr.herdr_app_path(),
            }))
        }
        "Herdr._run_installer" => {
            let herdr = build();
            let ok = herdr.run_installer(&text(input.get("script")));
            Ok(json!({"ok": ok, "calls": world.calls()}))
        }
        "Herdr.ensure_installed" => {
            let herdr = build();
            Ok(json!({"ready": herdr.ensure_installed(), "calls": world.calls()}))
        }
        "Herdr.list_agents" => {
            let agents = build()
                .list_agents(20.0)
                .map_err(|error| error.message().to_string())?;
            Ok(json!({"agents": agents, "calls": world.calls()}))
        }
        "Herdr.get_agent" => {
            let agent = build()
                .get_agent(&text(input.get("address")))
                .map_err(|error| error.message().to_string())?;
            Ok(json!({"agent": agent, "calls": world.calls()}))
        }
        "Herdr.read_agent" => {
            let herdr = build();
            let lines = opt_number(input.get("lines")).map(|value| value as i64);
            let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(6000);
            let text =
                Herdr::read_agent(&herdr, &text(input.get("address")), lines, limit as usize)
                    .map_err(|error| error.message().to_string())?;
            Ok(json!({"text": text, "calls": world.calls()}))
        }
        "Herdr.read_full" => {
            let herdr = build();
            let lines = number_or_zero(input.get("lines")) as i64;
            let text = herdr
                .read_full(&text(input.get("address")), lines)
                .map_err(|error| error.message().to_string())?;
            Ok(json!({"text": text, "calls": world.calls()}))
        }
        "Herdr.label_for" => {
            let herdr = build();
            let cache = match input.get("cache") {
                Some(Value::Array(_)) => Some(rows(input.get("cache"))),
                _ => None,
            };
            herdr.seed_context(
                cache,
                None,
                input.get("snapshot").and_then(Value::as_object).cloned(),
            );
            let address = match input.get("address") {
                Some(Value::String(text)) => text.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            Ok(json!({"label": herdr.label_for(&address)}))
        }
        "Herdr.cached_agents" => cached_agents_case(&world, &build(), input),
        "Herdr.context_text" => context_case(&world, &build(), input),
        "Herdr.send_agent" => {
            std::env::set_var("HC_TERMINAL_ID", text(input.get("env_hc")));
            let chars = input.get("chars").and_then(Value::as_u64).unwrap_or(0) as usize;
            let payload = if chars == 4 {
                "hola".to_string()
            } else {
                "x".repeat(chars)
            };
            let result = build()
                .send_agent(&text(input.get("address")), &payload, false)
                .map_err(|error| error.message().to_string())?;
            Ok(json!({"result": result, "calls": world.calls()}))
        }
        "Herdr.save_context" => save_context_case(case, input, temp, &world),
        "Herdr.keys_agent" => {
            let result = build()
                .keys_agent(&text(input.get("address")), &argv(input.get("keys")))
                .map_err(|error| error.message().to_string())?;
            Ok(json!({"result": result, "calls": world.calls()}))
        }
        "Herdr.open_terminal" => {
            let cwd = input.get("cwd").and_then(Value::as_str);
            let space = input.get("space").and_then(Value::as_str);
            let result = build()
                .open_terminal(
                    &text(input.get("name")),
                    cwd,
                    &text(input.get("shell")),
                    space,
                )
                .map_err(|error| error.message().to_string())?;
            Ok(json!({"result": result, "calls": world.calls()}))
        }
        "Herdr.focus_agent" => {
            let result = build()
                .focus_agent(&text(input.get("address")))
                .map_err(|error| error.message().to_string())?;
            Ok(json!({"result": result, "calls": world.calls()}))
        }
        "Herdr.kill_agent" => {
            let result = build()
                .kill_agent(&text(input.get("address")), false)
                .map_err(|error| error.message().to_string())?;
            Ok(json!({"result": result, "calls": world.calls()}))
        }
        "Herdr.run_agent" => super::launch::run_agent_case(&world, case, temp),
        other => Err(format!("función sin portar: {other}")),
    }
}

pub(super) fn cached_agents_case(
    world: &Arc<World>,
    herdr: &Herdr,
    input: &Value,
) -> Result<Value, String> {
    let agents = rows(input.get("agents"));
    let max_age = number_or_zero(input.get("max_age"));
    let _ = agents;
    let initial = herdr.cached_agents(max_age);
    herdr.refresh_context();
    let mut cached = herdr.cached_agents(max_age).unwrap_or_default();
    if let Some(first) = cached.first_mut() {
        first.insert("agent".to_string(), Value::from("mutado"));
    }
    let untouched = herdr.cached_agents(max_age);
    world.advance(2.0);
    let stale = herdr.cached_agents(max_age);
    herdr.refresh_context();
    let latest = herdr.cached_agents(max_age);
    herdr.seed_context(None, Some(world.now()), None);
    let none_after = herdr.cached_agents(max_age);
    herdr.seed_context(Some(vec![object(json!({"address": "b"}))]), None, None);
    let no_at = herdr.cached_agents(max_age);
    Ok(json!({
        "initial": initial,
        "cached": cached,
        "untouched": untouched,
        "stale": stale,
        "latest": latest,
        "none_after": none_after,
        "no_at": no_at,
        "calls": world.hook_calls(),
    }))
}

pub(super) fn parse_payload(text: &str) -> Option<Value> {
    let start = text.find('\n')?;
    let body = &text[start + 1..];
    let marker = "<untrusted_agent_windows>\n";
    let begin = body.find(marker)? + marker.len();
    let end = body.rfind("\n</untrusted_agent_windows>")?;
    serde_json::from_str(&body[begin..end]).ok()
}

pub(super) fn context_case(
    world: &Arc<World>,
    herdr: &Arc<Herdr>,
    input: &Value,
) -> Result<Value, String> {
    if let Some(limits) = input.get("field_limits") {
        let _ = limits;
        let first = herdr.context_text();
        let second = herdr.context_text();
        let third = herdr.context_text();
        let snapshot = herdr.context_snapshot();
        let payload = parse_payload(&second);
        let terminals = payload
            .as_ref()
            .and_then(|payload| payload.get("terminals"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let has_where = terminals.iter().any(|item| item.get("where").is_some());
        return Ok(json!({
            "first": first,
            "second": second,
            "third": third,
            "threads": world.threads(),
            "snapshot_total": snapshot.as_ref().and_then(|snapshot| snapshot.get("total")).cloned(),
            "terminals": terminals.len(),
            "has_where": has_where,
        }));
    }
    if input.get("agents_error").is_some() {
        let one = herdr.context_text();
        let two = herdr.context_text();
        world.advance(number_or_zero(input.get("advance")));
        let three = herdr.context_text();
        return Ok(json!({
            "one": one,
            "two": two,
            "three": three,
            "calls": world.timeout_calls(),
            "attempts": world.threads(),
            "snapshot": herdr.context_snapshot(),
        }));
    }
    if input.get("calls").is_some() {
        world.defer_threads();
        let first = herdr.context_text();
        let second = herdr.context_text();
        let third = herdr.context_text();
        let pending = herdr.context_pending();
        herdr.refresh_context();
        let after = herdr.context_text();
        return Ok(json!({
            "first": first,
            "second": second,
            "third": third,
            "pending": pending,
            "spawned": world.threads(),
            "after": after,
        }));
    }
    let _ = herdr.context_text();
    let fresh = herdr.context_text();
    world.advance(number_or_zero(input.get("advance")));
    let stale = herdr.context_text();
    Ok(json!({"fresh": fresh, "stale": stale}))
}

pub(super) fn save_context_case(
    case: &FixtureCase,
    input: &Value,
    temp: &str,
    world: &Arc<World>,
) -> Result<Value, String> {
    let suffix = case
        .case
        .rsplit('/')
        .next()
        .and_then(|name| name.split_once('-').map(|(_, rest)| rest))
        .unwrap_or("");
    let folder = Path::new(temp).join("save").join(suffix);
    std::fs::create_dir_all(&folder).expect("carpeta del caso");
    std::env::set_var("LOCALAPPDATA", &folder);
    let herdr = || Arc::new(Herdr::with_runtime(config(temp, input), runtime_of(world)));
    if input.get("broken").is_some() {
        std::fs::write(folder.join("EzAgent"), b"no es carpeta").expect("archivo");
        return Ok(json!({"path": herdr().save_context(&text(input.get("text")))}));
    }
    if let Some(files) = input.get("files").and_then(Value::as_object) {
        let handoff = folder.join("EzAgent").join("handoff");
        std::fs::create_dir_all(&handoff).expect("handoff");
        let now = SystemTime::now();
        for (name, days) in files {
            let path = handoff.join(name);
            std::fs::write(&path, name).expect("archivo viejo");
            let offset = days.as_f64().unwrap_or(0.0) * 86_400.0;
            let stamp = if offset < 0.0 {
                now - Duration::from_secs_f64(-offset)
            } else {
                now + Duration::from_secs_f64(offset)
            };
            std::fs::File::options()
                .write(true)
                .open(&path)
                .expect("abrir")
                .set_modified(stamp)
                .expect("mtime");
        }
    }
    let herdr = Arc::new(Herdr::with_runtime(config(temp, input), runtime_of(world)));
    let path = herdr.save_context(&text(input.get("text")));
    if input.get("files").is_some() {
        let mut names: Vec<String> = std::fs::read_dir(folder.join("EzAgent").join("handoff"))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().is_file())
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        return Ok(json!({"path": path, "files": names}));
    }
    Ok(json!({
        "path": path,
        "files": files_under(&folder),
    }))
}

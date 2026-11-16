//! Lectura y escritura de los artefactos del perfil por las APIs del port.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ez_agent::audit::{AuditEntry, AuditLog};
use ez_agent::journal::Journal;
use ez_agent::memory::{MemoryOptions, MemoryStore, MemoryTimestamp};
use ez_agent::system_prompt::{Callbacks, SystemPromptBuilder};
use ez_chat::store::{ChatEntry, ChatStore, SessionInfo};
use ez_core::agent_config::AgentConfig;
use ez_core::config::VoiceConfig;
use ez_core::llm_providers::{
    load_provider_catalog, resolve_llm, CatalogEntry, ResolveArgs, SaveArgs,
};
use ez_core::metrics::LatencyStats;
use ez_core::pyvalue::py_splitlines;
use ez_herdr::tasks::TaskRegistry;
use ez_swarm::db::Db;
use serde_json::{json, Map, Value};

/// Ejecuta un comando del arnés.
pub fn run(command: &str, flags: &BTreeMap<String, String>) -> Result<Value, String> {
    let home = home(flags)?;
    match command {
        "dump-voice" => dump_voice(&home),
        "save-voice" => save_voice(&home, flags),
        "dump-llm" => dump_llm(&home),
        "save-llm" => save_llm(&home),
        "dump-agent" => dump_agent(&home),
        "dump-system" => dump_system(&home),
        "dump-chat" => dump_chat(&home, flags),
        "save-chat" => save_chat(&home),
        "dump-tasks" => dump_tasks(&home),
        "save-tasks" => save_tasks(&home),
        "dump-journal" => dump_journal(&home),
        "save-journal" => save_journal(&home),
        "dump-memory" => dump_memory(&home, flags),
        "save-memory" => save_memory(&home),
        "dump-board" => dump_board(&home, flags),
        "save-board" => save_board(&home),
        "dump-logs" => dump_logs(&home),
        "append-logs" => append_logs(&home),
        "write-opencode" => write_opencode(flags),
        other => Err(format!("comando desconocido: {other}")),
    }
}

fn flag(flags: &BTreeMap<String, String>, name: &str) -> Result<String, String> {
    flags
        .get(name)
        .cloned()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("falta --{name}"))
}

fn flag_or(flags: &BTreeMap<String, String>, name: &str, default: &str) -> String {
    flags
        .get(name)
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

fn home(flags: &BTreeMap<String, String>) -> Result<PathBuf, String> {
    flag(flags, "home").map(PathBuf::from)
}

// ---------------------------------------------------------------- ajustes
fn dump_voice(home: &Path) -> Result<Value, String> {
    let config = VoiceConfig::load(Some(&home.join("voice.json")))?;
    Ok(json!({ "values": config.to_value() }))
}

fn save_voice(home: &Path, flags: &BTreeMap<String, String>) -> Result<Value, String> {
    let mut config = VoiceConfig::load(Some(&home.join("voice.json")))?;
    let pair = flag_or(flags, "set", "ui_scale=1.05");
    let (key, raw) = pair
        .split_once('=')
        .ok_or_else(|| "--set espera clave=valor".to_string())?;
    let value =
        serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_string()));
    config.set(key, value);
    config.save(Some(&home.join("voice.json")))?;
    Ok(json!({ "ok": true }))
}

fn catalog_entry(entry: &CatalogEntry) -> Value {
    let mut map = Map::new();
    map.insert("label".into(), json!(entry.label));
    map.insert("base_url".into(), json!(entry.base_url));
    map.insert("model".into(), json!(entry.model));
    map.insert("api_key".into(), json!(entry.api_key));
    map.insert("api_style".into(), json!(entry.api_style));
    if let Some(provider) = &entry.provider {
        map.insert("provider".into(), json!(provider));
    }
    if entry.saved {
        map.insert("saved".into(), json!("true"));
    }
    Value::Object(map)
}

fn dump_llm(home: &Path) -> Result<Value, String> {
    let path = home.join("llm.json");
    let catalog = load_provider_catalog(None, Some(&path))?;
    let mut entries = Map::new();
    for (name, entry) in &catalog {
        entries.insert(name.clone(), catalog_entry(entry));
    }
    let resolved = resolve_llm(&ResolveArgs {
        config_file: Some(&path),
        ..ResolveArgs::default()
    })?;
    Ok(json!({
        "order": catalog.keys().collect::<Vec<_>>(),
        "catalog": entries,
        "resolved": {
            "provider": resolved.provider,
            "model": resolved.model,
            "base_url": resolved.base_url,
            "api_key": resolved.api_key,
            "source": resolved.source,
            "api_style": resolved.api_style,
        },
    }))
}

fn save_llm(home: &Path) -> Result<Value, String> {
    let path = home.join("llm.json");
    let name = ez_core::llm_providers::save_provider_profile(&SaveArgs {
        provider: "compat-provider",
        model: "compat-model",
        base_url: "http://127.0.0.1:9/v1",
        api_key: "clave-de-prueba",
        api_style: "anthropic",
        profile_name: "Compat R1",
        config_file: Some(&path),
    })?;
    Ok(json!({ "provider": name }))
}

fn dump_agent(home: &Path) -> Result<Value, String> {
    let (config, warnings) = AgentConfig::load(Some(&home.join("permissions.toml")));
    let names = json!({
        "app_dir": config.app_dir.display().to_string(),
        "workspace": config.workspace.display().to_string(),
        "permissions_path": config.permissions_path.display().to_string(),
        "system_md_path": config.system_md_path.display().to_string(),
        "memory_dir": config.memory_dir.display().to_string(),
        "history_path": config.history_path.display().to_string(),
        "audit_path": config.audit_path.display().to_string(),
        "journal_dir": config.journal_dir.display().to_string(),
        "skills_dir": config.skills_dir.display().to_string(),
        "global_skills_dir": config.global_skills_dir.display().to_string(),
        "tools_d_dir": config.tools_d_dir.display().to_string(),
    });
    Ok(json!({
        "general": {
            "max_tool_calls_per_turn": config.general.max_tool_calls_per_turn,
            "confirm_timeout_s": config.general.confirm_timeout_s,
            "bulk_threshold": config.general.bulk_threshold,
            "max_file_mb": config.general.max_file_mb,
        },
        "paths": {
            "read_roots": config.paths.read_roots,
            "write_roots": config.paths.write_roots,
            "deny": config.paths.deny,
            "deny_globs": config.paths.deny_globs,
        },
        "confirm": { "accept": config.confirm.accept, "reject": config.confirm.reject },
        "web": {
            "max_fetch_chars": config.web.max_fetch_chars,
            "block_private_ips": config.web.block_private_ips,
        },
        "skills": {
            "allow_scripts": config.skills.allow_scripts,
            "trusted": config.skills.trusted,
        },
        "tools_d": { "allowed_executables": config.tools_d.allowed_executables },
        "names": names,
        "warnings": warnings,
    }))
}

fn dump_system(home: &Path) -> Result<Value, String> {
    let builder = SystemPromptBuilder::new(&home.join("system.md"), Callbacks::default());
    Ok(json!({ "text": builder.last_valid_text() }))
}

// ------------------------------------------------------------------ chat
fn chat_entry(entry: &ChatEntry) -> Value {
    json!({
        "t": entry.t,
        "session": entry.session,
        "role": entry.role.as_str(),
        "source": entry.source.as_str(),
        "text": entry.text,
        "seq": entry.seq,
    })
}

fn session_info(info: &SessionInfo) -> Value {
    json!({
        "session": info.session,
        "started": info.started,
        "messages": info.messages,
        "title": info.title,
    })
}

fn dump_chat(home: &Path, flags: &BTreeMap<String, String>) -> Result<Value, String> {
    let store = ChatStore::new(Some(chat_path(home)));
    let sessions = store.sessions();
    let mut by_session = Map::new();
    for info in &sessions {
        let entries: Vec<Value> = store
            .entries(Some(info.session))
            .iter()
            .map(chat_entry)
            .collect();
        by_session.insert(info.session.to_string(), Value::Array(entries));
    }
    let query = flag_or(flags, "query", "");
    let recent: Vec<Value> = store.recent(10).iter().map(chat_entry).collect();
    let search: Vec<Value> = store.search(&query, 5).iter().map(chat_entry).collect();
    Ok(json!({
        "path": store.path().display().to_string(),
        "session": store.session(),
        "len": store.len(),
        "entries": by_session,
        "sessions": sessions.iter().map(session_info).collect::<Vec<_>>(),
        "recent": recent,
        "search": search,
    }))
}

fn chat_path(home: &Path) -> PathBuf {
    home.join("history").join("chat.jsonl")
}

fn save_chat(home: &Path) -> Result<Value, String> {
    let store = ChatStore::new(Some(chat_path(home)));
    let first = store.session();
    let appended = store.append(
        ez_chat::store::Role::User,
        "mensaje de compatibilidad F7-03",
        ez_chat::store::Source::Chat,
        Some(1_700_000_000.125),
    );
    let second = store.new_session();
    let reply = store.append(
        ez_chat::store::Role::Assistant,
        "respuesta de compatibilidad F7-03",
        ez_chat::store::Source::Voice,
        Some(1_700_000_001.5),
    );
    let replaced = store.replace_last(
        ez_chat::store::Role::Assistant,
        "respuesta de compatibilidad F7-03",
        "respuesta corregida de compatibilidad F7-03",
    );
    let resumed = store.resume(first);
    let mut out = Map::new();
    out.insert("appended".into(), chat_entry(&appended));
    out.insert("reply".into(), chat_entry(&reply));
    out.insert("replaced".into(), json!(replaced));
    out.insert("new_session".into(), json!(second));
    out.insert("resumed".into(), json!(resumed));
    out.insert("final_session".into(), json!(store.session()));
    Ok(Value::Object(out))
}

// ----------------------------------------------------------------- tareas
fn dump_tasks(home: &Path) -> Result<Value, String> {
    let registry = TaskRegistry::new(home.join("tasks.json"));
    Ok(json!({
        "tasks": registry.all(),
        "chains": registry.chains(),
        "prompt": registry.prompt_block(),
    }))
}

fn save_tasks(home: &Path) -> Result<Value, String> {
    let registry = TaskRegistry::new(home.join("tasks.json"));
    let task = registry
        .add(
            "compat:1",
            "bob",
            "tarea de compatibilidad",
            Some(1_700_000_100.0),
        )
        .map_err(|error| error.to_string())?;
    let mut chain = Map::new();
    chain.insert("source".into(), json!("compat:1"));
    chain.insert("agent".into(), json!("cy"));
    chain.insert("task".into(), json!("etapa de compatibilidad"));
    chain.insert("created_at".into(), json!(1_700_000_101.0));
    let saved_chain = registry
        .add_chain(&chain)
        .map_err(|error| error.to_string())?;
    Ok(json!({ "task": task, "chain": saved_chain }))
}

// ----------------------------------------------------------------- diario
fn dump_journal(home: &Path) -> Result<Value, String> {
    let journal = Journal::new(Some(&home.join("journal")))?;
    Ok(json!({ "entries": journal.load_entries_value() }))
}

fn save_journal(home: &Path) -> Result<Value, String> {
    let journal = Journal::new(Some(&home.join("journal")))?;
    let target = home.join("compat-target.txt");
    let id = journal.record_create(&target, "create", "Prueba de compatibilidad F7-03")?;
    Ok(json!({ "id": id }))
}

// ----------------------------------------------------------------- memoria
fn dump_memory(home: &Path, flags: &BTreeMap<String, String>) -> Result<Value, String> {
    let store = memory_store(home)?;
    let query = flag_or(flags, "query", "");
    let recall = store.recall(&query)?;
    let backend = store.last_backend();
    Ok(json!({
        "count": store.count_messages()?,
        "fts": store.fts_enabled,
        "backend": backend,
        "user": store.read_markdown("user"),
        "memory": store.read_markdown("memory"),
        "recall": recall,
        "block": store.memory_block(),
    }))
}

fn memory_store(home: &Path) -> Result<MemoryStore, String> {
    let memory_dir = home.join("memory");
    MemoryStore::from_paths(
        &memory_dir,
        memory_dir.join("history.db"),
        MemoryOptions::default(),
    )
}

fn save_memory(home: &Path) -> Result<Value, String> {
    let store = memory_store(home)?;
    let saved = store.record(
        "user",
        "mensaje de compatibilidad F7-03",
        Some("compat"),
        &MemoryTimestamp::Text("2026-11-01T00:00:00+00:00".into()),
    )?;
    let remembered = store.remember("memory", "hecho de compatibilidad F7-03")?;
    Ok(json!({
        "ok": saved,
        "remembered": remembered.status,
        "count": store.count_messages()?,
    }))
}

// ----------------------------------------------------------------- tablero
fn dump_board(home: &Path, flags: &BTreeMap<String, String>) -> Result<Value, String> {
    let db = Db::open(home.join("swarm").join("board.db")).map_err(|error| error.to_string())?;
    let mut out = Map::new();
    out.insert(
        "live".into(),
        json!(db.live().map_err(|error| error.to_string())?),
    );
    out.insert(
        "latest".into(),
        json!(db.latest().map_err(|error| error.to_string())?),
    );
    if let Some(swarm) = flags.get("swarm").filter(|value| !value.is_empty()) {
        let info = db.get(swarm).map_err(|error| error.to_string())?;
        let members = db.members(swarm).map_err(|error| error.to_string())?;
        let tasks = db.tasks(swarm).map_err(|error| error.to_string())?;
        let messages = db
            .history(swarm, 0, 1_000)
            .map_err(|error| error.to_string())?;
        let leases = db.leases(swarm).map_err(|error| error.to_string())?;
        out.insert("info".into(), json!(info));
        out.insert("members".into(), json!(members));
        out.insert("tasks".into(), json!(tasks));
        out.insert("messages".into(), json!(messages));
        out.insert("leases".into(), json!(leases));
    }
    Ok(Value::Object(out))
}

fn save_board(home: &Path) -> Result<Value, String> {
    let db = Db::open(home.join("swarm").join("board.db")).map_err(|error| error.to_string())?;
    let swarm = db
        .create("Compatibilidad F7-03", "", "active")
        .map_err(|error| error.to_string())?;
    let member = db
        .join(
            &swarm, "ana", "dev", "opencode", "compat:1", "model-x", "high",
        )
        .map_err(|error| error.to_string())?;
    let task = db
        .add_task(&swarm, "Tarea de compatibilidad", "dev", "", &[], 0)
        .map_err(|error| error.to_string())?;
    let (message, _clipped) = db
        .post(&swarm, "ana", "mensaje de compatibilidad", "*", "info", "")
        .map_err(|error| error.to_string())?;
    Ok(json!({ "swarm": swarm, "member": member, "task": task, "message": message }))
}

// --------------------------------------------------------------------- logs
fn summarize_jsonl(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut keys = BTreeSet::new();
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut lines = 0;
    let mut bad = 0;
    for line in py_splitlines(&text) {
        if line.trim().is_empty() {
            continue;
        }
        lines += 1;
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Object(map)) => {
                keys.extend(map.keys().cloned());
                if let Some(kind) = map.get("kind").and_then(Value::as_str) {
                    *kinds.entry(kind.to_string()).or_default() += 1;
                }
            }
            _ => bad += 1,
        }
    }
    json!({ "lines": lines, "bad": bad, "keys": keys, "kinds": kinds })
}

fn dump_logs(home: &Path) -> Result<Value, String> {
    let logs = home.join("logs");
    Ok(json!({
        "audit": summarize_jsonl(&logs.join("audit.jsonl")),
        "latency": summarize_jsonl(&logs.join("latency.jsonl")),
    }))
}

fn append_logs(home: &Path) -> Result<Value, String> {
    let logs = home.join("logs");
    let audit = AuditLog::new(&logs.join("audit.jsonl"));
    let args = json!({ "command": "compat" });
    audit.log_at(
        &AuditEntry {
            turn_id: "compat-r1",
            tool: "compat",
            level: 1,
            args: &args,
            decision: "allow",
            taint: false,
            result_ok: true,
            ms: 12.34,
            error_kind: None,
        },
        "2026-01-02T03:04:05.678+00:00",
    );
    let latency = LatencyStats::new(50, Some(logs.join("latency.jsonl")));
    latency.record_command("compat", 12.34);
    Ok(json!({ "ok": true }))
}

/// Estado de OpenCode (`$LOCALAPPDATA`), que la app escribe para el CLI.
fn write_opencode(flags: &BTreeMap<String, String>) -> Result<Value, String> {
    let reference = flag_or(flags, "ref", "opencode-go/modelo-de-compatibilidad");
    let effort = flag_or(flags, "effort", "high");
    let env = ez_herdr::prelaunch::prepare_opencode_state(&reference, &effort);
    Ok(json!({ "env": env }))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

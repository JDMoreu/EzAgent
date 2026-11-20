//! Invented usage data for every reader of `ez usage`.
//!
//! All files live under the sandbox: no real conversation, usage or credential
//! is read. The formats are the ones documented in IR-0 and exercised by the
//! `ez-scan` tests.

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, Connection};
use serde_json::json;

use super::setup::{self, Sandbox, MCODE_MODEL, OPENCODE_FREE_MODEL, OPENCODE_GO_MODEL};

/// Tokens of the Claude Code `sonnet` messages (input + output + cache).
pub const CLAUDE_SONNET_TOKENS: u64 = 180;
/// Tokens of the Claude Code `opus` messages.
pub const CLAUDE_OPUS_TOKENS: u64 = 360;
/// Tokens of the OpenCode Go model.
pub const OPENCODE_GO_TOKENS: u64 = 180;
/// Tokens of the OpenCode free model.
pub const OPENCODE_FREE_TOKENS: u64 = 100;
/// Tokens of the mcode rows.
pub const MCODE_TOKENS: u64 = 280;
/// Tokens of the Pi messages.
pub const PI_TOKENS: u64 = 280;

/// Writes the Claude Code, OpenCode, mcode and Pi fixtures.
pub fn readers(sandbox: &Sandbox) -> Result<(), String> {
    let now = Utc::now();
    write_claude(sandbox, now)?;
    write_opencode(sandbox, now)?;
    write_mcode(sandbox)?;
    write_pi(sandbox, now)?;
    Ok(())
}

/// Writes a Codex session with a `rate_limits` row.
///
/// `resets_seconds` is an absolute epoch in seconds, as Codex stores it.
pub fn codex_session(
    sandbox: &Sandbox,
    file: &str,
    observed: DateTime<Utc>,
    used_percent: u32,
    resets_seconds: i64,
) -> Result<(), String> {
    let record = json!({
        "timestamp": rfc3339(observed),
        "payload": {
            "rate_limits": {
                "primary": {
                    "used_percent": used_percent,
                    "window_minutes": 300,
                    "resets_at": resets_seconds
                },
                "plan_type": "plus"
            }
        }
    });
    let path = sandbox
        .codex
        .join("sessions")
        .join("2026")
        .join("11")
        .join(file);
    setup::write_text(&path, &format!("{}\n", record))
}

/// Writes the watcher's `uso.json` with the free OpenCode model.
pub fn depleted_free_model(sandbox: &Sandbox) -> Result<(), String> {
    let now = Utc::now().timestamp_millis().max(0);
    let entries = json!([{
        "agente": "opencode",
        "modelo": format!("opencode/{OPENCODE_FREE_MODEL}"),
        "hasta": null,
        "visto_ms": now
    }]);
    setup::write_json(&sandbox.subagentes.join("uso.json"), &entries)
}

/// Seeds the registry with the previous `developer` of the fake parent.
///
/// The record is already `terminado` and notified, so the watcher ignores it;
/// `ez recommend qa_validator` uses its model family.
pub fn previous_developer(sandbox: &Sandbox, parent: &str) -> Result<(), String> {
    let since = Utc::now().timestamp_millis().saturating_sub(3_600_000);
    let record = json!({
        "name": "dev0-opencode",
        "terminal": "t-anterior",
        "parent": parent,
        "agent": "opencode",
        "label": "OpenCode",
        "model": format!("opencode-go/{OPENCODE_GO_MODEL}"),
        "effort": "high",
        "role": "developer",
        "cwd": setup::path_text(&sandbox.work),
        "brief_head": "encargo anterior de la prueba",
        "state": "terminado",
        "since_ms": since,
        "history": [{
            "state": "terminado",
            "at_ms": since,
            "by": "prueba",
            "detail": "el subagente terminó"
        }],
        "summary": "ok",
        "report": null,
        "validation": null,
        "notified": "terminado"
    });
    let folder = sandbox.subagentes.join(parent);
    setup::write_json(&folder.join("dev0-opencode.json"), &record)
}

/// Claude Code: two `message.usage` records and the plan in `~/.claude.json`.
fn write_claude(sandbox: &Sandbox, now: DateTime<Utc>) -> Result<(), String> {
    let path = sandbox
        .home
        .join(".claude")
        .join("projects")
        .join("prueba")
        .join("sesion.jsonl");
    let first = json!({
        "timestamp": rfc3339(now - Duration::minutes(30)),
        "message": {
            "model": "sonnet",
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_read_input_tokens": 20,
                "cache_creation_input_tokens": 10
            }
        }
    });
    let second = json!({
        "timestamp": rfc3339(now - Duration::minutes(20)),
        "message": {
            "model": "opus",
            "usage": {
                "input_tokens": 200,
                "output_tokens": 100,
                "cache_read_input_tokens": 40,
                "cache_creation_input_tokens": 20
            }
        }
    });
    setup::write_text(&path, &format!("{first}\n{second}\n"))?;
    setup::write_json(
        &sandbox.home.join(".claude.json"),
        &json!({"oauthAccount": {"organizationType": "claude_pro"}}),
    )
}

/// OpenCode: the `session_message` table, a Free notice and later Go usage.
fn write_opencode(sandbox: &Sandbox, now: DateTime<Utc>) -> Result<(), String> {
    let folder = sandbox.home.join(".local").join("share").join("opencode");
    std::fs::create_dir_all(folder.join("log"))
        .map_err(|error| format!("No pude crear {}: {error}", folder.display()))?;
    let connection = Connection::open(folder.join("opencode.db"))
        .map_err(|error| format!("No pude abrir opencode.db: {error}"))?;
    connection
        .execute_batch(
            "CREATE TABLE session_message (
                id TEXT, session_id TEXT, type TEXT, seq INTEGER,
                time_created INTEGER, time_updated INTEGER, data TEXT
            );",
        )
        .map_err(|error| format!("No pude crear session_message: {error}"))?;
    insert_opencode(
        &connection,
        now - Duration::minutes(60),
        "opencode",
        OPENCODE_FREE_MODEL,
        [10, 20, 30, 40],
        0.0,
    )?;
    insert_opencode(
        &connection,
        now - Duration::minutes(10),
        "opencode-go",
        OPENCODE_GO_MODEL,
        [100, 50, 20, 10],
        0.5,
    )?;
    drop(connection);
    let notice = format!(
        "timestamp={} level=ERROR run=fake message=\"Failed to drain Session\" cause=\"AI.Error: \
         Free usage limit reached. Free models work in OpenCode; subscribe to OpenCode Go to keep \
         going.\"",
        rfc3339(now - Duration::minutes(50))
    );
    setup::write_text(
        &folder.join("log").join("opencode.log"),
        &format!("{notice}\n"),
    )
}

/// One assistant row of `session_message`.
fn insert_opencode(
    connection: &Connection,
    at: DateTime<Utc>,
    provider: &str,
    model: &str,
    tokens: [i64; 4],
    cost: f64,
) -> Result<(), String> {
    let data = json!({
        "model": {"providerID": provider, "id": model, "variant": "high"},
        "tokens": {
            "input": tokens[0], "output": tokens[1], "reasoning": 0,
            "cache": {"read": tokens[2], "write": tokens[3]}
        },
        "cost": cost
    });
    connection
        .execute(
            "INSERT INTO session_message
             (id, session_id, type, seq, time_created, time_updated, data)
             VALUES (?1, 's', 'assistant', 0, ?2, ?2, ?3)",
            params![
                format!("m-{model}"),
                at.timestamp_millis(),
                data.to_string()
            ],
        )
        .map_err(|error| format!("No pude insertar la fila de OpenCode: {error}"))?;
    Ok(())
}

/// mcode: two rows of `local_runtime_token_usage`.
fn write_mcode(sandbox: &Sandbox) -> Result<(), String> {
    let folder = sandbox.minimax.join("v2").join("sqlite");
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("No pude crear {}: {error}", folder.display()))?;
    let connection = Connection::open(folder.join("runtime-state.sqlite"))
        .map_err(|error| format!("No pude abrir la base de mcode: {error}"))?;
    connection
        .execute_batch(
            "CREATE TABLE local_runtime_token_usage (
                model TEXT, input_tokens INTEGER, output_tokens INTEGER,
                cache_read_tokens INTEGER, cache_write_tokens INTEGER,
                cost_usd REAL, ts INTEGER
            );",
        )
        .map_err(|error| format!("No pude crear local_runtime_token_usage: {error}"))?;
    let now = Utc::now().timestamp_millis();
    for (offset, tokens, cost) in [(30, [100, 50, 20, 10], 0.5), (10, [10, 20, 30, 40], 0.25)] {
        connection
            .execute(
                "INSERT INTO local_runtime_token_usage
                 (model, input_tokens, output_tokens, cache_read_tokens,
                  cache_write_tokens, cost_usd, ts)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    MCODE_MODEL,
                    tokens[0],
                    tokens[1],
                    tokens[2],
                    tokens[3],
                    cost,
                    now - offset * 60_000
                ],
            )
            .map_err(|error| format!("No pude insertar la fila de mcode: {error}"))?;
    }
    Ok(())
}

/// Pi: two `message.usage` records with cost.
fn write_pi(sandbox: &Sandbox, now: DateTime<Utc>) -> Result<(), String> {
    let path = sandbox
        .home
        .join(".pi")
        .join("agent")
        .join("sessions")
        .join("prueba")
        .join("sesion.jsonl");
    let first = json!({
        "message": {
            "timestamp": (now - Duration::minutes(25)).timestamp_millis(),
            "model": "proveedor/libre",
            "usage": {
                "input": 10, "output": 20, "cacheRead": 30, "cacheWrite": 40,
                "cost": {"total": 0.25}
            }
        }
    });
    let second = json!({
        "message": {
            "timestamp": (now - Duration::minutes(5)).timestamp_millis(),
            "model": "proveedor/libre",
            "usage": {
                "input": 100, "output": 50, "cacheRead": 20, "cacheWrite": 10,
                "cost": {"total": 0.35}
            }
        }
    });
    setup::write_text(&path, &format!("{first}\n{second}\n"))
}

/// RFC 3339 with milliseconds, as the readers parse it.
fn rfc3339(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

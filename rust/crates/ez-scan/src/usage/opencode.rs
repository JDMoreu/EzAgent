//! Lector de OpenCode (§8.4 de IR-0): tokens y costo de su base, y el aviso de
//! límite de su registro (con la cuenta atrás al reinicio).
//!
//! La base se abre en solo lectura y con `query_only`; del registro solo se
//! leen las líneas de límite, nunca el contenido de las sesiones.

use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OpenFlags};

use super::jsonl;
use super::{Reading, TokenUsage, UsageDirs, Window};

const MAX_LOG_TAIL: u64 = 2 * 1024 * 1024;
const STALE_DB_MS: u64 = 24 * 3_600_000;

pub(crate) fn read(dirs: &UsageDirs, window: &Window, now_ms: u64) -> Reading {
    let mut reading = Reading::default();
    // DECISION: se lee la base en modo `ro` en vez de ejecutar
    // `opencode stats`: el brief prohíbe lanzar agentes reales.
    let db = dirs.opencode_data.join("opencode.db");
    if !db.is_file() {
        reading
            .warnings
            .push(format!("Sin base de OpenCode en {}.", db.display()));
    } else {
        match query(&db, window) {
            Ok(row) => {
                reading.tokens = Some(TokenUsage::new(
                    row.input,
                    row.output,
                    row.cache_read,
                    row.cache_write,
                ));
                reading.messages = Some(row.messages);
                reading.cost_usd = Some(row.cost);
                reading.sources.tokens =
                    Some("message.tokens de opencode.db (SQLite en solo lectura)".to_string());
                reading.sources.messages = Some("filas de message en la ventana".to_string());
                reading.sources.cost = Some("message.cost de opencode.db".to_string());
                if row.last_session > 0
                    && now_ms.saturating_sub(row.last_session as u64) > STALE_DB_MS
                {
                    let age = (now_ms - row.last_session as u64) / 3_600_000;
                    reading.warnings.push(format!(
                        "La base de OpenCode no se actualiza desde hace {age} h; puede no reflejar el uso de hoy."
                    ));
                }
            }
            Err(error) => {
                reading
                    .warnings
                    .push(format!("No pude leer el uso de OpenCode: {error}"));
            }
        }
    }
    if let Some(notice) = last_notice(&dirs.opencode_data.join("log").join("opencode.log")) {
        apply_notice(&mut reading, &notice, window, now_ms);
    }
    reading
}

fn apply_notice(reading: &mut Reading, notice: &Notice, window: &Window, now_ms: u64) {
    if reading.plan.is_none() {
        reading.plan = notice.plan.clone();
        if reading.plan.is_some() {
            reading.sources.plan = Some("mensaje de límite del registro de OpenCode".to_string());
        }
    }
    let Some(observed) = notice.observed_ms else {
        return;
    };
    let life_ms = notice
        .window_ms
        .unwrap_or((window.hours * 3_600_000.0) as u64);
    let recent = observed.saturating_add(life_ms) >= now_ms;
    let label = notice
        .tag
        .clone()
        .or_else(|| notice.plan.clone())
        .unwrap_or_else(|| "límite".to_string());
    if recent {
        reading.depleted = Some(format!(
            "aviso de límite de OpenCode en el registro ({label})"
        ));
        reading.resets_ms = notice.reset_ms;
        if notice.reset_ms.is_some() {
            reading.sources.reset =
                Some("cuenta atrás del mensaje de límite de OpenCode".to_string());
        }
    } else {
        let age = (now_ms - observed) / 3_600_000;
        reading.warnings.push(format!(
            "El último aviso de límite de OpenCode es de hace {age} h; ya venció y no lo cuento."
        ));
    }
}

struct DbRow {
    messages: u64,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    cost: f64,
    last_session: i64,
}

fn query(path: &Path, window: &Window) -> Result<DbRow, String> {
    let connection = open(path)?;
    let start = i64::try_from(window.start_ms).unwrap_or(i64::MAX);
    let sql = "SELECT COUNT(*), \
               COALESCE(SUM(json_extract(data, '$.tokens.input')), 0), \
               COALESCE(SUM(json_extract(data, '$.tokens.output')), 0), \
               COALESCE(SUM(json_extract(data, '$.tokens.cache.read')), 0), \
               COALESCE(SUM(json_extract(data, '$.tokens.cache.write')), 0), \
               COALESCE(SUM(json_extract(data, '$.cost')), 0.0) \
               FROM message WHERE time_created >= ?1";
    let (messages, input, output, cache_read, cache_write, cost) = connection
        .query_row(sql, params![start], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, f64>(5)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let messages = u64_of(messages);
    let input = u64_of(input);
    let output = u64_of(output);
    let cache_read = u64_of(cache_read);
    let cache_write = u64_of(cache_write);
    let last_session = connection
        .query_row(
            "SELECT COALESCE(MAX(time_updated), 0) FROM session",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    Ok(DbRow {
        messages,
        input,
        output,
        cache_read,
        cache_write,
        cost,
        last_session,
    })
}

fn u64_of(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn open(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    connection
        .busy_timeout(Duration::from_millis(2_000))
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch("PRAGMA query_only = 1;")
        .map_err(|error| error.to_string())?;
    Ok(connection)
}

/// Un aviso de límite del registro.
#[derive(Debug, Clone, Default, PartialEq)]
struct Notice {
    plan: Option<String>,
    tag: Option<String>,
    window_ms: Option<u64>,
    reset_ms: Option<u64>,
    observed_ms: Option<u64>,
}

/// El último aviso de límite de la cola del registro.
fn last_notice(path: &Path) -> Option<Notice> {
    let text = jsonl::read_tail(path, MAX_LOG_TAIL)?;
    let mut found = None;
    for line in text.lines() {
        if let Some(notice) = parse_notice(line) {
            found = Some(notice);
        }
    }
    found
}

fn parse_notice(line: &str) -> Option<Notice> {
    if !line.to_ascii_lowercase().contains("usage limit reached") {
        return None;
    }
    let observed_ms = line
        .split_whitespace()
        .find_map(|word| word.strip_prefix("timestamp="))
        .and_then(jsonl::parse_time_ms);
    let mut notice = Notice {
        plan: plan_of(line),
        observed_ms,
        ..Notice::default()
    };
    if line.contains("Free usage limit reached") {
        notice.plan = Some("Free".to_string());
        return Some(notice);
    }
    let after = line.split_once("usage limit reached (")?.1;
    let (tag, rest) = after.split_once(',')?;
    notice.tag = Some(tag.trim().to_string());
    notice.window_ms = window_of(tag);
    let countdown = rest.split_once("resets in ")?.1;
    let countdown: String = countdown
        .chars()
        .take_while(|character| character.is_ascii_digit() || "dhms ".contains(*character))
        .collect();
    if let (Some(observed), Some(span)) = (observed_ms, countdown_ms(&countdown)) {
        notice.reset_ms = Some(observed.saturating_add(span));
    }
    Some(notice)
}

fn plan_of(line: &str) -> Option<String> {
    // El mensaje de Go menciona «Go Plus» como mejora, no como plan actual.
    if line.contains("OpenCode Go Plus usage limit reached") {
        Some("OpenCode Go Plus".to_string())
    } else if line.contains("OpenCode Go usage limit reached") {
        Some("OpenCode Go".to_string())
    } else {
        None
    }
}

fn window_of(tag: &str) -> Option<u64> {
    let tag = tag.trim().to_lowercase();
    match tag.as_str() {
        "5-hour" | "5 hour" | "5-hour window" => Some(5 * 3_600_000),
        "weekly" => Some(7 * 24 * 3_600_000),
        "monthly" => Some(30 * 24 * 3_600_000),
        _ => None,
    }
}

/// `4h 40m`, `10d 3h` o `90m` en milisegundos.
fn countdown_ms(text: &str) -> Option<u64> {
    let mut total = 0u64;
    let mut number = 0u64;
    let mut saw_unit = false;
    for character in text.chars() {
        if character.is_ascii_digit() {
            number = number * 10 + u64::from(character as u8 - b'0');
            continue;
        }
        let factor = match character {
            'd' => 86_400_000,
            'h' => 3_600_000,
            'm' => 60_000,
            's' => 1_000,
            ' ' => continue,
            _ => return None,
        };
        total = total.saturating_add(number.saturating_mul(factor));
        number = 0;
        saw_unit = true;
    }
    (saw_unit && total > 0).then_some(total)
}

#[cfg(test)]
mod tests;

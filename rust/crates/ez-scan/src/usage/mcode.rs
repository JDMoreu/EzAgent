//! Lector de MiniMax Code (§8.5 de IR-0): su SQLite de uso, en solo lectura.
//!
//! La tabla `local_runtime_token_usage` guarda una fila por turno con `ts` en
//! milisegundos. No hay límite ni porcentaje local: eso solo se declara.

use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OpenFlags};

use super::{Reading, TokenUsage, UsageDirs, Window};

const TABLE: &str = "local_runtime_token_usage";

pub(crate) fn read(dirs: &UsageDirs, window: &Window, _now_ms: u64) -> Reading {
    let path = dirs
        .minimax
        .join("v2")
        .join("sqlite")
        .join("runtime-state.sqlite");
    let mut reading = Reading::default();
    if !path.is_file() {
        reading.warnings.push(format!(
            "Sin base de uso de MiniMax Code en {}.",
            path.display()
        ));
        return reading;
    }
    match query(&path, window) {
        Ok(row) => {
            reading.tokens = Some(TokenUsage::new(
                u64_of(row.input),
                u64_of(row.output),
                u64_of(row.cache_read),
                u64_of(row.cache_write),
            ));
            reading.messages = Some(u64_of(row.messages));
            reading.cost_usd = Some(row.cost);
            reading.observed_ms =
                (row.last_ts > 0).then_some(u64::try_from(row.last_ts).unwrap_or_default());
            reading.sources.tokens =
                Some(format!("{TABLE} de ~/.minimax (SQLite en solo lectura)"));
            reading.sources.messages = Some(format!("filas de {TABLE}"));
            reading.sources.cost = Some(format!("cost_usd de {TABLE}"));
        }
        Err(error) => {
            reading
                .warnings
                .push(format!("No pude leer el uso de MiniMax Code: {error}"));
        }
    }
    reading
}

struct Row {
    messages: i64,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    cost: f64,
    last_ts: i64,
}

fn u64_of(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn query(path: &Path, window: &Window) -> Result<Row, String> {
    let connection = open(path)?;
    let start = i64::try_from(window.start_ms).unwrap_or(i64::MAX);
    let sql = format!(
        "SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0), \
         COALESCE(SUM(cache_read_tokens), 0), COALESCE(SUM(cache_write_tokens), 0), \
         COALESCE(SUM(cost_usd), 0.0), COALESCE(MAX(ts), 0) \
         FROM {TABLE} WHERE ts >= ?1"
    );
    connection
        .query_row(&sql, params![start], |row| {
            Ok(Row {
                messages: row.get(0)?,
                input: row.get(1)?,
                output: row.get(2)?,
                cache_read: row.get(3)?,
                cache_write: row.get(4)?,
                cost: row.get(5)?,
                last_ts: row.get(6)?,
            })
        })
        .map_err(|error| error.to_string())
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

//! Lector de MiniMax Code (§8.5 de IR-0): su SQLite de uso, en solo lectura.
//!
//! La tabla `local_runtime_token_usage` guarda una fila por turno con `ts` en
//! milisegundos. No hay límite ni porcentaje local: eso solo se declara.

use std::cmp::Reverse;
use std::path::Path;

use rusqlite::params;

use super::sqlite;
use super::{ModelUsage, Reading, TokenUsage, UsageDirs, UsageState, Window};

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
            reading.models = row.models;
            reading.observed_ms = (row.last_ts > 0).then_some(u64_of(row.last_ts));
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
    models: Vec<ModelUsage>,
}

struct Item {
    model: String,
    messages: i64,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    cost: f64,
    last_ts: i64,
}

fn query(path: &Path, window: &Window) -> Result<Row, String> {
    let connection = sqlite::open_readonly(path)?;
    let start = i64::try_from(window.start_ms).unwrap_or(i64::MAX);
    let sql = format!(
        "SELECT COALESCE(model, 'desconocido'), COUNT(*), \
         COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0), \
         COALESCE(SUM(cache_read_tokens), 0), COALESCE(SUM(cache_write_tokens), 0), \
         COALESCE(SUM(cost_usd), 0.0), COALESCE(MAX(ts), 0) \
         FROM {TABLE} WHERE ts >= ?1 GROUP BY 1"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![start], |row| {
            Ok(Item {
                model: row.get(0)?,
                messages: row.get(1)?,
                input: row.get(2)?,
                output: row.get(3)?,
                cache_read: row.get(4)?,
                cache_write: row.get(5)?,
                cost: row.get(6)?,
                last_ts: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?;
    let items = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(summarize(items))
}

fn summarize(items: Vec<Item>) -> Row {
    let mut row = Row {
        messages: 0,
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        cost: 0.0,
        last_ts: 0,
        models: Vec::new(),
    };
    for item in items {
        row.messages += item.messages;
        row.input += item.input;
        row.output += item.output;
        row.cache_read += item.cache_read;
        row.cache_write += item.cache_write;
        row.cost += item.cost;
        row.last_ts = row.last_ts.max(item.last_ts);
        row.models.push(ModelUsage {
            model: item.model,
            tokens: Some(TokenUsage::new(
                u64_of(item.input),
                u64_of(item.output),
                u64_of(item.cache_read),
                u64_of(item.cache_write),
            )),
            messages: Some(u64_of(item.messages)),
            cost_usd: Some(item.cost),
            state: UsageState::Desconocido,
        });
    }
    row.models
        .sort_by_key(|model| Reverse(model.tokens.map_or(0, |tokens| tokens.total)));
    row
}

fn u64_of(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

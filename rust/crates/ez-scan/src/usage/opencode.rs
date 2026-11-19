//! Lector de OpenCode (§8.4 de IR-0): tokens y costo de su base, por modelo, y
//! los avisos de límite de su registro.
//!
//! La versión actual (v2.3.0) guarda los mensajes en `session_message`; las
//! versiones viejas usaban `message`. Se elige la tabla cuyo dato más nuevo sea
//! mayor y se abre en solo lectura por URI (`mode=ro`) para ver el WAL. Del
//! registro solo se leen las líneas de límite, nunca el contenido de sesiones.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rusqlite::Connection;

use super::jsonl;
use super::sqlite;
use super::{ModelUsage, Reading, TokenUsage, UsageDirs, UsageState, Window};

const MAX_LOG_TAIL: u64 = 2 * 1024 * 1024;
const STALE_DB_MS: u64 = 24 * 3_600_000;
const FREE_LIFE_MS: u64 = 24 * 3_600_000;
/// Modelos usados en los últimos días cuentan como «usables» para decidir si
/// el agente entero está agotado.
const RECENT_MODEL_MS: u64 = 7 * 24 * 3_600_000;

pub(crate) fn read(dirs: &UsageDirs, window: &Window, now_ms: u64) -> Reading {
    let mut reading = Reading::default();
    let log = dirs.opencode_data.join("log").join("opencode.log");
    let notices = notices(&log);
    let db = dirs.opencode_data.join("opencode.db");
    if !db.is_file() {
        reading
            .warnings
            .push(format!("Sin base de OpenCode en {}.", db.display()));
        apply_notices(&mut reading, None, &notices, window, now_ms);
        return reading;
    }
    let connection = match sqlite::open_readonly(&db) {
        Ok(connection) => connection,
        Err(error) => {
            reading
                .warnings
                .push(format!("No pude abrir la base de OpenCode: {error}"));
            apply_notices(&mut reading, None, &notices, window, now_ms);
            return reading;
        }
    };
    let Some((schema, latest)) = newest_schema(&connection) else {
        reading
            .warnings
            .push("La base de OpenCode no tiene tablas de mensajes conocidas.".to_string());
        apply_notices(&mut reading, None, &notices, window, now_ms);
        return reading;
    };
    // Los modelos usados en los últimos días: la regla de IR-2b y la de
    // `uso.json` comparten este conjunto de «usables».
    reading.usable_models = recent_models(&connection, now_ms);
    match query(&connection, schema, window) {
        Ok(groups) => fill(&mut reading, groups, schema, latest, now_ms),
        Err(error) => reading
            .warnings
            .push(format!("No pude leer el uso de OpenCode: {error}")),
    }
    apply_notices(
        &mut reading,
        Some((&connection, schema)),
        &notices,
        window,
        now_ms,
    );
    reading
}

/// La tabla con el dato más nuevo: `session_message` (v2) o `message` (vieja).
fn newest_schema(connection: &Connection) -> Option<(Schema, i64)> {
    let current = sqlite::max_time(connection, "session_message");
    let legacy = sqlite::max_time(connection, "message");
    match (current, legacy) {
        (Some(current), Some(legacy)) if current >= legacy => {
            Some((Schema::SessionMessage, current))
        }
        (Some(_), Some(legacy)) => Some((Schema::Message, legacy)),
        (Some(current), None) => Some((Schema::SessionMessage, current)),
        (None, Some(legacy)) => Some((Schema::Message, legacy)),
        (None, None) => None,
    }
}

fn fill(reading: &mut Reading, groups: Vec<Group>, schema: Schema, latest: i64, now_ms: u64) {
    let mut input = 0u64;
    let mut output = 0u64;
    let mut cache_read = 0u64;
    let mut cache_write = 0u64;
    let mut messages = 0u64;
    let mut cost = 0.0;
    for group in &groups {
        input = input.saturating_add(u64_of(group.input));
        output = output.saturating_add(u64_of(group.output));
        cache_read = cache_read.saturating_add(u64_of(group.cache_read));
        cache_write = cache_write.saturating_add(u64_of(group.cache_write));
        messages = messages.saturating_add(u64_of(group.messages));
        cost += group.cost;
    }
    reading.tokens = Some(TokenUsage::new(input, output, cache_read, cache_write));
    reading.messages = Some(messages);
    reading.cost_usd = Some(cost);
    reading.models = groups
        .iter()
        .map(|group| ModelUsage {
            model: group.model(),
            tokens: Some(TokenUsage::new(
                u64_of(group.input),
                u64_of(group.output),
                u64_of(group.cache_read),
                u64_of(group.cache_write),
            )),
            messages: Some(u64_of(group.messages)),
            cost_usd: Some(group.cost),
            state: UsageState::Desconocido,
        })
        .collect();
    reading
        .models
        .sort_by_key(|model| Reverse(model.tokens.map_or(0, |tokens| tokens.total)));
    reading.observed_ms = (latest > 0).then_some(u64_of(latest));
    reading.sources.tokens = Some(format!(
        "tokens de {} en opencode.db (SQLite en solo lectura)",
        schema.table()
    ));
    reading.sources.messages = Some(format!("filas de {} en la ventana", schema.table()));
    reading.sources.cost = Some(format!("cost de {} en opencode.db", schema.table()));
    if let Some(observed) = reading.observed_ms {
        if now_ms.saturating_sub(observed) > STALE_DB_MS {
            let age = now_ms.saturating_sub(observed) / 3_600_000;
            reading.warnings.push(format!(
                "La base de OpenCode no se actualiza desde hace {age} h; puede no reflejar el uso de hoy."
            ));
        }
    }
}

fn query(connection: &Connection, schema: Schema, window: &Window) -> Result<Vec<Group>, String> {
    let start = i64::try_from(window.start_ms).unwrap_or(i64::MAX);
    let (provider, model) = match schema {
        Schema::SessionMessage => ("$.model.providerID", "$.model.id"),
        Schema::Message => ("$.providerID", "$.modelID"),
    };
    let sql = format!(
        "SELECT COALESCE(json_extract(data, '{provider}'), ''), \
         COALESCE(json_extract(data, '{model}'), ''), \
         COUNT(*), \
         COALESCE(SUM(json_extract(data, '$.tokens.input')), 0), \
         COALESCE(SUM(json_extract(data, '$.tokens.output')), 0), \
         COALESCE(SUM(json_extract(data, '$.tokens.cache.read')), 0), \
         COALESCE(SUM(json_extract(data, '$.tokens.cache.write')), 0), \
         COALESCE(SUM(json_extract(data, '$.cost')), 0.0) \
         FROM {} WHERE {} AND time_created >= ?1 AND json_valid(data) \
         AND json_extract(data, '$.tokens') IS NOT NULL GROUP BY 1, 2",
        schema.table(),
        schema.assistant_filter()
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([start], |row| {
            Ok(Group {
                provider: row.get(0)?,
                id: row.get(1)?,
                messages: row.get(2)?,
                input: row.get(3)?,
                output: row.get(4)?,
                cache_read: row.get(5)?,
                cache_write: row.get(6)?,
                cost: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

/// Avisos vigentes por plan: Free agota `*-free`; Go agota `opencode-go/*`.
fn apply_notices(
    reading: &mut Reading,
    db: Option<(&Connection, Schema)>,
    notices: &[Notice],
    window: &Window,
    now_ms: u64,
) {
    if notices.is_empty() {
        return;
    }
    let active = active_notices(reading, db, notices, window, now_ms);
    let recent = reading.usable_models.clone();
    mark_models(reading, &active, &recent);
    reading.plan =
        plan_from_usage(reading).or_else(|| active.last().map(|notice| notice.label.clone()));
    if reading.plan.is_some() && reading.sources.plan.is_none() {
        reading.sources.plan =
            Some("modelo en uso en la ventana o aviso de límite de OpenCode".to_string());
    }
    if let Some(reset) = active.iter().filter_map(|notice| notice.reset_ms).max() {
        reading.resets_ms = Some(reset);
        reading.sources.reset = Some("cuenta atrás del mensaje de límite de OpenCode".to_string());
    }
}

fn active_notices(
    reading: &mut Reading,
    db: Option<(&Connection, Schema)>,
    notices: &[Notice],
    window: &Window,
    now_ms: u64,
) -> Vec<Notice> {
    let mut active = Vec::new();
    for notice in notices {
        let later = db
            .and_then(|(connection, schema)| latest_usage(connection, schema, notice.plan))
            .map(u64_of);
        if notice.is_active(now_ms, window.hours, later) {
            active.push(notice.clone());
            reading.warnings.push(format!(
                "Aviso vigente de {}: agota {}.",
                notice.label,
                notice.scope()
            ));
        } else {
            reading.warnings.push(format!(
                "El aviso de {} ya no cuenta: reinicio vencido o hubo uso posterior.",
                notice.label
            ));
        }
    }
    active
}

fn mark_models(reading: &mut Reading, active: &[Notice], recent: &[String]) {
    for model in &mut reading.models {
        if active.iter().any(|notice| notice.covers(&model.model)) {
            model.state = UsageState::Agotado;
        }
    }
    let exhausted: Vec<&ModelUsage> = reading
        .models
        .iter()
        .filter(|model| model.state == UsageState::Agotado)
        .collect();
    // DECISION: el agente solo es «agotado» si todos los modelos con uso en la
    // ventana y los usados en los últimos 7 días están cubiertos por un aviso.
    let mut usable: BTreeSet<String> = reading
        .models
        .iter()
        .map(|model| model.model.clone())
        .collect();
    usable.extend(recent.iter().cloned());
    let all_covered = !reading.models.is_empty()
        && !usable.is_empty()
        && usable
            .iter()
            .all(|model| active.iter().any(|notice| notice.covers(model)));
    if all_covered {
        reading.depleted = Some(
            "aviso de límite de OpenCode: todos los modelos usados recientemente están agotados"
                .to_string(),
        );
    } else if !exhausted.is_empty() {
        let names = exhausted
            .iter()
            .map(|model| model.model.clone())
            .collect::<Vec<_>>()
            .join(", ");
        reading.warnings.push(format!(
            "El modelo {names} está agotado; el agente no, porque otros modelos de la ventana siguen disponibles."
        ));
    }
}

fn plan_from_usage(reading: &Reading) -> Option<String> {
    let top = reading
        .models
        .iter()
        .max_by_key(|model| model.tokens.map_or(0, |tokens| tokens.total))?;
    if top.model.starts_with("opencode-go/") {
        Some("OpenCode Go".to_string())
    } else if is_free(&top.model) {
        Some("Free".to_string())
    } else {
        None
    }
}

fn is_free(model: &str) -> bool {
    model
        .rsplit('/')
        .next()
        .is_some_and(|id| id.to_ascii_lowercase().ends_with("-free"))
}

fn latest_usage(connection: &Connection, schema: Schema, plan: Plan) -> Option<i64> {
    let filter = match (schema, plan) {
        (Schema::SessionMessage, Plan::Free) => {
            "type = 'assistant' AND json_extract(data, '$.model.id') LIKE '%-free'"
        }
        (Schema::SessionMessage, Plan::Go) => {
            "type = 'assistant' AND json_extract(data, '$.model.providerID') = 'opencode-go'"
        }
        (Schema::Message, Plan::Free) => "json_extract(data, '$.modelID') LIKE '%-free'",
        (Schema::Message, Plan::Go) => "json_extract(data, '$.providerID') = 'opencode-go'",
    };
    let sql = format!(
        "SELECT MAX(time_created) FROM {} WHERE json_valid(data) AND {filter}",
        schema.table()
    );
    connection
        .query_row(&sql, [], |row| row.get::<_, Option<i64>>(0))
        .ok()
        .flatten()
}

fn u64_of(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

/// Modelos usados en los últimos días, para no agotar el agente entero cuando
/// solo lo está el plan que nombra el aviso.
fn recent_models(connection: &Connection, now_ms: u64) -> Vec<String> {
    if !sqlite::has_table(connection, "session_v2") {
        return Vec::new();
    }
    let start = i64::try_from(now_ms.saturating_sub(RECENT_MODEL_MS)).unwrap_or(0);
    let sql = "SELECT COALESCE(json_extract(model, '$.providerID'), ''), \
               COALESCE(json_extract(model, '$.id'), '') \
               FROM session_v2 WHERE time_updated >= ?1 AND json_valid(model)";
    let Ok(mut statement) = connection.prepare(sql) else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map([start], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) else {
        return Vec::new();
    };
    let mut seen = BTreeSet::new();
    for (provider, id) in rows.flatten() {
        let name = model_name(&provider, &id);
        if name != "desconocido" {
            seen.insert(name);
        }
    }
    seen.into_iter().collect()
}

fn model_name(provider: &str, id: &str) -> String {
    let provider = provider.trim();
    let id = id.trim();
    match (provider.is_empty(), id.is_empty()) {
        (false, false) => format!("{provider}/{id}"),
        (false, true) => provider.to_string(),
        (true, false) => id.to_string(),
        (true, true) => "desconocido".to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Schema {
    SessionMessage,
    Message,
}

impl Schema {
    fn table(self) -> &'static str {
        match self {
            Schema::SessionMessage => "session_message",
            Schema::Message => "message",
        }
    }

    fn assistant_filter(self) -> &'static str {
        match self {
            Schema::SessionMessage => "type = 'assistant'",
            Schema::Message => "1 = 1",
        }
    }
}

#[derive(Debug, Clone)]
struct Group {
    provider: String,
    id: String,
    messages: i64,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    cost: f64,
}

impl Group {
    fn model(&self) -> String {
        model_name(&self.provider, &self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Plan {
    Free,
    Go,
}

/// Un aviso de límite del registro (se queda el último de cada plan).
#[derive(Debug, Clone, PartialEq)]
struct Notice {
    plan: Plan,
    label: String,
    reset_ms: Option<u64>,
    observed_ms: Option<u64>,
}

impl Notice {
    fn is_active(&self, now_ms: u64, window_hours: f64, later_usage: Option<u64>) -> bool {
        let Some(observed) = self.observed_ms else {
            return false;
        };
        if later_usage.is_some_and(|usage| usage > observed) {
            return false;
        }
        match self.reset_ms {
            Some(reset) => reset > now_ms,
            None => {
                let life = match self.plan {
                    Plan::Free => FREE_LIFE_MS,
                    Plan::Go => (window_hours * 3_600_000.0) as u64,
                };
                observed.saturating_add(life) > now_ms
            }
        }
    }

    fn covers(&self, model: &str) -> bool {
        match self.plan {
            Plan::Free => is_free(model),
            Plan::Go => model.starts_with("opencode-go/"),
        }
    }

    fn scope(&self) -> &'static str {
        match self.plan {
            Plan::Free => "los modelos gratuitos (*-free)",
            Plan::Go => "los modelos de opencode-go/*",
        }
    }
}

fn notices(path: &Path) -> Vec<Notice> {
    let Some(text) = jsonl::read_tail(path, MAX_LOG_TAIL) else {
        return Vec::new();
    };
    let mut by_plan: BTreeMap<Plan, Notice> = BTreeMap::new();
    for line in text.lines() {
        if let Some(notice) = parse_notice(line) {
            by_plan.insert(notice.plan, notice);
        }
    }
    by_plan.into_values().collect()
}

fn parse_notice(line: &str) -> Option<Notice> {
    if !line.to_ascii_lowercase().contains("usage limit reached") {
        return None;
    }
    let observed_ms = line
        .split_whitespace()
        .find_map(|word| word.strip_prefix("timestamp="))
        .and_then(jsonl::parse_time_ms);
    if line.contains("Free usage limit reached") {
        return Some(Notice {
            plan: Plan::Free,
            label: "Free".to_string(),
            reset_ms: None,
            observed_ms,
        });
    }
    let (plan, label) = if line.contains("OpenCode Go Plus usage limit reached") {
        (Plan::Go, "OpenCode Go Plus")
    } else if line.contains("OpenCode Go usage limit reached") {
        (Plan::Go, "OpenCode Go")
    } else {
        return None;
    };
    let after = line.split_once("usage limit reached (")?.1;
    let (_, rest) = after.split_once(',')?;
    let countdown = rest.split_once("resets in ")?.1;
    let countdown: String = countdown
        .chars()
        .take_while(|character| character.is_ascii_digit() || "dhms ".contains(*character))
        .collect();
    let reset_ms = match (observed_ms, countdown_ms(&countdown)) {
        (Some(observed), Some(span)) => Some(observed.saturating_add(span)),
        _ => None,
    };
    Some(Notice {
        plan,
        label: label.to_string(),
        reset_ms,
        observed_ms,
    })
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

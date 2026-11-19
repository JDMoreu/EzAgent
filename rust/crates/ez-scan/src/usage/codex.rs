//! Lector de Codex (§8.1 de IR-0): el último `rate_limits` de sus sesiones.
//!
//! Es el único agente que guarda el porcentaje real de la ventana y su reinicio
//! (`used_percent`, `window_minutes`, `resets_at`, `plan_type`).

use std::path::Path;

use serde_json::Value;

use super::jsonl;
use super::{Reading, UsageDirs, UsageLimit, Window};

pub(crate) fn read(dirs: &UsageDirs, window: &Window, now_ms: u64) -> Reading {
    let root = dirs.codex.join("sessions");
    let files = jsonl::files_newest_first(&root);
    if files.is_empty() {
        let mut reading = Reading::default();
        reading
            .warnings
            .push(format!("Sin sesiones de Codex en {}.", root.display()));
        return reading;
    }
    let mut found = None;
    let mut sessions = 0usize;
    for path in &files {
        sessions += 1;
        if let Some(row) = last_rate_limits(path) {
            found = Some(row);
            break;
        }
    }
    let Some(row) = found else {
        let mut reading = Reading::default();
        reading.warnings.push(
            "Ninguna sesión de Codex tiene rate_limits; el uso queda desconocido.".to_string(),
        );
        return reading;
    };
    build(row, window, now_ms, sessions)
}

fn build(row: RateRow, window: &Window, now_ms: u64, sessions: usize) -> Reading {
    let mut reading = Reading {
        observed_ms: row.observed_ms,
        plan: row.plan.clone(),
        ..Reading::default()
    };
    reading.sources.plan = Some("plan_type de rate_limits (Codex)".to_string());
    let Some(block) = pick_block(&row, window) else {
        reading
            .warnings
            .push("El último rate_limits de Codex no trae ventanas utilizables.".to_string());
        return reading;
    };
    apply_block(&mut reading, block, row.reached);
    apply_freshness(
        &mut reading,
        now_ms,
        window,
        sessions,
        row.unlimited_credits,
    );
    reading.sources.tokens = Some("rate_limits no da tokens; da porcentaje".to_string());
    reading
}

fn apply_block(reading: &mut Reading, block: &Block, reached: bool) {
    reading.observed_window_hours = block
        .window_minutes
        .filter(|minutes| *minutes > 0)
        .map(|minutes| minutes as f64 / 60.0);
    match block.used_percent {
        Some(percent) if (0.0..=100.0).contains(&percent) => {
            reading.used_percent = Some(percent);
            reading.limit = Some(UsageLimit::percent(100.0));
            reading.sources.limit = Some("observado: rate_limits.used_percent (Codex)".to_string());
            reading.sources.state = format!("{percent:.1} % observado de la ventana");
        }
        Some(percent) => reading.warnings.push(format!(
            "Codex informó un porcentaje fuera de 0–100 ({percent}); lo ignoro."
        )),
        None => {}
    }
    reading.resets_ms = block.resets_at.and_then(seconds_to_ms);
    if reading.resets_ms.is_some() {
        reading.sources.reset = Some("resets_at de rate_limits (epoch en segundos)".to_string());
    }
    if reached {
        reading.depleted = Some("rate_limit_reached_type de Codex".to_string());
    }
}

fn apply_freshness(
    reading: &mut Reading,
    now_ms: u64,
    window: &Window,
    sessions: usize,
    unlimited_credits: bool,
) {
    let window_ms = reading
        .observed_window_hours
        .map(|hours| (hours * 3_600_000.0) as u64)
        .unwrap_or((window.hours * 3_600_000.0) as u64);
    let reset_passed = reading.resets_ms.is_some_and(|reset| reset <= now_ms);
    let age_ms = reading
        .observed_ms
        .map(|observed| now_ms.saturating_sub(observed))
        .unwrap_or(0);
    // DECISION: con el reinicio ya pasado el porcentaje no es actual: el
    // estado queda `desconocido` y se avisa; no se adivina la ventana nueva.
    let age = age_text(age_ms);
    if reset_passed {
        reading.stale = Some(format!(
            "El dato de Codex es de {age} y su ventana ya reinició; no lo presento como actual."
        ));
    } else if age_ms > window_ms {
        reading.stale = Some(format!(
            "El dato de Codex es de {age}, más viejo que su ventana; no lo presento como actual."
        ));
    }
    if let Some(stale) = reading.stale.clone() {
        reading.warnings.push(stale);
    }
    if unlimited_credits {
        reading
            .warnings
            .push("Codex informa créditos ilimitados en esta lectura.".to_string());
    }
    reading.warnings.push(format!(
        "Último dato de cuota: {age}, en {sessions} archivo(s) revisado(s)."
    ));
}

fn pick_block<'a>(row: &'a RateRow, window: &Window) -> Option<&'a Block> {
    let wanted = window.minutes();
    let blocks = [row.primary.as_ref(), row.secondary.as_ref()];
    blocks
        .iter()
        .flatten()
        .find(|block| block.window_minutes == Some(wanted))
        .copied()
        .or(row.primary.as_ref())
        .or(row.secondary.as_ref())
}

fn seconds_to_ms(seconds: i64) -> Option<u64> {
    if seconds <= 0 {
        return None;
    }
    u64::try_from(seconds).ok().map(|value| value * 1_000)
}

fn age_text(age_ms: u64) -> String {
    let hours = age_ms / 3_600_000;
    if hours >= 48 {
        format!("hace {} días", hours / 24)
    } else if hours >= 1 {
        format!("hace {hours} h")
    } else {
        format!("hace {} min", age_ms / 60_000)
    }
}

#[derive(Debug, Clone, Default)]
struct Block {
    used_percent: Option<f64>,
    window_minutes: Option<i64>,
    resets_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
struct RateRow {
    primary: Option<Block>,
    secondary: Option<Block>,
    plan: Option<String>,
    reached: bool,
    unlimited_credits: bool,
    observed_ms: Option<u64>,
}

/// El último registro con `payload.rate_limits` del archivo.
fn last_rate_limits(path: &Path) -> Option<RateRow> {
    let mut found = None;
    jsonl::read_lines(path, |line| {
        if !line.contains("rate_limits") {
            return;
        }
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let Some(payload) = record.get("payload") else {
            return;
        };
        let Some(limits) = payload.get("rate_limits").filter(|value| value.is_object()) else {
            return;
        };
        let observed_ms = record
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(jsonl::parse_time_ms);
        let mut row = parse_row(limits);
        row.observed_ms = observed_ms;
        found = Some(row);
    });
    found
}

fn parse_row(limits: &Value) -> RateRow {
    RateRow {
        primary: limits.get("primary").map(parse_block),
        secondary: limits.get("secondary").map(parse_block),
        plan: limits
            .get("plan_type")
            .and_then(Value::as_str)
            .map(str::to_string),
        reached: limits
            .get("rate_limit_reached_type")
            .is_some_and(|value| !value.is_null()),
        unlimited_credits: limits
            .get("credits")
            .and_then(|credits| credits.get("unlimited"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        observed_ms: None,
    }
}

fn parse_block(value: &Value) -> Block {
    Block {
        used_percent: value.get("used_percent").and_then(Value::as_f64),
        window_minutes: value.get("window_minutes").and_then(Value::as_i64),
        resets_at: value.get("resets_at").and_then(Value::as_i64),
    }
}

#[cfg(test)]
mod tests;

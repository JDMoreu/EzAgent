//! Fechas y horas con el mismo texto que Python (F2-01).

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike, Utc};

use crate::pyjson::float_repr;

/// `time.time()`: segundos desde la época con la misma precisión de `f64`.
pub fn timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

/// `datetime.fromtimestamp(secs).isoformat()` en hora local, con microsegundos
/// solo si son distintos de cero (igual que Python).
pub fn isoformat_local(secs: f64) -> String {
    let Some(utc) = seconds_to_utc(secs) else {
        return String::new();
    };
    let local = utc.with_timezone(&Local);
    format_like_python(&local, false)
}

/// `datetime.now(timezone.utc).isoformat()`: termina en `+00:00`.
pub fn isoformat_utc(secs: f64) -> String {
    match seconds_to_utc(secs) {
        Some(utc) => format_like_python(&utc, true),
        None => String::new(),
    }
}

/// `datetime.fromtimestamp(secs).date().isoformat()`.
pub fn date_local(secs: f64) -> String {
    match seconds_to_utc(secs) {
        Some(utc) => {
            let local = utc.with_timezone(&Local);
            format!("{:04}-{:02}-{:02}", local.year(), local.month(), local.day())
        }
        None => String::new(),
    }
}

fn seconds_to_utc(secs: f64) -> Option<DateTime<Utc>> {
    if !secs.is_finite() {
        return None;
    }
    let whole = secs.trunc();
    // Python guarda microsegundos; redondear a ns daba 0 por la precisión de f64.
    let micros = ((secs - whole) * 1_000_000.0).round() as u32;
    Utc.timestamp_opt(whole as i64, micros * 1_000).single()
}

fn format_like_python(moment: &DateTime<impl TimeZone>, with_offset: bool) -> String {
    let micros = moment.timestamp_subsec_micros();
    let fraction = if micros == 0 { String::new() } else { format!(".{micros:06}") };
    let offset = if with_offset { "+00:00" } else { "" };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{fraction}{offset}",
        moment.year(),
        moment.month(),
        moment.day(),
        moment.hour(),
        moment.minute(),
        moment.second()
    )
}

/// Los flotantes de `latency.jsonl` y `audit.jsonl` se escriben con `repr`.
pub fn json_seconds(secs: f64) -> String {
    float_repr(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_isoformat_matches_python() {
        assert_eq!(isoformat_utc(1735689600.0), "2025-01-01T00:00:00+00:00");
        assert_eq!(isoformat_utc(1735689600.5), "2025-01-01T00:00:00.500000+00:00");
        assert_eq!(isoformat_utc(1735689600.000001), "2025-01-01T00:00:00.000001+00:00");
    }

    #[test]
    fn floats_use_python_repr() {
        assert_eq!(json_seconds(1735689600.5), "1735689600.5");
        assert_eq!(json_seconds(1e-05), "1e-05");
        assert_eq!(json_seconds(1297.0), "1297.0");
    }

    #[test]
    fn timestamp_is_now() {
        let now = timestamp();
        assert!(now > 1_700_000_000.0);
    }
}

//! Fechas y horas con el mismo texto que Python (F2-01).

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike, Utc};

use crate::pyjson::float_repr;

/// `OSError` de Windows cuando el instante cae fuera del rango que acepta.
const INVALID_ARGUMENT: &str = "OSError: [Errno 22] Invalid argument";
/// `OverflowError`: el valor no cabe en un `time_t`.
const OUT_OF_RANGE: &str = "OverflowError: timestamp out of range for platform time_t";
/// `ValueError`: `datetime.fromtimestamp(nan)`.
const NOT_A_NUMBER: &str = "ValueError: Invalid value NaN (not a number)";

// DECISION: el `gmtime` de Windows acepta instantes negativos solo hasta -12 h
// (comprobado contra Python 3.12: -43200 funciona, -43201 da `OSError`). Se
// reproduce el límite para que el port falle donde falla el Python.
const WINDOWS_GMTIME_FLOOR: f64 = -43_200.0;

/// `time.time()`: segundos desde la época con la misma precisión de `f64`.
pub fn timestamp() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

/// `datetime.fromtimestamp(secs).isoformat()` en hora local, con microsegundos
/// solo si son distintos de cero (igual que Python).
pub fn isoformat_local(secs: f64) -> Result<String, String> {
    let local = local_datetime(secs)?;
    Ok(format_micros(&local, false))
}

/// `datetime.fromtimestamp(secs, timezone.utc).isoformat()`: `+00:00` siempre.
pub fn isoformat_utc(secs: f64) -> Result<String, String> {
    let utc = utc_datetime(secs)?;
    Ok(format_micros(&utc, true))
}

/// `datetime.fromtimestamp(secs, timezone.utc).isoformat(timespec="milliseconds")`:
/// exactamente tres decimales, truncando los microsegundos.
pub fn isoformat_millis_utc(secs: f64) -> Result<String, String> {
    let utc = utc_datetime(secs)?;
    let millis = utc.timestamp_subsec_micros() / 1_000;
    Ok(format_moment(&utc, &format!(".{millis:03}"), "+00:00"))
}

/// `datetime.fromtimestamp(secs).date().isoformat()`.
pub fn date_local(secs: f64) -> Result<String, String> {
    let local = local_datetime(secs)?;
    Ok(format!(
        "{:04}-{:02}-{:02}",
        local.year(),
        local.month(),
        local.day()
    ))
}

/// `fromtimestamp(secs)` sin zona: Python usa `localtime`, que en Windows
/// rechaza cualquier instante negativo.
fn local_datetime(secs: f64) -> Result<DateTime<Local>, String> {
    let utc = utc_instant(secs)?;
    if secs < 0.0 {
        return Err(INVALID_ARGUMENT.to_string());
    }
    Ok(utc.with_timezone(&Local))
}

/// `fromtimestamp(secs, timezone.utc)`: Python usa `gmtime`.
fn utc_datetime(secs: f64) -> Result<DateTime<Utc>, String> {
    if secs < WINDOWS_GMTIME_FLOOR {
        return Err(INVALID_ARGUMENT.to_string());
    }
    utc_instant(secs)
}

fn utc_instant(secs: f64) -> Result<DateTime<Utc>, String> {
    if secs.is_nan() {
        return Err(NOT_A_NUMBER.to_string());
    }
    if !secs.is_finite() {
        return Err(OUT_OF_RANGE.to_string());
    }
    let (whole, micros) = split_seconds(secs);
    if whole < i64::MIN as f64 || whole >= i64::MAX as f64 {
        return Err(OUT_OF_RANGE.to_string());
    }
    Utc.timestamp_opt(whole as i64, micros * 1_000)
        .single()
        .ok_or_else(|| INVALID_ARGUMENT.to_string())
}

/// Parte entera y microsegundos como `datetime._fromtimestamp`: redondeo a par
/// y acarreo al segundo siguiente.
fn split_seconds(secs: f64) -> (f64, u32) {
    let mut whole = secs.trunc();
    let mut micros = ((secs - whole) * 1_000_000.0).round_ties_even();
    if micros >= 1_000_000.0 {
        whole += 1.0;
        micros -= 1_000_000.0;
    } else if micros < 0.0 {
        whole -= 1.0;
        micros += 1_000_000.0;
    }
    (whole, micros as u32)
}

fn format_micros(moment: &DateTime<impl TimeZone>, with_offset: bool) -> String {
    let micros = moment.timestamp_subsec_micros();
    let fraction = if micros == 0 {
        String::new()
    } else {
        format!(".{micros:06}")
    };
    let offset = if with_offset { "+00:00" } else { "" };
    format_moment(moment, &fraction, offset)
}

fn format_moment(moment: &DateTime<impl TimeZone>, fraction: &str, offset: &str) -> String {
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
        assert_eq!(
            isoformat_utc(1735689600.0).ok().as_deref(),
            Some("2025-01-01T00:00:00+00:00")
        );
        assert_eq!(
            isoformat_utc(1735689600.5).ok().as_deref(),
            Some("2025-01-01T00:00:00.500000+00:00")
        );
        assert_eq!(
            isoformat_utc(1735689600.000001).ok().as_deref(),
            Some("2025-01-01T00:00:00.000001+00:00")
        );
    }

    #[test]
    fn micros_carry_to_the_next_second() {
        assert_eq!(
            isoformat_utc(0.9999995).ok().as_deref(),
            Some("1970-01-01T00:00:01+00:00")
        );
        assert_eq!(
            isoformat_utc(0.9999999).ok().as_deref(),
            Some("1970-01-01T00:00:01+00:00")
        );
    }

    #[test]
    fn millis_truncate_and_keep_three_decimals() {
        assert_eq!(
            isoformat_millis_utc(1735689600.123456).ok().as_deref(),
            Some("2025-01-01T00:00:00.123+00:00")
        );
        assert_eq!(
            isoformat_millis_utc(0.0).ok().as_deref(),
            Some("1970-01-01T00:00:00.000+00:00")
        );
    }

    #[test]
    fn negative_instants_fail_like_windows() {
        assert_eq!(
            isoformat_local(-1.0),
            Err("OSError: [Errno 22] Invalid argument".to_string())
        );
        assert_eq!(
            date_local(-1735689600.0),
            Err("OSError: [Errno 22] Invalid argument".to_string())
        );
        assert_eq!(
            isoformat_utc(-1735689600.0),
            Err("OSError: [Errno 22] Invalid argument".to_string())
        );
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

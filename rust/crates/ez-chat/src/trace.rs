//! Port of `src/ezagent/voice/chat/trace.py` (F3-14).
//!
//! Fecha y hora local de cada mensaje del chat.

use chrono::{DateTime, Datelike, FixedOffset, Local, TimeZone, Timelike};

use crate::session::ChatMessage;

const MONTHS: [&str; 12] = [
    "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
];

fn format_stamp<Tz: TimeZone>(moment: &DateTime<Tz>) -> String {
    format!(
        "{} {} {} · {:02}:{:02}:{:02}",
        moment.day(),
        MONTHS[moment.month0() as usize],
        moment.year(),
        moment.hour(),
        moment.minute(),
        moment.second()
    )
}

/// «13 nov 2026 · 16:46:12» en la hora local.
pub fn stamp(t: f64) -> String {
    let Some(moment) = Local.timestamp_opt(t.trunc() as i64, 0).single() else {
        // DECISION: Python lanza OSError con instantes fuera de rango; aquí se
        // avisa y se devuelve "" para no tumbar la vista.
        tracing::warn!("No pude formatear la hora del chat: {t}");
        return String::new();
    };
    format_stamp(&moment)
}

/// El mismo texto en una zona fija. El oráculo de paridad de `chat_trace` usó
/// una zona fija; esta puerta permite comprobarlo sin depender de la máquina.
// DECISION: `stamp` sigue con `Local`; esta variante no la usa la aplicación.
pub fn stamp_with_offset(t: f64, offset_seconds: i32) -> String {
    let Some(zone) = FixedOffset::east_opt(offset_seconds) else {
        tracing::warn!("Desfase horario inválido: {offset_seconds}");
        return String::new();
    };
    let Some(moment) = zone.timestamp_opt(t.trunc() as i64, 0).single() else {
        tracing::warn!("No pude formatear la hora del chat: {t}");
        return String::new();
    };
    format_stamp(&moment)
}

/// Fecha y hora de un mensaje del usuario o de EzAgent; "" mientras se escribe.
pub fn stamp_for(message: &ChatMessage) -> String {
    stamp_for_with(message, stamp)
}

/// Igual que `stamp_for`, con una zona fija (la del oráculo de paridad).
pub fn stamp_for_with_offset(message: &ChatMessage, offset_seconds: i32) -> String {
    stamp_for_with(message, |t| stamp_with_offset(t, offset_seconds))
}

fn stamp_for_with(message: &ChatMessage, stamp: impl Fn(f64) -> String) -> String {
    if message.role.as_str() != "user" && message.role.as_str() != "assistant" {
        return String::new();
    }
    if message.streaming {
        return String::new();
    }
    stamp(message.t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Role, Source};

    fn message(role: Role, t: f64, streaming: bool) -> ChatMessage {
        ChatMessage {
            id: 1,
            role,
            text: String::new(),
            source: Source::Chat,
            t,
            streaming,
            error: false,
        }
    }

    #[test]
    fn stamp_has_date_and_time_in_local_time() {
        let moment = Local
            .with_ymd_and_hms(2026, 11, 13, 16, 46, 12)
            .single()
            .expect("fecha válida");
        assert_eq!(stamp(moment.timestamp() as f64), "13 nov 2026 · 16:46:12");
    }

    #[test]
    fn only_finished_user_and_assistant_messages_get_a_stamp() {
        let moment = Local
            .with_ymd_and_hms(2026, 11, 13, 16, 46, 17)
            .single()
            .expect("fecha válida");
        let t = moment.timestamp() as f64;
        assert_eq!(
            stamp_for(&message(Role::User, t, false)),
            "13 nov 2026 · 16:46:17"
        );
        assert_eq!(
            stamp_for(&message(Role::Assistant, t, false)),
            "13 nov 2026 · 16:46:17"
        );
        assert_eq!(stamp_for(&message(Role::Assistant, t, true)), "");
        assert_eq!(stamp_for(&message(Role::System, t, false)), "");
    }

    #[test]
    fn the_clock_truncates_fractional_seconds() {
        let moment = Local
            .with_ymd_and_hms(2026, 1, 15, 12, 0, 1)
            .single()
            .expect("fecha válida");
        assert_eq!(
            stamp(moment.timestamp() as f64 + 0.9),
            "15 ene 2026 · 12:00:01"
        );
    }
}

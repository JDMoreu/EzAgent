//! Textos que ve la persona o el agente. Español neutro latinoamericano.

use serde_json::Value;

/// Traduce un evento de la traza a su texto visible (brief ECO-6b §4).
/// Los campos que el evento no trae se dejan vacíos o a cero; la vista aplica
/// la misma tabla en `ui/js/texts.js` a los eventos que llegan del árbitro.
// DECISION: el brief pide la tabla en texts.js (un solo lugar de la vista) y
// también pruebas Rust de la traducción; se mantiene aquí el espejo que rellena
// `texto` en los eventos de ejemplo y se prueba cada tipo.
pub fn event_text(kind: &str, data: &Value) -> String {
    let text = |key: &str| {
        data.get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let number = |key: &str| data.get(key).and_then(Value::as_u64).unwrap_or(0);
    match kind {
        "task_proposed" => format!("Tarea propuesta: {}", text("title")),
        "task_approved" => "Tarea aprobada".to_string(),
        "team_proposed" => "Equipo propuesto".to_string(),
        "team_approved" => "Equipo aprobado".to_string(),
        "round_started" => format!("Ronda {}", number("round")),
        "fish_launched" => format!(
            "{} {} en marcha ({} · {})",
            text("role"),
            text("fish"),
            text("agente"),
            text("modelo")
        ),
        "fish_reported" => format!(
            "{}: {} ({} bloqueantes, {} mayores)",
            text("fish"),
            if text("verdict") == "pass" {
                "pasa"
            } else {
                "no pasa"
            },
            number("blocking"),
            number("major")
        ),
        "fish_blocked" => format!("{} espera una respuesta ({})", text("fish"), text("motivo")),
        "fish_failed" => format!("{} falló: {}", text("fish"), text("reason")),
        "verification" => format!(
            "Verificación: {}/{} criterios",
            number("aprobados"),
            number("total")
        ),
        "round_closed" => {
            if data.get("clean").and_then(Value::as_bool).unwrap_or(false) {
                format!("Ronda {} limpia", number("round"))
            } else {
                format!("Ronda {} no limpia: {}", number("round"), text("motivos"))
            }
        }
        "paused" => format!("En pausa: {}", text("motivo")),
        "resumed" => "Reanudada".to_string(),
        "task_closed" => "Tarea terminada".to_string(),
        "task_stopped" => "Tarea detenida".to_string(),
        "integrated" => "Rama integrada".to_string(),
        "rejected" => format!("Rechazado ({}): {}", text("rule"), text("message")),
        other => other.to_string(),
    }
}

/// Nombre visible de cada agente que puede ser agente principal.
pub fn agent_name(agent: &str) -> &str {
    match agent {
        "claude" => "Claude Code",
        "opencode" => "OpenCode",
        "codex" => "Codex",
        other => other,
    }
}

/// Nombre visible de cada papel de subagente.
pub fn role_name(role: &str) -> &str {
    match role {
        "planner" => "Planificador",
        "plan_reviewer" => "Revisor del plan",
        "developer" => "Desarrollador",
        "qa" => "QA",
        "qa_validator" => "Validador de QA",
        other => other,
    }
}

pub const SPACE_NAME_EMPTY: &str = "Escribe un nombre para el espacio.";
pub const ELEPHANT_CLOSED: &str = "La terminal del agente principal no está abierta.";
pub const VIEW_CLOSED: &str = "La terminal se cerró.";
pub const CARD_ACTION_INVALID: &str = "La acción de la tarjeta debe ser approve o reject.";
pub const CARD_COMMENT_REQUIRED: &str =
    "Para rechazar, escribe un comentario para el agente principal.";
pub const SPACES_FILE: &str = "No pude guardar los espacios.";

// Nota de voz (ECO-7): textos de la línea de estado de la barra.
pub const VOICE_NO_MICROPHONE: &str = "No encontré un micrófono.";
pub const VOICE_EMPTY: &str = "No escuché nada.";
pub const VOICE_BUSY: &str = "La nota de voz ya está en curso.";
pub const VOICE_NOT_RECORDING: &str = "No hay una grabación en curso.";
pub const VOICE_STT_FAILED: &str = "No pude iniciar la transcripción.";
pub const VOICE_ALREADY_STARTED: &str = "La grabación ya estaba empezada.";

/// «No pude transcribir: {motivo}»
pub fn voice_transcribe_failed(reason: &str) -> String {
    format!("No pude transcribir: {reason}")
}

/// «No encontré «{programa}» en PATH.»
pub fn program_not_found(program: &str) -> String {
    format!("No encontré «{program}» en PATH.")
}

/// ««{folder}» no existe.»
pub fn folder_missing(folder: &str) -> String {
    format!("«{folder}» no existe.")
}

/// ««{folder}» no es una carpeta.»
pub fn folder_not_dir(folder: &str) -> String {
    format!("«{folder}» no es una carpeta.")
}

/// «No conozco el agente «{agent}»; usa claude, opencode o codex.»
pub fn unknown_agent(agent: &str) -> String {
    format!("No conozco el agente «{agent}»; usa claude, opencode o codex.")
}

/// «No hay una terminal con id «{id}».»
pub fn unknown_view(id: &str) -> String {
    format!("No hay una terminal con id «{id}».")
}

/// «No hay una tarjeta con id «{card}».»
pub fn unknown_card(card: &str) -> String {
    format!("No hay una tarjeta con id «{card}».")
}

/// «No pude abrir la terminal: {detail}»
pub fn pty_failed(detail: &str) -> String {
    format!("No pude abrir la terminal: {detail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn approval_and_round_events_have_their_text() {
        assert_eq!(
            event_text("task_proposed", &json!({"title": "Transcriptor"})),
            "Tarea propuesta: Transcriptor"
        );
        assert_eq!(event_text("task_approved", &json!({})), "Tarea aprobada");
        assert_eq!(event_text("team_proposed", &json!({})), "Equipo propuesto");
        assert_eq!(event_text("team_approved", &json!({})), "Equipo aprobado");
        assert_eq!(event_text("round_started", &json!({"round": 3})), "Ronda 3");
    }

    #[test]
    fn fish_events_have_their_text() {
        let launched = json!({"role": "QA", "fish": "qa-r1", "agente": "Codex", "modelo": "gpt-6"});
        assert_eq!(
            event_text("fish_launched", &launched),
            "QA qa-r1 en marcha (Codex · gpt-6)"
        );
        let failed = json!({"fish": "dev-r1-1", "verdict": "fail", "blocking": 2, "major": 1});
        assert_eq!(
            event_text("fish_reported", &failed),
            "dev-r1-1: no pasa (2 bloqueantes, 1 mayores)"
        );
        assert_eq!(
            event_text(
                "fish_blocked",
                &json!({"fish": "qa-r1", "motivo": "respuesta"})
            ),
            "qa-r1 espera una respuesta (respuesta)"
        );
        assert_eq!(
            event_text("fish_failed", &json!({"fish": "qa-r1", "reason": "salió"})),
            "qa-r1 falló: salió"
        );
    }

    #[test]
    fn verification_round_and_lifecycle_events_have_their_text() {
        assert_eq!(
            event_text("verification", &json!({"aprobados": 2, "total": 3})),
            "Verificación: 2/3 criterios"
        );
        assert_eq!(
            event_text("round_closed", &json!({"round": 1, "clean": true})),
            "Ronda 1 limpia"
        );
        assert_eq!(
            event_text(
                "round_closed",
                &json!({"round": 2, "clean": false, "motivos": "falló C1"})
            ),
            "Ronda 2 no limpia: falló C1"
        );
        assert_eq!(
            event_text("paused", &json!({"motivo": "tope_rondas"})),
            "En pausa: tope_rondas"
        );
        assert_eq!(event_text("resumed", &json!({})), "Reanudada");
        assert_eq!(event_text("task_closed", &json!({})), "Tarea terminada");
        assert_eq!(event_text("task_stopped", &json!({})), "Tarea detenida");
        assert_eq!(event_text("integrated", &json!({})), "Rama integrada");
        assert_eq!(
            event_text("rejected", &json!({"rule": "R7", "message": "sin trabajo"})),
            "Rechazado (R7): sin trabajo"
        );
    }

    #[test]
    fn an_unknown_event_shows_its_type() {
        assert_eq!(event_text("algo_nuevo", &json!({})), "algo_nuevo");
    }

    #[test]
    fn a_pass_report_says_pasa() {
        assert_eq!(
            event_text("fish_reported", &json!({"fish": "qa", "verdict": "pass"})),
            "qa: pasa (0 bloqueantes, 0 mayores)"
        );
    }
}

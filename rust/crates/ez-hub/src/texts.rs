//! Textos que ve la persona o el agente. Español neutro latinoamericano.

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

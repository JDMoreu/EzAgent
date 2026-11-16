//! Textos del modo autónomo (9.6). Los genera el código, nunca el modelo.
//!
//! Español neutro latinoamericano, copiado literal del diseño aprobado.

use super::state::TaskState;

/// Confirmación única de activación (9.6), generada por el código.
pub fn confirm(task: &str, agent: &str, space: &str, rounds: i64, minutes: i64) -> String {
    format!(
        "Modo autónomo: haré «{task}» con {agent} en el espacio «{space}». Aprobaré yo los \
         permisos que pidan los agentes de esta tarea hasta que se cumpla lo que pediste, como \
         máximo {rounds} rondas o {minutes} minutos. ¿Lo activo?"
    )
}

/// Inicio (9.6).
pub const START: &str = "Empiezo en modo autónomo.";

/// Ronda (9.6).
pub fn round(n: i64) -> String {
    format!("Ronda {n}: verificando.")
}

/// Pausa (9.6).
pub fn paused(reason: &str) -> String {
    format!("Pausé el modo autónomo: {reason}. Di «sigue» o «para».")
}

/// Fin (9.6).
pub fn done(task: &str, approvals: i64) -> String {
    format!("Terminé «{task}»: se cumple lo que pediste. Aprobé {approvals} permisos.")
}

/// Parada (9.6).
pub const STOPPED: &str = "Detuve el modo autónomo.";

/// `autonomous_status` sin tarea (9.2).
pub const NO_TASK: &str = "No hay tarea autónoma.";

/// Guarda 1: el turno leyó datos no confiables.
pub const TAINTED: &str =
    "No puedo activar el modo autónomo: en este turno se leyó web, archivos o \
                           salida de agentes. Pídemelo otra vez en un mensaje nuevo.";

/// Guarda 2: el usuario no lo autorizó en su último mensaje.
pub const NOT_AUTHORIZED: &str = "No puedo activar el modo autónomo: necesito que me lo pidas \
                                  explícitamente («modo autónomo», «autorizo» o «yolo»).";

/// Guarda 3: solo una tarea autónoma a la vez.
pub const ALREADY_ACTIVE: &str =
    "Ya hay una tarea autónoma en curso; deténla antes de empezar otra.";

/// Guarda 4: el usuario dijo que no (o no respondió).
pub const DECLINED: &str = "No activé el modo autónomo.";

/// Motivos de pausa (9.5).
pub const REASON_DEADLINE: &str = "se cumplió el tiempo máximo";
pub const REASON_ROUNDS: &str = "se agotaron las rondas";
pub const REASON_CHURN: &str = "el mismo problema vuelve";
pub const REASON_VERDICT: &str = "el verificador no devolvió un veredicto válido dos veces";
pub const REASON_EXECUTOR: &str = "el ejecutor falló dos veces seguidas";
pub const REASON_QUOTA: &str = "se agotó la cuota del agente";
pub const REASON_CONTEXT: &str = "el contexto del agente se llenó";
pub const REASON_WINDOW: &str = "se cerró la ventana del agente";
pub const REASON_RESTART: &str = "la app se reinició";

/// Palabra del estado para la fila de la vista rápida (9.8).
pub fn state_label(state: TaskState) -> &'static str {
    match state {
        TaskState::Preparing => "preparando",
        TaskState::Running => "ejecutando",
        TaskState::Verifying => "verificando",
        TaskState::Done => "cumplida",
        TaskState::Paused => "en pausa",
        TaskState::Stopped => "detenida",
    }
}

/// Textos de los esquemas de las herramientas (9.2).
pub const TASK_DESCRIPTION: &str = "start_autonomous_task: el encargo completo, con las palabras \
                                    del usuario.";
pub const CRITERIA_DESCRIPTION: &str = "start_autonomous_task: criterios verificables de \
                                        terminado, uno por elemento.";
pub const AGENT_DESCRIPTION: &str = "start_autonomous_task: agente del catálogo que ejecuta.";
pub const VERIFIER_DESCRIPTION: &str = "start_autonomous_task: agente que verifica; por omisión, \
                                        el mismo del ejecutor.";
pub const SPACE_DESCRIPTION: &str = "start_autonomous_task: espacio de Herdr donde trabaja.";
pub const TERMINAL_DESCRIPTION: &str = "start_autonomous_task: terminal ya abierta del espacio; \
                                        sin él se abre una nueva.";
pub const ROUNDS_DESCRIPTION: &str = "Máximo de rondas antes de pausar y preguntar.";
pub const MINUTES_DESCRIPTION: &str = "Máximo de minutos antes de pausar y preguntar.";
pub const ROLES_DESCRIPTION: &str = "start_autonomous_team: papeles del equipo, en orden de \
                                     trabajo.";
pub const REASON_DESCRIPTION: &str = "stop_autonomous_task: motivo, para la bitácora.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_keep_their_guillemets() {
        assert_eq!(START, "Empiezo en modo autónomo.");
        assert_eq!(STOPPED, "Detuve el modo autónomo.");
        assert_eq!(round(2), "Ronda 2: verificando.");
        assert_eq!(
            paused("se agotaron las rondas"),
            "Pausé el modo autónomo: se agotaron las rondas. Di «sigue» o «para»."
        );
        assert_eq!(
            done("migrar la API", 3),
            "Terminé «migrar la API»: se cumple lo que pediste. Aprobé 3 permisos."
        );
        assert!(confirm("x", "codex", "proyecto", 5, 120).ends_with("¿Lo activo?"));
    }
}

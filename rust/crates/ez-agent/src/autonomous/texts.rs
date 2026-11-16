//! Textos del modo autónomo (9.6 y 10.3). Los genera el código, nunca el modelo.
//!
//! Español neutro latinoamericano, copiado literal del diseño aprobado.

use super::state::TaskState;
use super::Criterion;

/// Cuántos criterios enumera la confirmación antes de remitir a `criterios.md`.
pub const CONFIRM_CRITERIA: usize = 5;

fn criteria_lines(criteria: &[Criterion]) -> String {
    let mut text = String::new();
    for (index, criterion) in criteria.iter().take(CONFIRM_CRITERIA).enumerate() {
        text.push_str(&format!(
            "{}. {} (se comprueba con: {})\n",
            index + 1,
            criterion.text,
            criterion.check
        ));
    }
    if criteria.len() > CONFIRM_CRITERIA {
        text.push_str(&format!(
            "y {} más, en criterios.md.\n",
            criteria.len() - CONFIRM_CRITERIA
        ));
    }
    text
}

/// Confirmación única de activación de una tarea (10.3), generada por el código.
pub fn confirm(
    task: &str,
    agent: &str,
    space: &str,
    workdir: &str,
    criteria: &[Criterion],
    rounds: i64,
    minutes: i64,
) -> String {
    format!(
        "Modo autónomo: haré «{task}» con {agent} en el espacio «{space}», en la carpeta \
         «{workdir}». Aprobaré yo los permisos que pidan los agentes de esta tarea hasta que se \
         cumplan estos criterios:\n{}Como máximo {rounds} rondas o {minutes} minutos. ¿Lo activo?",
        criteria_lines(criteria)
    )
}

/// Confirmación única de activación de un equipo (10.3 y 10.18).
pub fn confirm_team(
    task: &str,
    roles: &str,
    space: &str,
    workdir: &str,
    criteria: &[Criterion],
    rounds: i64,
    minutes: i64,
) -> String {
    format!(
        "Modo autónomo: haré «{task}» con el equipo {roles} en el espacio «{space}», en la carpeta \
         «{workdir}». Aprobaré yo los permisos que pidan los agentes de esta tarea hasta que se \
         cumplan estos criterios:\n{}Como máximo {rounds} rondas o {minutes} minutos. ¿Lo activo?",
        criteria_lines(criteria)
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

/// Pausa que solo retira las aprobaciones (10.13).
pub fn paused_approvals(reason: &str) -> String {
    format!("Pausé las aprobaciones: {reason}. El agente puede seguir trabajando hasta que pida permiso.")
}

/// Fin (9.6).
pub fn done(task: &str, approvals: i64) -> String {
    format!("Terminé «{task}»: se cumple lo que pediste. Aprobé {approvals} permisos.")
}

/// Fin con evidencia (10.16).
pub fn done_with_summary(task: &str, path: &str) -> String {
    format!("Terminé «{task}». Revisa el resumen: {path}.")
}

/// El ejecutor se detuvo sin escribir `fin-N.md` (10.10).
pub fn idle(agent: &str) -> String {
    format!("{agent} se detuvo sin terminar; puede estar esperando una respuesta.")
}

/// Parada (9.6).
pub const STOPPED: &str = "Detuve el modo autónomo.";

/// `resume_autonomous_task` cuando la tarea sigue (10.5).
pub const RESUMED: &str = "Sigo con el modo autónomo.";

/// `resume_autonomous_task` sin una tarea en pausa (10.5).
pub const NOT_PAUSED: &str = "La tarea autónoma no está en pausa.";

/// `stop_autonomous_task` con `accept` (10.5).
pub const ACCEPTED: &str = "Di por aceptado el resultado del modo autónomo.";

/// `autonomous_status` sin tarea (10.5).
pub const NO_TASK: &str = "No hay tarea autónoma.";

/// Guarda 1: el turno leyó datos no confiables.
pub const TAINTED: &str =
    "No puedo activar el modo autónomo: en este turno se leyó web, archivos o \
                           salida de agentes. Pídemelo otra vez en un mensaje nuevo.";

/// Guarda 2: el usuario no lo autorizó en sus últimos mensajes.
pub const NOT_AUTHORIZED: &str = "No puedo activar el modo autónomo: necesito que me lo pidas \
                                  explícitamente («modo autónomo», «autorizo» o «yolo»).";

/// Guarda 3: solo una tarea autónoma a la vez.
pub const ALREADY_ACTIVE: &str =
    "Ya hay una tarea autónoma en curso; deténla antes de empezar otra.";

/// Guarda 4: el usuario dijo que no (o no respondió).
pub const DECLINED: &str = "No activé el modo autónomo.";

/// La confirmación superó el plazo del registro (10.5).
pub const TIMED_OUT: &str =
    "La confirmación tardó más que el límite de la herramienta; no activé el modo autónomo.";

/// `workdir` sin argumento y sin `default_cwd` (10.2).
pub const WORKDIR_MISSING: &str = "¿En qué carpeta trabajo?";

/// Un criterio sin su comprobación (10.3).
pub const MISSING_CHECK: &str =
    "Falta «check» en un criterio: dime cómo se comprueba (un comando o una comprobación observable).";

/// Motivos de pausa (9.5, 10.10 y 10.13).
pub const REASON_DEADLINE: &str = "se cumplió el tiempo máximo";
pub const REASON_ROUNDS: &str = "se agotaron las rondas";
pub const REASON_CHURN: &str = "el mismo problema vuelve";
pub const REASON_VERDICT: &str = "el verificador no devolvió un veredicto válido dos veces";
pub const REASON_VERIFICATIONS: &str = "se agotaron las verificaciones de la ronda";
pub const REASON_DOUBTFUL: &str = "el veredicto no cumple pero no dice qué corregir";
pub const REASON_EXECUTOR: &str = "el ejecutor falló dos veces seguidas";
pub const REASON_APPROVALS: &str = "se alcanzó el máximo de aprobaciones";
pub const REASON_LOST: &str = "se perdió la terminal del agente";
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

/// Textos de los esquemas de las herramientas (10.2 y 10.3).
pub const TASK_DESCRIPTION: &str =
    "start_autonomous_task: el encargo completo, con las palabras del usuario.";
pub const CRITERIA_DESCRIPTION: &str = "start_autonomous_task: criterios verificables de \
                                        terminado, cada uno con su texto y su comprobación.";
pub const CRITERION_TEXT_DESCRIPTION: &str = "Qué debe cumplirse.";
pub const CRITERION_CHECK_DESCRIPTION: &str =
    "El comando o la comprobación observable que demuestra el criterio.";
pub const AGENT_DESCRIPTION: &str = "start_autonomous_task: agente del catálogo que ejecuta.";
pub const VERIFIER_DESCRIPTION: &str = "start_autonomous_task: agente que verifica; por omisión, \
                                        el mismo del ejecutor.";
pub const SPACE_DESCRIPTION: &str = "start_autonomous_task: espacio de Herdr donde trabaja.";
pub const WORKDIR_DESCRIPTION: &str =
    "Carpeta de trabajo de la tarea; sin ella se usa «default_cwd» de voice.json.";
pub const TERMINAL_DESCRIPTION: &str = "start_autonomous_task: terminal ya abierta del espacio; \
                                        sin él se abre una nueva.";
pub const ROUNDS_DESCRIPTION: &str = "Máximo de rondas antes de pausar y preguntar.";
pub const MINUTES_DESCRIPTION: &str = "Máximo de minutos antes de pausar y preguntar.";
pub const ROLES_DESCRIPTION: &str = "start_autonomous_team: papeles del equipo, en orden de \
                                     trabajo.";
pub const REASON_DESCRIPTION: &str = "stop_autonomous_task: motivo, para la bitácora.";
pub const ACCEPT_DESCRIPTION: &str = "Da por aceptado el resultado de una tarea cumplida.";
pub const EXTEND_ROUNDS_DESCRIPTION: &str = "Rondas extra al reanudar una tarea en pausa.";
pub const EXTEND_MINUTES_DESCRIPTION: &str = "Minutos extra al reanudar una tarea en pausa.";

#[cfg(test)]
mod tests {
    use super::*;

    fn criteria() -> Vec<Criterion> {
        vec![Criterion {
            text: "compila".to_string(),
            check: "cargo build".to_string(),
        }]
    }

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
        assert_eq!(
            done_with_summary("migrar la API", "C:/t/privado/resumen.md"),
            "Terminé «migrar la API». Revisa el resumen: C:/t/privado/resumen.md."
        );
        assert_eq!(
            idle("codex"),
            "codex se detuvo sin terminar; puede estar esperando una respuesta."
        );
        assert_eq!(
            paused_approvals("se alcanzó el máximo de aprobaciones"),
            "Pausé las aprobaciones: se alcanzó el máximo de aprobaciones. El agente puede seguir \
             trabajando hasta que pida permiso."
        );
    }

    #[test]
    fn confirmation_enumerates_the_criteria_and_the_folder() {
        let text = confirm("migrar", "codex", "proyecto", "C:/t", &criteria(), 5, 120);
        assert!(text.starts_with("Modo autónomo: haré «migrar» con codex"));
        assert!(text.contains("en la carpeta «C:/t»"));
        assert!(text.contains("1. compila (se comprueba con: cargo build)"));
        assert!(text.ends_with("Como máximo 5 rondas o 120 minutos. ¿Lo activo?"));
        let team = confirm_team(
            "migrar",
            "plan: codex, dev: opencode (big)",
            "proyecto",
            "C:/t",
            &criteria(),
            5,
            120,
        );
        assert!(team.contains("con el equipo plan: codex, dev: opencode (big)"));
    }

    #[test]
    fn more_than_five_criteria_point_to_the_file() {
        let many: Vec<Criterion> = (1..=7)
            .map(|n| Criterion {
                text: format!("criterio {n}"),
                check: format!("check {n}"),
            })
            .collect();
        let text = confirm("migrar", "codex", "proyecto", "C:/t", &many, 5, 120);
        assert!(text.contains("5. criterio 5"));
        assert!(!text.contains("6. criterio 6"));
        assert!(text.contains("y 2 más, en criterios.md."));
    }
}

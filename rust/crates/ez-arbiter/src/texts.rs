//! Textos que lee la persona o un agente. Todos en español neutro y generados
//! por el código; nunca vienen del LLM.

use std::path::Path;

use crate::types::Verdict;

/// R1: el comando es del elefante.
pub const ELEPHANT_ONLY: &str =
    "Ese comando lo ejecuta el agente principal del espacio, no un subagente.";
/// R1: el comando es de un pez.
pub const FISH_ONLY: &str = "Solo un subagente puede enviar un informe con `ez report`.";
/// El espacio no está registrado.
pub const UNKNOWN_SPACE: &str =
    "Ese espacio no está registrado en el árbitro: ábrelo en la ventana primero.";
/// R1: el pez no pertenece a ninguna tarea activa.
pub const UNKNOWN_FISH: &str =
    "Ese subagente no pertenece a ninguna tarea activa: revisa el identificador.";
/// No hay tarea activa en el espacio.
pub const NO_ACTIVE_TASK: &str =
    "No hay una tarea activa en este espacio: crea una con `ez task new`.";
/// Ya hay tarea activa.
pub const ACTIVE_TASK: &str =
    "Ya hay una tarea activa en este espacio: ciérrala o detenla antes de crear otra.";
/// La tarea existe pero no está aprobada.
pub const NOT_APPROVED: &str = "La persona todavía no aprobó la tarea.";
/// El equipo no está aprobado.
pub const TEAM_NOT_APPROVED: &str = "La persona todavía no aprobó el equipo.";
/// El equipo no fue propuesto.
pub const TEAM_NOT_PROPOSED: &str = "Todavía no hay un equipo propuesto para esta tarea.";
/// R4: ya hay ronda.
pub const ROUND_OPEN: &str = "Ya hay una ronda abierta: ciérrala antes de empezar otra.";
/// No hay ronda abierta.
pub const NO_ROUND: &str =
    "No hay una ronda abierta en esta tarea: empieza una con `ez round start`.";
/// R10: la tarea ya terminó.
pub const TASK_FINISHED: &str = "La tarea ya terminó.";
/// R2: el encargo no existe.
pub fn brief_missing(path: &Path) -> String {
    format!("No existe el encargo: {}.", path.display())
}
/// R2: los criterios no existen.
pub fn criteria_missing(path: &Path) -> String {
    format!("No existe el archivo de criterios: {}.", path.display())
}
/// R2: un archivo está fuera de la carpeta del espacio.
pub fn outside_folder(path: &Path) -> String {
    format!(
        "El archivo {} está fuera de la carpeta del espacio.",
        path.display()
    )
}
/// R2 (literal del diseño §8).
pub const GIT_NOT_REPO: &str =
    "La carpeta del espacio no es un repositorio git: ejecuta `git init` primero.";
/// No se pudo leer el estado de git.
pub fn git_failed(detail: &str) -> String {
    format!("No se pudo leer el estado de git: {detail}.")
}
/// El papel no está en el equipo.
pub fn role_not_in_team(role: &str) -> String {
    format!("El papel {role} no está en el equipo aprobado de esta tarea.")
}
/// R5: falta el informe de otros peces.
pub fn waiting_for(fish: &[String]) -> String {
    format!(
        "Todavía no: falta el informe de {} en esta ronda.",
        fish.join(", ")
    )
}
/// R5: ya hay un pez de ese papel en la ronda.
pub fn already_in_round(role: &str) -> String {
    format!("Ya hay un subagente de {role} en esta ronda; solo se permite uno.")
}
/// R5: se alcanzó el máximo de subagentes vivos de un papel.
pub fn too_many_fish(alive: u32, max: u32) -> String {
    format!("Ya hay {alive} desarrolladores vivos y el equipo permite {max}.")
}

/// R9: ningún verificador informó en la ronda.
pub fn no_verifier_reported() -> String {
    "Ningún verificador informó en esta ronda.".to_string()
}
/// R5: el archivo del encargo del pez no existe.
pub fn fish_brief_missing(path: &Path) -> String {
    format!("No existe el encargo del subagente: {}.", path.display())
}
/// R6: no se puede corregir a un verificador.
pub fn cannot_correct(fish: &str) -> String {
    format!(
        "No se puede enviar una corrección a {fish}: solo al planificador o a un desarrollador."
    )
}
/// R6: la terminal ya no está viva.
pub fn terminal_dead(fish: &str) -> String {
    format!("La terminal de {fish} ya no está viva: no se puede enviar la corrección.")
}
/// R6: el archivo de la corrección no existe.
pub fn correction_missing(path: &Path) -> String {
    format!("No existe el archivo de la corrección: {}.", path.display())
}
/// R6: la línea exacta que se envía al pez (diseño §14.3).
pub fn correction_line(round: u32, number: u32, path: &Path, report: &Path) -> String {
    format!(
        "Corrección {number} de la ronda {round}: lee {} y al terminar informa con ez report usando --file \"{}\".",
        path.display(),
        report.display()
    )
}
/// R7: el pez no tiene trabajo pendiente.
pub fn no_pending_work(fish: &str) -> String {
    format!("{fish} no tiene trabajo pendiente en esta ronda: no hay nada que informar.")
}
/// R7: un verificador ya informó.
pub fn already_reported(fish: &str) -> String {
    format!("{fish} ya informó en esta ronda: los verificadores informan una sola vez.")
}
/// R7: faltan los números de mutaciones.
pub const MUTATIONS_REQUIRED: &str =
    "El papel que valida mutaciones debe informar `--mutations-tried` y `--mutations-caught`.";
/// R7: falta el archivo del informe.
pub fn report_missing(path: &Path) -> String {
    format!("No existe el informe: {}.", path.display())
}
/// R8: el código de salida no es el esperado.
pub fn wrong_code(got: Option<i32>, expected: i32) -> String {
    let got = match got {
        Some(code) => code.to_string(),
        None => "ninguno (lo mató el tope)".to_string(),
    };
    format!("el código fue {got} y se esperaba {expected}")
}
/// R8: falta el texto esperado.
pub fn lacks_text(text: &str) -> String {
    format!("la salida no contiene «{text}»")
}
/// R8: venció el tope de tiempo.
pub fn timed_out(seconds: u64) -> String {
    format!("venció el tope de {seconds} s")
}
/// R8: no se pudo ejecutar el comando.
pub fn command_failed(detail: &str) -> String {
    format!("no se pudo ejecutar el comando: {detail}")
}
/// R9: faltan informes para cerrar.
pub fn close_needs(reporters: &[String]) -> String {
    format!(
        "Todavía no: faltan los informes de {} en esta ronda.",
        reporters.join(", ")
    )
}
/// R9: ronda limpia.
pub fn round_clean(round: u32) -> String {
    format!("Ronda {round} limpia.")
}
/// R9: ronda sucia.
pub fn round_dirty(round: u32, reasons: &[String]) -> String {
    format!("Ronda {round} sucia: {}.", reasons.join("; "))
}
/// R9: un criterio no pasó.
pub fn criterion_failed(id: &str, reason: &str) -> String {
    format!("el criterio {id} no pasó porque {reason}")
}
/// R9: el QA no dio paso.
pub fn qa_not_clean() -> String {
    "el QA no informó `pass` sin bloqueantes ni mayores".to_string()
}
/// R9: el validador no dio paso.
pub fn validator_not_clean() -> String {
    "el validador de QA no cumplió las mutaciones mínimas o no detectó todas".to_string()
}
/// R9: un archivo protegido cambió.
pub fn protected_touched(paths: &[String]) -> String {
    format!("un archivo protegido del QA cambió: {}", paths.join(", "))
}
/// R10 (literal del diseño).
pub fn two_clean_rounds(streak: u32) -> String {
    format!("Todavía no: hacen falta dos rondas limpias seguidas (llevas {streak}).")
}
/// R11: cambios del elefante fuera de ronda.
pub fn elephant_changed_outside_round(paths: &[String]) -> String {
    format!(
        "Hay cambios fuera de `.ez/` desde el cierre de la ronda anterior ({}): la tarea queda en pausa.",
        paths.join(", ")
    )
}
/// R3: los criterios cambiaron.
pub const CRITERIA_CHANGED: &str =
    "Los criterios cambiaron después de la aprobación: la tarea queda en pausa.";
/// Pausa por tope de rondas.
pub fn max_rounds(round: u32) -> String {
    format!("Se llegó al tope de {round} rondas: la tarea queda en pausa.")
}
/// Pausa porque el error se repite.
pub const SAME_ERROR: &str =
    "Los criterios que fallan y sus salidas son iguales a los de la ronda anterior: la tarea queda en pausa.";
/// Pausa por tope de tiempo.
pub fn max_hours(hours: f64) -> String {
    format!("Pasaron más de {hours} horas desde la aprobación: la tarea queda en pausa.")
}
/// Pausa manual sin motivo.
pub const PAUSED_MANUAL: &str = "La persona puso la tarea en pausa.";
/// Pausa sin motivo escrito.
pub const PAUSED_NO_REASON: &str = "Pausa.";
/// Está en pausa.
pub fn paused(reason: &str) -> String {
    format!("La tarea está en pausa: {reason} Sigue la persona con `resume`.")
}
/// Reanudada.
pub const RESUMED: &str = "La tarea se reanudó.";
/// Detenida.
pub const STOPPED: &str = "La tarea quedó detenida por la persona.";
/// Tarea aprobada.
pub const TASK_APPROVED: &str = "Tarea aprobada: los criterios quedan congelados.";
/// Tarea rechazada.
pub fn task_rejected(comment: &str) -> String {
    if comment.trim().is_empty() {
        "La persona rechazó la tarea.".to_string()
    } else {
        format!("La persona rechazó la tarea: {comment}")
    }
}
/// Equipo aprobado.
pub const TEAM_APPROVED: &str = "Equipo aprobado.";
/// Equipo rechazado.
pub fn team_rejected(comment: &str) -> String {
    if comment.trim().is_empty() {
        "La persona rechazó el equipo.".to_string()
    } else {
        format!("La persona rechazó el equipo: {comment}")
    }
}
/// Tarea cerrada.
pub fn task_closed(streak: u32) -> String {
    format!("Tarea cerrada con {streak} rondas limpias seguidas.")
}
/// Ronda abierta.
pub fn round_started(round: u32) -> String {
    format!("Ronda {round} abierta.")
}
/// Pez lanzado.
pub fn fish_launched(fish: &str, terminal: &str) -> String {
    format!("Subagente {fish} lanzado en {terminal}.")
}
/// Corrección enviada.
pub fn correction_sent(fish: &str) -> String {
    format!("Corrección enviada a {fish}.")
}
/// Informe recibido.
pub fn report_received(fish: &str, verdict: Verdict) -> String {
    format!("Informe de {fish} recibido: {}.", verdict.id())
}
/// Verificación sin criterios fallidos.
pub const VERIFY_OK: &str = "Verificación: todos los criterios pasan.";
/// Verificación con criterios fallidos.
pub fn verify_failed(failed: usize, total: usize) -> String {
    format!("Verificación: {failed} de {total} criterios fallan.")
}
/// Elefante: pez bloqueado.
pub fn elephant_blocked(fish: &str, kind: Option<&str>) -> String {
    match kind {
        Some(kind) => format!("El subagente {fish} quedó bloqueado ({kind})."),
        None => format!("El subagente {fish} quedó bloqueado."),
    }
}
/// Elefante: pez terminó su terminal.
pub fn elephant_exited(fish: &str) -> String {
    format!("La terminal del subagente {fish} terminó.")
}
/// Criterio en la tarjeta.
pub fn card_criterion(id: &str, description: &str, command: &str) -> String {
    format!("- {id}: {description} → {command}")
}
/// Tarjeta de tarea propuesta.
pub fn task_card(title: &str, brief: &str, criteria: &[String]) -> String {
    let excerpt: Vec<&str> = brief.lines().take(40).collect();
    let mut card = format!(
        "Tarea propuesta: {title}\n\n{}\n\nCriterios:",
        excerpt.join("\n")
    );
    for line in criteria {
        card.push('\n');
        card.push_str(line);
    }
    card
}
/// Estado del elefante.
pub fn status_elephant(
    space: &str,
    task: &str,
    title: &str,
    status: &str,
    round: u32,
    streak: u32,
    team: &str,
) -> String {
    format!(
        "Espacio {space}: tarea {task} «{title}» en {status} (ronda {round}, {streak} rondas limpias seguidas, equipo {team})."
    )
}
/// Estado del pez.
pub fn status_fish(fish: &str, role: &str, task: &str, round: u32, pending: bool) -> String {
    let work = if pending {
        "tienes trabajo pendiente: informa con `ez report`"
    } else {
        "ya informaste en esta ronda"
    };
    format!("Eres {fish} ({role}) en la ronda {round} de la tarea {task}: {work}.")
}
/// Estado de un pez sin tarea.
pub const STATUS_UNKNOWN_FISH: &str = "Ese subagente no tiene una tarea activa.";
/// Estado de un espacio sin tarea.
pub const STATUS_NO_TASK: &str = "El espacio no tiene una tarea activa.";
/// Nombre del estado para la persona.
pub fn status_label(status: &str) -> &'static str {
    match status {
        "pending_approval" => "esperando aprobación",
        "approved" => "aprobada",
        "in_round" => "en ronda",
        "between_rounds" => "entre rondas",
        "paused" => "en pausa",
        "done" => "terminada",
        "stopped" => "detenida",
        "rejected" => "rechazada",
        _ => "desconocida",
    }
}
/// La tarea no está esperando aprobación.
pub const NOT_PENDING: &str = "La tarea no está esperando aprobación de la persona.";
/// R10: antes hay que cerrar la ronda.
pub const CLOSE_ROUND_FIRST: &str = "Todavía no: cierra la ronda abierta antes de cerrar la tarea.";
/// R4: ya está en pausa.
pub const ALREADY_PAUSED: &str = "La tarea ya está en pausa.";
/// R4: no está en pausa.
pub const NOT_PAUSED: &str = "La tarea no está en pausa.";

/// R1: la clave del pez no coincide con ningún trabajo vigente (§14.1).
pub const FISH_NOT_ACTIVE: &str = "Este subagente ya no está activo en esta tarea.";
/// R5: la entrega del encargo falló (§14.2).
pub fn delivery_failed(fish: &str, detail: &str) -> String {
    format!("No pude entregar el encargo al subagente {fish}: {detail}")
}
/// R5: la entrega no se pudo confirmar; el trabajo sigue.
pub fn delivery_unconfirmed(fish: &str) -> String {
    format!(" No se pudo confirmar la entrega al subagente {fish}; revisa su terminal.")
}
/// R7: el informe no está en la ruta del trabajo pendiente (§14.3).
pub fn wrong_report_path(path: &Path) -> String {
    format!("El informe debe ir en {}.", path.display())
}
/// R7: `pass` con hallazgos bloqueantes o mayores (§14.3).
pub const PASS_WITH_FINDINGS: &str =
    "El veredicto pass no puede llevar hallazgos bloqueantes o mayores.";
/// R9: queda trabajo pendiente al cerrar (§14.3).
pub fn pending_work(fish: &[String]) -> String {
    format!("Hay trabajo pendiente: {}.", fish.join(", "))
}
/// R5/R6: el QA ya empezó en esta ronda (§14.3).
pub const QA_STARTED: &str =
    "Ya empezó el QA de esta ronda: las correcciones van en la ronda siguiente.";
/// R5: el plan solo se hace en la ronda 1 (§14.3).
pub const PLAN_ONLY_ROUND1: &str =
    "El plan se hace en la ronda 1; para corregirlo usa ez send al planificador.";
/// R3/§14.6: cambiaron los artefactos congelados.
pub const ARTIFACTS_MODIFIED: &str =
    "Los artefactos congelados de la tarea cambiaron: la tarea queda en pausa.";
/// Reanudar una pausa de artefactos restaura las copias.
pub const ARTIFACTS_RESTORED: &str = "Se restauraron los artefactos congelados.";
/// Línea de validación en la tarjeta (§14.6).
pub fn validation_line(mutations: u32, rounds: u32, hours: f64) -> String {
    format!("Validación: mutaciones mínimas {mutations}, rondas máximas {rounds}, horas máximas {hours}.")
}
/// Novedad para el elefante: un pez falló antes de informar (§14.5).
pub fn fish_failed(fish: &str, reason: &str) -> String {
    format!("{fish} falló antes de informar: {reason}.")
}
/// R5: se agotaron los tres intentos de un papel en la ronda (§14.3).
pub fn too_many_attempts(role: &str) -> String {
    format!("{role} agotó los 3 intentos de esta ronda.")
}
/// R5: no se puede abrir una ronda solo de verificación (§14.3).
pub fn no_verification_round() -> String {
    "Solo se puede hacer una ronda sin desarrolladores si la anterior quedó limpia y el proyecto no cambió.".to_string()
}
/// R9: un informe de la ronda no fue `pass` limpio (§14.3).
pub fn reports_not_clean(fish: &[String]) -> String {
    format!(
        "los informes de {} no son pass sin bloqueantes ni mayores",
        fish.join(", ")
    )
}
/// R9: un papel que no debe tocar el proyecto lo hizo (§14.3).
pub fn role_changed_files(role: &str, paths: &[String]) -> String {
    format!("{role} cambió archivos del proyecto: {}", paths.join(", "))
}
/// R10: la tarea solo se cierra entre rondas.
pub fn close_between_rounds() -> String {
    "Todavía no: la tarea solo se cierra entre rondas.".to_string()
}
/// R10/§14.4: el proyecto cambió desde la última ronda limpia.
pub fn project_changed_since_clean() -> String {
    "El proyecto cambió desde la última ronda limpia: hace falta otra ronda.".to_string()
}
/// §14.5: las tareas no finales se pausan al reiniciar.
pub const RESTART: &str = "El árbitro se reinició: revisa el trabajo pendiente y reanuda.";
/// §14.5: la persona amplía el margen y reanuda.
pub fn more_time(rounds: u32, hours: f64) -> String {
    format!("La tarea se reanudó con {rounds} rondas y {hours} horas de margen adicionales.")
}

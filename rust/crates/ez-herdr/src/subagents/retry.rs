//! `ez retry`: relanzar el encargo de un subagente bloqueado o fallido
//! (`novedades-subagentes.md` §4).
//!
//! Lanza un hijo nuevo del mismo padre con nombre `<nombre>-r<n>`, en una
//! sesión nueva (una terminal nueva nunca retoma la sesión anterior), y deja
//! el registro viejo en `fallido` con «reemplazado por …». Cierra la terminal
//! vieja solo después de que la nueva arrancó bien y si no se pidió `--keep`.
//!
//! La elección del tipo, el modelo y el esfuerzo la hace quien llama
//! (`ez-ctl` con `ez-scan`): este módulo no depende de `ez-scan`.

use ez_core::pyvalue::truthy;
use serde_json::{Map, Value};

use crate::driver::Choice;
use crate::herdr::{DelegateError, DelegateRequest, Herdr, LaunchSupport, Placement};

use super::{one_line, Change, Record, Registry, TaskState};

/// Tope de nombres que se prueban antes de rendirse.
const MAX_NAME_PROBES: usize = 100;

/// Intentos de lanzamiento si `hc` dice que el nombre se ocupó a la carrera.
const MAX_LAUNCH_ATTEMPTS: usize = 5;

/// El máximo de un nombre de `hc` (§6).
const NAME_MAX: usize = 32;

/// La validación del catálogo que recibe el reintento (la de `ez delegate`).
type Check<'a> = &'a dyn Fn(&str, Option<&str>, Option<&str>) -> Result<Choice, String>;

/// Lo que necesita un reintento ya decidido por el llamador.
#[derive(Debug, Clone)]
pub struct RetryRequest {
    /// La terminal del padre (la misma del registro viejo).
    pub parent: String,
    /// El registro que se reemplaza.
    pub old: Record,
    /// El tipo elegido para el hijo nuevo.
    pub agent: String,
    /// El modelo elegido, si lo hay.
    pub model: Option<String>,
    /// El esfuerzo elegido, si lo hay.
    pub effort: Option<String>,
    /// El encargo completo que se le entregará.
    pub brief: String,
    /// La ruta del encargo (la del registro viejo), si se conoce.
    pub brief_path: Option<String>,
    /// La causa que se le cuenta al hijo.
    pub cause: String,
    /// Si la terminal vieja se conserva (`--keep`).
    pub keep_terminal: bool,
}

/// Por qué no se pudo reintentar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryError {
    /// El catálogo, el registro o `hc` lo rechazaron (código 1).
    Rejected(String),
    /// No hay `hc` (código 3).
    Unavailable(String),
}

impl RetryError {
    /// El texto para la persona.
    pub fn message(&self) -> &str {
        match self {
            Self::Rejected(message) | Self::Unavailable(message) => message,
        }
    }

    /// El código de salida de `ez retry`.
    pub fn code(&self) -> i32 {
        match self {
            Self::Rejected(_) => 1,
            Self::Unavailable(_) => 3,
        }
    }
}

impl std::fmt::Display for RetryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl std::error::Error for RetryError {}

impl From<DelegateError> for RetryError {
    fn from(error: DelegateError) -> Self {
        match error {
            DelegateError::Rejected(message) => Self::Rejected(message),
            DelegateError::Unavailable(message) => Self::Unavailable(message),
        }
    }
}

/// Lo que dejó un reintento.
#[derive(Debug, Clone)]
pub struct RetryOutcome {
    /// El resultado del lanzamiento, listo para la salida (`--json`).
    pub row: Map<String, Value>,
    /// El registro nuevo, si se pudo guardar.
    pub record: Option<Record>,
    /// Si se cerró la terminal vieja.
    pub closed_old: bool,
}

/// Un reintento automático del vigilante (`planes.toml` §4).
///
/// Lo implementa `ez watch`, que sí puede usar `ez-scan` para recomendar; el
/// vigilante solo pregunta si la persona permitió el reintento y, si sí, cuál
/// fue el nombre nuevo.
pub trait AutoRetry: Send + Sync {
    /// `Ok(None)` si la persona no permitió el reintento automático.
    fn auto_retry(&self, record: &Record, cause: &str) -> Result<Option<String>, String>;
}

/// El nombre `<base>-r<n>`, recortado a 32 letras.
///
/// Si el nombre viejo ya termina en `-r<dígitos>`, se continúa esa serie
/// (`dev1-r1` → `dev1-r2`) en vez de encadenar sufijos.
pub fn retry_name(base: &str, n: usize) -> String {
    let suffix = format!("-r{n}");
    let keep = NAME_MAX.saturating_sub(suffix.chars().count());
    let trimmed: String = retry_base(base).chars().take(keep).collect();
    format!("{trimmed}{suffix}")
}

/// El nombre sin su sufijo `-r<dígitos>`, si lo tiene.
fn retry_base(name: &str) -> &str {
    match name.rsplit_once("-r") {
        Some((base, number))
            if !base.is_empty()
                && !number.is_empty()
                && number.chars().all(|c| c.is_ascii_digit()) =>
        {
            base
        }
        _ => name,
    }
}

/// El primer nombre libre de la serie, mirando el registro y `hc`.
pub fn next_retry_name(
    registry: &Registry,
    parent: &str,
    old_name: &str,
    taken: &[String],
) -> Result<String, RetryError> {
    for n in 1..=MAX_NAME_PROBES {
        let candidate = retry_name(old_name, n);
        if taken
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&candidate))
        {
            continue;
        }
        match registry.get(parent, &candidate) {
            Ok(None) => return Ok(candidate),
            Ok(Some(_)) => continue,
            Err(error) => return Err(RetryError::Rejected(error.message().to_string())),
        }
    }
    Err(RetryError::Rejected(format!(
        "No encontré un nombre libre para {old_name}-r<n>."
    )))
}

/// Relanza el encargo del registro viejo y deja los registros enlazados.
///
/// `launcher` y `check` son los mismos de `ez delegate`; la elección del tipo,
/// el modelo y el esfuerzo ya viene decidida en `request`.
pub fn retry<L: LaunchSupport + Clone + Send + 'static>(
    herdr: &Herdr,
    launcher: L,
    check: Check<'_>,
    request: &RetryRequest,
    registry: &Registry,
) -> Result<RetryOutcome, RetryError> {
    if !matches!(
        request.old.state,
        TaskState::InProgress | TaskState::Blocked | TaskState::Done | TaskState::Failed
    ) {
        return Err(RetryError::Rejected(format!(
            "{} está «{}»; no se reintenta lo ya validado.",
            request.old.name,
            request.old.state.as_str()
        )));
    }
    let mut taken = names_in_use(herdr);
    let mut attempts = 0;
    // DECISION: un padre que no es una terminal (la app de voz, `voz`) lanza
    // al hijo SIN `--parent`: `hc run --parent voz` da `not_found` (19-nov).
    // El registro sigue bajo `request.parent`; solo cambia el padre de `hc`.
    let hc_parent = if herdr.get_agent(&request.parent).is_ok() {
        request.parent.as_str()
    } else {
        ""
    };
    loop {
        attempts += 1;
        let name = next_retry_name(registry, &request.parent, &request.old.name, &taken)?;
        let delegate_request = delegate_request(request, &name, hc_parent);
        match herdr.delegate(launcher.clone(), check, &delegate_request, None) {
            Ok(row) => return finish(herdr, request, registry, row, &name),
            Err(DelegateError::Rejected(message))
                if is_name_taken(&message) && attempts < MAX_LAUNCH_ATTEMPTS =>
            {
                taken.push(name);
            }
            Err(error) => return Err(error.into()),
        }
    }
}

/// El `ez delegate` del reintento: el mismo encargo con la nota de §4 delante.
fn delegate_request(request: &RetryRequest, name: &str, hc_parent: &str) -> DelegateRequest {
    // DECISION: el lanzamiento no retoma ninguna sesión anterior (aquí no hay
    // `--session`): una terminal nueva es una sesión nueva, que es justo la
    // lección de OpenCode del 19-nov.
    //
    // DECISION: la nota va delante del encargo (el primer mensaje de §7 mete el
    // encargo tal cual); `brief_head` del registro nuevo guarda solo el encargo.
    let brief = format!(
        "{}\n\n{}",
        retry_note(&request.old, &request.cause),
        request.brief
    );
    DelegateRequest {
        role: request.old.role.clone(),
        agent: request.agent.clone(),
        model: request.model.clone(),
        effort: request.effort.clone(),
        brief,
        name: Some(name.to_string()),
        // DECISION: el reintento va al árbol para no mover el foco de la
        // persona; la ubicación no es parte de la resolución de la novedad.
        show: Placement::Tree,
        cwd: Some(request.old.cwd.clone()).filter(|folder| !folder.trim().is_empty()),
        parent: hc_parent.to_string(),
        // DECISION: los avisos los da EzAgent (§13), nunca `hc`.
        notify: false,
        outlive_parent: false,
        brief_path: request.brief_path.clone(),
    }
}

/// La nota que recibe el hijo nuevo (§4).
fn retry_note(old: &Record, cause: &str) -> String {
    let cause = one_line(cause, 200);
    let cause = cause.trim_end_matches('.');
    let cause = if cause.is_empty() {
        "una novedad"
    } else {
        cause
    };
    format!(
        "Retomas el trabajo de {}, que se detuvo por {cause}. Revisa `git status` y el registro antes de seguir.",
        old.name
    )
}

/// Guarda el registro nuevo, marca el viejo y cierra su terminal si toca.
fn finish(
    herdr: &Herdr,
    request: &RetryRequest,
    registry: &Registry,
    mut row: Map<String, Value>,
    candidate: &str,
) -> Result<RetryOutcome, RetryError> {
    let name = text(&row, "name");
    let name = if name.is_empty() {
        candidate.to_string()
    } else {
        name
    };
    // El resultado habla del registro: su padre es siempre el del registro
    // viejo, aunque el lanzamiento no haya tenido padre de `hc`.
    row.insert("parent".to_string(), Value::String(request.parent.clone()));
    let record = build_record(request, &row, &name);
    let saved = match registry.create(&record) {
        Ok(()) => Some(record.clone()),
        Err(error) => {
            push_warning(
                &mut row,
                format!("El subagente arrancó pero no pude guardar su registro: {error}"),
            );
            None
        }
    };
    row.insert(
        "state".to_string(),
        Value::String(record.state.as_str().to_string()),
    );
    mark_old(registry, request, &name, &mut row);
    let terminal = text(&row, "terminal");
    // Sin encargo entregado la nueva no arrancó «bien»: la vieja se conserva.
    let sent = row.get("task_sent").is_some_and(truthy);
    let closed_old = sent && close_old(herdr, request, &terminal, &mut row);
    row.insert(
        "retry_of".to_string(),
        Value::String(request.old.name.clone()),
    );
    row.insert("closed_old".to_string(), Value::Bool(closed_old));
    row.insert(
        "old_terminal".to_string(),
        Value::String(request.old.terminal.clone()),
    );
    Ok(RetryOutcome {
        row,
        record: saved,
        closed_old,
    })
}

/// El registro del hijo nuevo, enlazado con el viejo.
fn build_record(request: &RetryRequest, row: &Map<String, Value>, name: &str) -> Record {
    let now = super::epoch_ms();
    let sent = row.get("task_sent").is_some_and(truthy);
    let (state, detail) = if sent {
        (
            TaskState::InProgress,
            format!("encargo entregado (reintento de {})", request.old.name),
        )
    } else {
        (
            TaskState::Failed,
            "no se pudo entregar el encargo al subagente".to_string(),
        )
    };
    Record {
        name: name.to_string(),
        terminal: text(row, "terminal"),
        // El registro vive bajo el padre del registro viejo aunque el
        // lanzamiento de `hc` no tenga padre (la app de voz).
        parent: request.parent.clone(),
        agent: text(row, "agent"),
        label: text(row, "label"),
        model: optional_text(row, "model"),
        effort: optional_text(row, "effort"),
        role: request.old.role.clone(),
        cwd: request.old.cwd.clone(),
        brief_head: one_line(&request.brief, 200),
        brief_path: request.brief_path.clone(),
        retry_of: Some(request.old.name.clone()),
        state,
        since_ms: now,
        history: vec![Change {
            state,
            at_ms: now,
            by: "ez retry".to_string(),
            detail,
        }],
        summary: None,
        report: None,
        validation: None,
        notified: None,
    }
}

/// El registro viejo pasa a `fallido` («reemplazado por …»).
fn mark_old(
    registry: &Registry,
    request: &RetryRequest,
    new_name: &str,
    row: &mut Map<String, Value>,
) {
    // DECISION: si ya estaba `fallido` no se reescribe (es final); el enlace
    // queda en el registro nuevo con `retry_of`.
    if request.old.state == TaskState::Failed {
        return;
    }
    let detail = format!("reemplazado por {new_name}");
    if let Err(error) = registry.transition(
        &request.old.parent,
        &request.old.name,
        TaskState::Failed,
        "ez retry",
        &detail,
        |_| {},
    ) {
        push_warning(
            row,
            format!("No pude marcar {} como fallido: {error}", request.old.name),
        );
    }
}

/// Cierra la terminal vieja solo si sigue viva y la nueva ya arrancó.
fn close_old(
    herdr: &Herdr,
    request: &RetryRequest,
    new_terminal: &str,
    row: &mut Map<String, Value>,
) -> bool {
    let terminal = &request.old.terminal;
    if request.keep_terminal || terminal.is_empty() || terminal == new_terminal {
        return false;
    }
    if !terminal_live(herdr, terminal) {
        return false;
    }
    match herdr.kill_agent(terminal, true) {
        Ok(_) => true,
        Err(error) => {
            push_warning(
                row,
                format!("No pude cerrar la terminal vieja {terminal}: {error}"),
            );
            false
        }
    }
}

/// ¿La terminal sigue viva? (`hc ls` da la vida en el nivel superior; el
/// `status.state` de `hc get` es el del agente, no el de la terminal).
fn terminal_live(herdr: &Herdr, terminal: &str) -> bool {
    let argv = vec![
        herdr.hc_path(),
        "ls".to_string(),
        "--all".to_string(),
        "--json".to_string(),
    ];
    let Ok(data) = herdr.json_command(&argv, 20.0) else {
        return false;
    };
    data.get("terminals")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|item| {
            item.get("address").and_then(Value::as_str) == Some(terminal)
                && item.get("state").and_then(Value::as_str) == Some("live")
        })
}

/// Los nombres que `hc` ya tiene (para no chocar con terminales sin registro).
fn names_in_use(herdr: &Herdr) -> Vec<String> {
    herdr
        .list_agents(20.0)
        .unwrap_or_default()
        .iter()
        .filter_map(|agent| agent.get("name").and_then(Value::as_str))
        .map(str::to_lowercase)
        .collect()
}

/// ¿`hc` rechazó el lanzamiento porque el nombre ya estaba?
fn is_name_taken(message: &str) -> bool {
    let message = message.to_lowercase();
    message.contains("already named") || message.contains("name_taken")
}

/// Un texto del resultado.
fn text(row: &Map<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Un texto opcional del resultado (vacío = `None`).
fn optional_text(row: &Map<String, Value>, key: &str) -> Option<String> {
    let value = text(row, key);
    (!value.trim().is_empty()).then_some(value)
}

/// Añade un aviso al resultado.
fn push_warning(row: &mut Map<String, Value>, message: String) {
    let mut warnings = match row.get("warnings") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().unwrap_or_default().to_string())
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    warnings.push(message);
    row.insert(
        "warnings".to_string(),
        Value::Array(warnings.into_iter().map(Value::String).collect()),
    );
}

#[cfg(test)]
mod tests;

//! `ez retry`: retomar un subagente bloqueado o fallido
//! (`novedades-subagentes.md` §4).
//!
//! Elige el tipo, el modelo y el esfuerzo (`--auto` con la recomendación de
//! IR-3, `--same` con los del registro o las opciones manuales), lee el
//! encargo guardado y delega el lanzamiento en `ez-herdr`, que deja los
//! registros enlazados y cierra la terminal vieja si corresponde.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ez_core::config::VoiceConfig;
use ez_herdr::driver::AgentDriver;
use ez_herdr::herdr::Herdr;
use ez_herdr::subagents::{AutoRetry, Record, Registry, RetryError, RetryRequest, TaskState};
use ez_scan::usage::planes;
use serde_json::Value;

use crate::cli::{CliError, Parsed};

use super::choice::{self, Filled};
use super::state::Context;
use super::{LocalDelegate, LocalError, MISSING_TERMINAL};

/// `ez retry <nombre> [--agent T] [--model M] [--effort E] [--auto] [--same] [--keep]`.
#[derive(Debug, Clone, Default)]
pub(crate) struct LocalRetry {
    /// El nombre del subagente que se reemplaza.
    pub name: String,
    /// El tipo pedido; vacío = el del registro (o la recomendación con `--auto`).
    pub agent: Option<String>,
    /// El modelo pedido.
    pub model: Option<String>,
    /// El esfuerzo pedido.
    pub effort: Option<String>,
    /// Elegir con la recomendación (excluye lo agotado).
    pub auto: bool,
    /// Mismo tipo y modelo que el registro.
    pub same: bool,
    /// Conservar la terminal vieja.
    pub keep: bool,
}

/// Analiza `ez retry`.
pub(crate) fn build_retry(parsed: &mut Parsed) -> Result<LocalRetry, CliError> {
    let name = parsed.word("Falta el nombre del subagente.")?;
    if name.trim().is_empty() {
        return Err(CliError::Usage(
            "El nombre del subagente no puede estar vacío.".to_string(),
        ));
    }
    if !ez_herdr::herdr::valid_name(&name) {
        return Err(CliError::Usage(format!(
            "El nombre «{name}» no vale: usa minúsculas, números, «-», «_» o «.», hasta 32 caracteres."
        )));
    }
    let command = LocalRetry {
        name: name.trim().to_string(),
        agent: parsed.take("agent"),
        model: parsed.take("model"),
        effort: parsed.take("effort"),
        auto: parsed.flag("auto"),
        same: parsed.flag("same"),
        keep: parsed.flag("keep"),
    };
    for (flag, value) in [
        ("agent", &command.agent),
        ("model", &command.model),
        ("effort", &command.effort),
    ] {
        if value.as_deref().is_some_and(|text| text.trim().is_empty()) {
            return Err(CliError::Usage(format!(
                "El valor de --{flag} no puede estar vacío."
            )));
        }
    }
    if command.auto && command.same {
        return Err(CliError::Usage(
            "--auto y --same no se pueden usar juntos.".to_string(),
        ));
    }
    if command.auto
        && (command.agent.is_some() || command.model.is_some() || command.effort.is_some())
    {
        return Err(CliError::Usage(
            "--auto elige el tipo y el modelo; no lo combines con --agent, --model ni --effort."
                .to_string(),
        ));
    }
    if command.same
        && (command.agent.is_some() || command.model.is_some() || command.effort.is_some())
    {
        return Err(CliError::Usage(
            "--same usa el tipo y el modelo del registro; no lo combines con --agent, --model ni --effort."
                .to_string(),
        ));
    }
    parsed.check_empty()?;
    Ok(command)
}

/// Lo que ya se resolvió para el reintento.
struct Prepared {
    cfg: VoiceConfig,
    fill: Filled,
    brief: String,
    brief_path: Option<String>,
    warnings: Vec<String>,
}

/// Ejecuta `ez retry`.
pub(crate) fn run_retry(
    command: &LocalRetry,
    env: &dyn Fn(&str) -> Option<String>,
    json: bool,
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> i32 {
    let Some(parent) = env("HC_TERMINAL_ID").filter(|value| !value.trim().is_empty()) else {
        let _ = writeln!(err, "{MISSING_TERMINAL}");
        return 3;
    };
    let context = Context::new(env);
    let old = match context.registry.get(&parent, &command.name) {
        Ok(Some(record)) => record,
        Ok(None) => {
            let _ = writeln!(
                err,
                "No hay ninguna tarea de EzAgent llamada «{}» bajo este padre.",
                command.name
            );
            return 1;
        }
        Err(error) => {
            let _ = writeln!(err, "{error}");
            return 1;
        }
    };
    if old.state == TaskState::Validated {
        let _ = writeln!(
            err,
            "La tarea «{}» ya quedó validada; no se reintenta.",
            command.name
        );
        return 1;
    }
    let prepared = match prepare(command, &old, &parent, env) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = writeln!(err, "{}", error.message());
            return error.code();
        }
    };
    match execute(&parent, &old, &prepared, command.keep, &context.registry) {
        Ok(outcome) => {
            print_outcome(out, err, &old, outcome, &prepared, command, json);
            0
        }
        Err(error) => {
            let _ = writeln!(err, "{}", error.message());
            error.code()
        }
    }
}

/// Resuelve tipo, modelo, esfuerzo y encargo del reintento.
fn prepare(
    command: &LocalRetry,
    old: &Record,
    parent: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Prepared, LocalError> {
    let cfg = VoiceConfig::load(None).map_err(LocalError::Unavailable)?;
    prepare_with_cfg(command, old, parent, env, cfg)
}

/// Como [`prepare`], con la configuración ya cargada (pruebas).
fn prepare_with_cfg(
    command: &LocalRetry,
    old: &Record,
    parent: &str,
    env: &dyn Fn(&str) -> Option<String>,
    cfg: VoiceConfig,
) -> Result<Prepared, LocalError> {
    let (brief, brief_path, mut warnings) = brief_for(old)?;
    if command.same
        || (command.agent.is_none()
            && command.model.is_none()
            && command.effort.is_none()
            && !command.auto)
    {
        // DECISION: sin `--auto` ni opciones manuales se conservan el tipo, el
        // modelo y el esfuerzo del registro: es el reintento más predecible.
        return Ok(Prepared {
            cfg,
            fill: Filled {
                agent: old.agent.clone(),
                model: old.model.clone(),
                effort: old.effort.clone(),
                chosen: None,
                warnings: Vec::new(),
            },
            brief,
            brief_path,
            warnings,
        });
    }
    let inputs = choice::gather(env, &cfg, parent, &workdir(old));
    let fill = select(command, old, &inputs)?;
    warnings.extend(fill.warnings.iter().cloned());
    Ok(Prepared {
        cfg,
        fill,
        brief,
        brief_path,
        warnings,
    })
}

/// Elige el tipo, el modelo y el esfuerzo del reintento.
fn select(
    command: &LocalRetry,
    old: &Record,
    inputs: &choice::Inputs,
) -> Result<Filled, LocalError> {
    let request = LocalDelegate {
        role: old.role.clone(),
        agent: if command.auto {
            String::new()
        } else {
            command.agent.clone().unwrap_or_else(|| old.agent.clone())
        },
        model: command.model.clone(),
        effort: command.effort.clone(),
        ..LocalDelegate::default()
    };
    choice::fill(&request, inputs)
}

/// El preparado de un reintento automático (sin opciones manuales).
fn prepare_auto(old: &Record, parent: &str) -> Result<Prepared, LocalError> {
    let command = LocalRetry {
        name: old.name.clone(),
        auto: true,
        ..LocalRetry::default()
    };
    prepare(&command, old, parent, &|name: &str| {
        std::env::var(name).ok()
    })
}

/// El encargo guardado: la ruta, o las primeras 200 letras si no hay ruta.
fn brief_for(old: &Record) -> Result<(String, Option<String>, Vec<String>), LocalError> {
    if let Some(path) = old
        .brief_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        match super::read_brief(path) {
            Ok(brief) => return Ok((brief, Some(path.to_string()), Vec::new())),
            Err(error) => {
                let warning = format!(
                    "No pude leer el encargo guardado ({path}): {}. Uso sus primeras letras.",
                    error.message()
                );
                let brief = fallback_brief(old)?;
                return Ok((brief, None, vec![warning]));
            }
        }
    }
    let warning =
        "El registro no guardó el encargo completo; uso sus primeras 200 letras.".to_string();
    Ok((fallback_brief(old)?, None, vec![warning]))
}

/// El `brief_head` del registro viejo.
fn fallback_brief(old: &Record) -> Result<String, LocalError> {
    let head = old.brief_head.trim();
    if head.is_empty() {
        return Err(LocalError::Rejected(format!(
            "El registro de {} no guardó su encargo; no puedo reintentarlo.",
            old.name
        )));
    }
    Ok(head.to_string())
}

/// Lanza el hijo nuevo con lo ya elegido.
fn execute(
    parent: &str,
    old: &Record,
    prepared: &Prepared,
    keep: bool,
    registry: &Registry,
) -> Result<ez_herdr::subagents::RetryOutcome, LocalError> {
    let herdr = Arc::new(Herdr::new(prepared.cfg.clone()));
    let driver = AgentDriver::new(Arc::clone(&herdr), prepared.cfg.clone());
    let launcher = driver.clone();
    let check = |agent: &str, model: Option<&str>, effort: Option<&str>| {
        driver.validate_choice(agent, model, effort)
    };
    let request = RetryRequest {
        parent: parent.to_string(),
        old: old.clone(),
        agent: prepared.fill.agent.clone(),
        model: prepared.fill.model.clone(),
        effort: prepared.fill.effort.clone(),
        brief: prepared.brief.clone(),
        brief_path: prepared.brief_path.clone(),
        // DECISION: la causa sale del último cambio del registro; si no hay,
        // se dice «una novedad» para no inventarla.
        cause: old
            .history
            .last()
            .map(|change| change.detail.clone())
            .unwrap_or_default(),
        keep_terminal: keep,
    };
    ez_herdr::subagents::retry(&herdr, launcher, &check, &request, registry).map_err(map_error)
}

/// El mismo error con el código de `ez retry`.
fn map_error(error: RetryError) -> LocalError {
    match error {
        RetryError::Rejected(message) => LocalError::Rejected(message),
        RetryError::Unavailable(message) => LocalError::Unavailable(message),
    }
}

/// La carpeta de trabajo del registro (o la actual).
fn workdir(old: &Record) -> PathBuf {
    let saved = PathBuf::from(&old.cwd);
    if old.cwd.trim().is_empty() || !saved.is_dir() {
        std::env::current_dir().unwrap_or_default()
    } else {
        saved
    }
}

/// La salida de `ez retry`.
fn print_outcome(
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
    old: &Record,
    outcome: ez_herdr::subagents::RetryOutcome,
    prepared: &Prepared,
    command: &LocalRetry,
    json: bool,
) {
    if json {
        if let Ok(text) = serde_json::to_string(&outcome.row) {
            let _ = writeln!(out, "{text}");
        }
        return;
    }
    if let Some(chosen) = &prepared.fill.chosen {
        let _ = writeln!(out, "{chosen}");
    }
    let _ = writeln!(
        out,
        "{}",
        summary(&outcome.row, old, outcome.closed_old, command.keep)
    );
    for warning in &prepared.warnings {
        let _ = writeln!(err, "Aviso: {warning}");
    }
    for warning in super::warnings(&outcome.row) {
        let _ = writeln!(err, "Aviso: {warning}");
    }
}

/// La línea legible del reintento.
fn summary(
    row: &serde_json::Map<String, Value>,
    old: &Record,
    closed_old: bool,
    keep: bool,
) -> String {
    let mut line = super::summary(row);
    line.push_str(&format!(" Reemplaza a {}.", old.name));
    if closed_old {
        line.push_str(&format!(" Cerré su terminal vieja ({}).", old.terminal));
    } else if keep {
        line.push_str(" Conservé la terminal vieja (--keep).");
    } else if !old.terminal.is_empty() {
        line.push_str(&format!(
            " No cerré la terminal vieja ({}): ya no estaba viva.",
            old.terminal
        ));
    }
    line
}

/// Si la persona permitió el reintento automático (`planes.toml` §4).
pub(crate) fn automatic_retry_enabled(appdata: &Path) -> bool {
    planes::load(&appdata.join("planes.toml"))
        .novedades
        .automatic_retry
}

/// El reintento automático que instala `ez watch`.
pub(crate) struct WatchAutoRetry {
    /// La terminal del padre.
    pub parent: String,
    /// La raíz del registro.
    pub root: PathBuf,
    /// `%APPDATA%\EzAgent`.
    pub appdata: PathBuf,
}

impl AutoRetry for WatchAutoRetry {
    fn auto_retry(&self, record: &Record, _cause: &str) -> Result<Option<String>, String> {
        if !automatic_retry_enabled(&self.appdata) {
            return Ok(None);
        }
        let registry = Registry::at(&self.root);
        let old = registry
            .get(&record.parent, &record.name)
            .map_err(|error| error.message().to_string())?
            .ok_or_else(|| format!("no hay registro de {} para reintentar", record.name))?;
        if old.state != TaskState::Blocked {
            return Ok(None);
        }
        let prepared = prepare_auto(&old, &self.parent).map_err(map_message)?;
        let outcome =
            execute(&self.parent, &old, &prepared, false, &registry).map_err(map_message)?;
        Ok(Some(
            outcome
                .row
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(&record.name)
                .to_string(),
        ))
    }
}

/// El texto de un error local.
fn map_message(error: LocalError) -> String {
    error.message().to_string()
}

#[cfg(test)]
mod tests;

//! Los comandos del estado de los subagentes en el modo local de Herdr:
//! `ez report`, `ez validate` y `ez tasks` (§13 del diseño).
//!
//! Aquí vive el análisis de las opciones y lo que hace cada comando. El
//! vigilante (`ez watch`) está en `local::watch`.

use std::io::Write;
use std::path::{Path, PathBuf};

use ez_core::config::VoiceConfig;
use ez_herdr::subagents::{Record, Registry, TaskState, Validation};
use serde_json::Value;

use crate::cli::{CliError, Parsed};

use super::MISSING_TERMINAL;

/// Lo que los comandos locales necesitan del entorno.
pub(crate) struct Context {
    /// El registro de subagentes.
    pub registry: Registry,
    /// La raíz del registro.
    pub root: PathBuf,
    /// La ruta de `hc`.
    pub hc: String,
    /// La terminal de quien ejecuta el comando.
    pub terminal: Option<String>,
}

impl Context {
    /// El contexto según el entorno (el inyectado en las pruebas).
    pub fn new(env: &dyn Fn(&str) -> Option<String>) -> Self {
        Self::with_hc(env, None)
    }

    /// El contexto con la ruta de `hc` ya resuelta.
    ///
    /// El vigilante la trae en `--hc`: con ella no hace falta leer la
    /// configuración real de la persona (`%APPDATA%\EzAgent\voice.json`).
    pub fn with_hc(env: &dyn Fn(&str) -> Option<String>, hc: Option<&str>) -> Self {
        let root = registry_root(env);
        let hc = hc_path(env, hc);
        let terminal = env("HC_TERMINAL_ID")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        Self {
            registry: Registry::at(&root),
            root,
            hc,
            terminal,
        }
    }
}

/// La carpeta raíz del registro (`EZ_SUBAGENTS_DIR` o `%LOCALAPPDATA%`).
fn registry_root(env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    if let Some(folder) = env("EZ_SUBAGENTS_DIR")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return PathBuf::from(folder);
    }
    // DECISION: la misma raíz que `Registry::from_env`, pero con el entorno
    // inyectado: las pruebas apuntan a una carpeta temporal.
    let base = env("LOCALAPPDATA")
        .or_else(|| env("HOME"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("EzAgent").join("subagentes")
}

/// La ruta de `hc`: la dada, la del entorno o la de la configuración.
///
/// DECISION: con la ruta ya dada (el `--hc` del vigilante) no se lee la
/// configuración real de la persona: el vigilante corre en una terminal de `hc`
/// sin entorno y leería su `voice.json` sin necesitarlo.
fn hc_path(env: &dyn Fn(&str) -> Option<String>, given: Option<&str>) -> String {
    hc_path_with(env, given, &|| VoiceConfig::load(None).ok())
}

/// Lo mismo, pero la configuración se carga por la función dada (así las
/// pruebas cuentan cuántas veces se lee).
pub(super) fn hc_path_with(
    env: &dyn Fn(&str) -> Option<String>,
    given: Option<&str>,
    load_config: &dyn Fn() -> Option<VoiceConfig>,
) -> String {
    if let Some(path) = given.map(str::trim).filter(|path| !path.is_empty()) {
        return path.to_string();
    }
    if let Some(path) = env("EZ_HC_PATH")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return path;
    }
    // DECISION: `EZ_HC_PATH` manda sobre `voice.json` (pruebas y máquinas
    // donde Herdr vive en otro sitio).
    let from_config = load_config()
        .and_then(|config| {
            config
                .get("hc_path")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    if from_config.trim().is_empty() {
        "hc".to_string()
    } else {
        from_config
    }
}

/// `ez report --state terminado|fallido`.
pub(crate) struct ReportCommand {
    /// El estado que declara el subagente.
    pub state: TaskState,
    /// El resumen (una línea).
    pub summary: Option<String>,
    /// La ruta del informe.
    pub file: Option<String>,
}

/// `ez validate <nombre> --pass|--fail`.
pub(crate) struct ValidateCommand {
    /// El nombre de la tarea.
    pub name: String,
    /// Si la validación pasó.
    pub pass: bool,
    /// El resumen de la validación.
    pub summary: Option<String>,
    /// La evidencia.
    pub file: Option<String>,
}

/// `ez tasks`.
pub(crate) struct TasksCommand {
    /// El padre; por omisión, la terminal de quien ejecuta el comando.
    pub parent: Option<String>,
}

/// Una orden local del estado de los subagentes.
pub(crate) enum LocalState {
    /// `ez report`.
    Report(ReportCommand),
    /// `ez validate`.
    Validate(ValidateCommand),
    /// `ez tasks`.
    Tasks(TasksCommand),
    /// `ez watch`.
    Watch(super::watch::WatchCommand),
}

/// Analiza `ez report` en el modo local.
pub(crate) fn build_report(parsed: &mut Parsed, cwd: &Path) -> Result<LocalState, CliError> {
    let state = parsed.required("state")?;
    let state = match state.as_str() {
        "terminado" => TaskState::Done,
        "fallido" => TaskState::Failed,
        other => {
            return Err(CliError::Usage(format!(
                "--state debe ser terminado o fallido (no «{other}»)."
            )))
        }
    };
    let summary = parsed.take("summary");
    let file = optional_file(parsed, cwd)?;
    parsed.check_empty()?;
    Ok(LocalState::Report(ReportCommand {
        state,
        summary,
        file,
    }))
}

/// Analiza `ez validate`.
pub(crate) fn build_validate(parsed: &mut Parsed, cwd: &Path) -> Result<LocalState, CliError> {
    let name = parsed.word("Falta el nombre de la tarea.")?;
    if name.trim().is_empty() {
        return Err(CliError::Usage(
            "El nombre de la tarea no puede estar vacío.".to_string(),
        ));
    }
    let pass = parsed.flag("pass");
    let fail = parsed.flag("fail");
    if pass == fail {
        return Err(CliError::Usage(
            "Hay que indicar --pass o --fail.".to_string(),
        ));
    }
    let summary = parsed.take("summary");
    let file = optional_file(parsed, cwd)?;
    parsed.check_empty()?;
    Ok(LocalState::Validate(ValidateCommand {
        name: name.trim().to_string(),
        pass,
        summary,
        file,
    }))
}

/// Analiza `ez tasks`.
pub(crate) fn build_tasks(parsed: &mut Parsed, _cwd: &Path) -> Result<LocalState, CliError> {
    let parent = parsed.take("parent");
    parsed.check_empty()?;
    Ok(LocalState::Tasks(TasksCommand { parent }))
}

/// `--file`, si viene: tiene que existir.
fn optional_file(parsed: &mut Parsed, cwd: &Path) -> Result<Option<String>, CliError> {
    match parsed.take("file") {
        Some(value) => {
            let path = cwd.join(&value);
            if !path.exists() {
                return Err(CliError::MissingFile(path));
            }
            Ok(Some(path.to_string_lossy().into_owned()))
        }
        None => Ok(None),
    }
}

/// Ejecuta una orden local del estado.
pub(crate) fn run_state(
    state: &LocalState,
    env: &dyn Fn(&str) -> Option<String>,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    match state {
        LocalState::Report(command) => {
            let context = Context::new(env);
            run_report(command, &context, json, out, err)
        }
        LocalState::Validate(command) => {
            let context = Context::new(env);
            run_validate(command, &context, json, out, err)
        }
        LocalState::Tasks(command) => {
            let context = Context::new(env);
            run_tasks(command, &context, json, out, err)
        }
        // DECISION: el vigilante llega con la ruta de `hc` (`--hc`): con ella
        // no se lee la configuración real de la persona.
        LocalState::Watch(command) => {
            let context = Context::with_hc(env, command.hc.as_deref());
            super::watch::run_watch(command, &context, env, json, out, err)
        }
    }
}

/// `ez report`: el subagente declara cómo terminó su tarea.
fn run_report(
    command: &ReportCommand,
    context: &Context,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let Some(terminal) = context.terminal.clone() else {
        let _ = writeln!(err, "{MISSING_TERMINAL}");
        return 3;
    };
    let record = match context.registry.find_by_terminal(&terminal) {
        Ok(Some(record)) => record,
        Ok(None) => {
            let _ = writeln!(err, "Esta terminal no tiene una tarea de EzAgent.");
            return 1;
        }
        Err(error) => {
            let _ = writeln!(err, "{error}");
            return 1;
        }
    };
    let summary = command
        .summary
        .clone()
        .filter(|text| !text.trim().is_empty());
    let detail = report_detail(command.state, summary.as_deref());
    let file = command.file.clone();
    let changed = context.registry.transition(
        &record.parent,
        &record.name,
        command.state,
        &terminal,
        &detail,
        |record| {
            if let Some(text) = &summary {
                record.summary = Some(text.clone());
            }
            record.report = file.clone();
        },
    );
    match changed {
        Ok(record) => {
            super::watch::start_watcher(context, &record.parent);
            if json {
                print_json(out, &record);
            } else {
                let _ = writeln!(
                    out,
                    "Estado registrado: {} ({}).",
                    record.state.as_str(),
                    record.name
                );
            }
            0
        }
        Err(error) => {
            let _ = writeln!(err, "{error}");
            1
        }
    }
}

/// El motivo que queda en el historial.
fn report_detail(state: TaskState, summary: Option<&str>) -> String {
    match (state, summary) {
        (TaskState::Done, Some(text)) => format!("el subagente terminó: {text}"),
        (TaskState::Done, None) => "el subagente terminó".to_string(),
        (TaskState::Failed, Some(text)) => format!("el subagente falló: {text}"),
        (TaskState::Failed, None) => "el subagente falló".to_string(),
        _ => "el subagente cambió de estado".to_string(),
    }
}

/// `ez validate`: el padre (o un hermano) dice si la tarea pasó.
fn run_validate(
    command: &ValidateCommand,
    context: &Context,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let Some(terminal) = context.terminal.clone() else {
        let _ = writeln!(err, "{MISSING_TERMINAL}");
        return 3;
    };
    let Some(record) = find_by_name(context, &command.name) else {
        let _ = writeln!(
            err,
            "No hay ninguna tarea de EzAgent llamada «{}».",
            command.name
        );
        return 1;
    };
    if record.parent != terminal && !super::watch::is_sibling(context, &terminal, &record.parent) {
        let _ = writeln!(
            err,
            "Solo el padre o un hermano puede validar «{}».",
            command.name
        );
        return 1;
    }
    if record.state != TaskState::Done {
        let _ = writeln!(
            err,
            "La tarea «{}» está «{}»; solo se validan las terminadas.",
            command.name,
            record.state.as_str()
        );
        return 1;
    }
    let to = if command.pass {
        TaskState::Validated
    } else {
        TaskState::Failed
    };
    let summary = command.summary.clone().unwrap_or_default();
    let detail = if command.pass {
        format!("validado por {terminal}")
    } else {
        format!("no pasó la validación de {terminal}: {summary}")
    };
    let file = command.file.clone();
    let at_ms = ez_herdr::subagents::epoch_ms();
    let changed = context.registry.transition(
        &record.parent,
        &record.name,
        to,
        &terminal,
        &detail,
        |record| {
            record.validation = Some(Validation {
                by: terminal.clone(),
                verdict: command.pass,
                summary: summary.clone(),
                file: file.clone(),
                at_ms,
            });
        },
    );
    match changed {
        Ok(record) => {
            if json {
                print_json(out, &record);
            } else {
                let _ = writeln!(
                    out,
                    "Validación registrada: {} ({}).",
                    record.state.as_str(),
                    record.name
                );
            }
            0
        }
        Err(error) => {
            let _ = writeln!(err, "{error}");
            1
        }
    }
}

/// `ez tasks`: cómo va cada subagente del padre.
fn run_tasks(
    command: &TasksCommand,
    context: &Context,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let parent = command
        .parent
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| context.terminal.clone());
    let Some(parent) = parent else {
        let _ = writeln!(
            err,
            "Ejecútalo desde la terminal de un agente en Herdr o pasa --parent."
        );
        return 3;
    };
    let outcome = match context.registry.list_lenient(&parent) {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = writeln!(err, "{error}");
            return 1;
        }
    };
    if json {
        print_json(out, &outcome.records);
    } else {
        print_table(out, &outcome.records);
    }
    // Los ilegibles van a stderr: stdout queda con una sola forma (la tabla o
    // el array de registros válidos).
    for invalid in &outcome.invalid {
        let _ = writeln!(
            err,
            "registro ilegible: {}: {}",
            invalid.path.display(),
            invalid.error
        );
    }
    0
}

/// El registro de un nombre, buscando en todos los padres.
fn find_by_name(context: &Context, name: &str) -> Option<Record> {
    let folders = std::fs::read_dir(&context.root).ok()?;
    for folder in folders.flatten() {
        let path = folder.path();
        if !path.is_dir() {
            continue;
        }
        let entries = match std::fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let file = entry.path();
            if file.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            let Ok(record) = serde_json::from_str::<Record>(&text) else {
                continue;
            };
            if record.name == name {
                return Some(record);
            }
        }
    }
    None
}

/// Imprime un valor como JSON.
fn print_json(out: &mut dyn Write, value: &impl serde::Serialize) {
    match serde_json::to_string(value) {
        Ok(text) => {
            let _ = writeln!(out, "{text}");
        }
        Err(error) => {
            let _ = writeln!(out, "{{\"error\":\"{error}\"}}");
        }
    }
}

/// La tabla legible de `ez tasks`.
fn print_table(out: &mut dyn Write, records: &[Record]) {
    let headers = [
        "nombre",
        "tipo · modelo",
        "papel",
        "estado",
        "hace",
        "resumen",
    ];
    let mut rows: Vec<Vec<String>> = Vec::new();
    for record in records {
        rows.push(vec![
            record.name.clone(),
            agent_line(record),
            record.role.clone(),
            record.state.as_str().to_string(),
            age_line(record.since_ms),
            summary_line(record),
        ]);
    }
    let mut widths: Vec<usize> = headers
        .iter()
        .map(|header| header.chars().count())
        .collect();
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }
    let _ = writeln!(out, "{}", join_row(&headers, &widths));
    for row in &rows {
        let _ = writeln!(out, "{}", join_row(row, &widths));
    }
    if rows.is_empty() {
        let _ = writeln!(out, "No hay subagentes.");
    }
}

/// Una fila con las columnas alineadas.
fn join_row(cells: &[impl AsRef<str>], widths: &[usize]) -> String {
    let mut line = String::new();
    for (index, cell) in cells.iter().enumerate() {
        let width = widths.get(index).copied().unwrap_or(0);
        let text = cell.as_ref();
        line.push_str(text);
        if index + 1 < cells.len() {
            let pad = width.saturating_sub(text.chars().count()) + 2;
            line.push_str(&" ".repeat(pad));
        }
    }
    line.trim_end().to_string()
}

/// «OpenCode · deepseek-v4.1-flash».
fn agent_line(record: &Record) -> String {
    match record.model.as_deref() {
        Some(model) if !model.is_empty() => format!("{} · {}", record.label, model),
        _ => record.label.clone(),
    }
}

/// «hace 3 min».
fn age_line(since_ms: u64) -> String {
    let now = ez_herdr::subagents::epoch_ms();
    let seconds = now.saturating_sub(since_ms) / 1000;
    if seconds < 60 {
        format!("hace {seconds} s")
    } else if seconds < 3600 {
        format!("hace {} min", seconds / 60)
    } else {
        format!("hace {} h {} min", seconds / 3600, (seconds % 3600) / 60)
    }
}

/// El resumen del subagente (o de su validación).
fn summary_line(record: &Record) -> String {
    let summary = record
        .summary
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .unwrap_or("");
    match &record.validation {
        Some(validation) if !validation.verdict => {
            format!("{summary} (no pasó la validación)")
        }
        Some(_) => summary.to_string(),
        None => summary.to_string(),
    }
}

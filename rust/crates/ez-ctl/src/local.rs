//! El modo local de `ez delegate`: delegar en un subagente desde Herdr.
//!
//! Cuando no hay árbitro (`EZ_URL`/`EZ_TOKEN`), el comando lanza al subagente
//! en la terminal que delega: valida el tipo, el modelo y el esfuerzo contra
//! el catálogo (`AgentDriver::validate_choice`), lo cuelga de su padre y le
//! entrega el encargo (§4–§8 de `docs/designs/subagentes-herdr.md`).
//!
//! Aquí vive también el análisis de `ez delegate`: en una tarea del árbitro el
//! comando es el de siempre, y las opciones del modo local se rechazan.

use std::path::Path;
use std::sync::Arc;

use ez_core::config::VoiceConfig;
use ez_herdr::driver::AgentDriver;
use ez_herdr::herdr::{DelegateError, DelegateRequest, Herdr, Placement};
use serde_json::{json, Map, Value};

use crate::cli::{CliError, Parsed};
use crate::protocol::Request;

/// El texto cuando `ez delegate` corre fuera de una terminal de Herdr.
pub const MISSING_TERMINAL: &str = "Ejecútalo desde la terminal de un agente en Herdr.";

/// El tamaño máximo del encargo (`--brief`), en bytes.
pub const MAX_BRIEF_BYTES: u64 = 200_000;

/// El texto cuando alguien pide el modo local dentro de una tarea del árbitro.
const ARBITRATOR_OPTIONS: &str = "En una tarea del árbitro el tipo sale del equipo aprobado (llega con la integración del árbitro).";

/// Lo que hace `ez delegate`: hablar con el árbitro o delegar en Herdr.
pub enum Delegate {
    /// Una orden para el árbitro (la de siempre).
    Arbiter(Request),
    /// Una delegación en el modo local (Herdr).
    Local(LocalDelegate),
}

/// Las opciones de `ez delegate`, ya analizadas.
#[derive(Debug, Clone, Default)]
pub struct DelegateOptions {
    /// El papel tal como se escribió.
    pub role: String,
    /// El tipo pedido (`opencode`, `qwen`…).
    pub agent: Option<String>,
    /// El modelo pedido, si lo hubo.
    pub model: Option<String>,
    /// El nivel de esfuerzo pedido, si lo hubo.
    pub effort: Option<String>,
    /// La ruta del archivo con el encargo.
    pub brief: String,
    /// El nombre pedido; por omisión, el de §6.
    pub name: Option<String>,
    /// Dónde se ve el subagente en Herdr.
    pub show: Placement,
    /// La carpeta de trabajo; por omisión, la actual.
    pub cwd: Option<String>,
    /// Si `hc` avisa al padre de lo que hace el subagente.
    pub notify: bool,
    /// Si el subagente sobrevive al cierre del padre.
    pub outlive_parent: bool,
    /// El pedido trae opciones que solo valen en el modo local.
    pub local_options: bool,
}

/// Una delegación en el modo local: todo lo que el comando ya validó.
#[derive(Debug, Clone, Default)]
pub struct LocalDelegate {
    /// El papel tal como se escribió.
    pub role: String,
    /// El tipo pedido (`opencode`, `qwen`…).
    pub agent: String,
    /// El modelo pedido, si lo hubo.
    pub model: Option<String>,
    /// El nivel de esfuerzo pedido, si lo hubo.
    pub effort: Option<String>,
    /// La ruta del archivo con el encargo.
    pub brief_path: String,
    /// El nombre pedido; por omisión, el de §6.
    pub name: Option<String>,
    /// Dónde se ve el subagente en Herdr.
    pub show: Placement,
    /// La carpeta de trabajo; por omisión, la actual.
    pub cwd: Option<String>,
    /// Si `hc` avisa al padre de lo que hace el subagente.
    pub notify: bool,
    /// Si el subagente sobrevive al cierre del padre.
    pub outlive_parent: bool,
}

/// Por qué no se pudo delegar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalError {
    /// Uso incorrecto: el encargo no sirve (código 2).
    Usage(String),
    /// No hay Herdr ni configuración (código 3).
    Unavailable(String),
    /// El catálogo o `hc` lo rechazaron (código 1).
    Rejected(String),
}

impl LocalError {
    /// El texto para la persona.
    pub fn message(&self) -> &str {
        match self {
            Self::Usage(message) | Self::Unavailable(message) | Self::Rejected(message) => message,
        }
    }

    /// El código de salida de `ez delegate`.
    pub fn code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::Unavailable(_) => 3,
            Self::Rejected(_) => 1,
        }
    }
}

impl std::fmt::Display for LocalError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

/// Analiza `ez delegate`: en el árbitro es como hoy; en Herdr, el modo local.
pub(crate) fn build_delegate(
    parsed: &mut Parsed,
    cwd: &Path,
    arbiter: bool,
    partial_env: bool,
) -> Result<Delegate, CliError> {
    let options = delegate_options(parsed, cwd, !arbiter)?;
    if arbiter {
        if options.local_options {
            return Err(CliError::Usage(ARBITRATOR_OPTIONS.to_string()));
        }
        let request = parsed.finish_request(
            "agent.delegate",
            json!({ "role": options.role, "brief": options.brief }),
        )?;
        return Ok(Delegate::Arbiter(request));
    }
    // DECISION: con el entorno del árbitro a medias no se delega en Herdr: el
    // comando avisa de que falta el entorno (código 3).
    if partial_env {
        return Err(CliError::NoEnv);
    }
    parsed.check_empty()?;
    Ok(Delegate::Local(LocalDelegate {
        role: options.role,
        agent: options.agent.unwrap_or_default(),
        model: options.model,
        effort: options.effort,
        brief_path: options.brief,
        name: options.name,
        show: options.show,
        cwd: options.cwd,
        notify: options.notify,
        outlive_parent: options.outlive_parent,
    }))
}

/// Las opciones que solo valen en el modo local (Herdr).
struct LocalFlags {
    agent: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    name: Option<String>,
    show: Option<String>,
    cwd: Option<String>,
    no_notify: bool,
    outlive_parent: bool,
    hc_notes: bool,
}

impl LocalFlags {
    /// Las toma del análisis: las que llevan valor y las que no.
    fn take(parsed: &mut Parsed) -> Self {
        Self {
            agent: parsed.take("agent"),
            model: parsed.take("model"),
            effort: parsed.take("effort"),
            name: parsed.take("name"),
            show: parsed.take("show"),
            cwd: parsed.take("cwd"),
            no_notify: parsed.flag("no-notify"),
            outlive_parent: parsed.flag("outlive-parent"),
            hc_notes: parsed.flag("hc-notes"),
        }
    }

    /// ¿Vino alguna?
    fn any(&self) -> bool {
        self.agent.is_some()
            || self.model.is_some()
            || self.effort.is_some()
            || self.name.is_some()
            || self.show.is_some()
            || self.cwd.is_some()
            || self.no_notify
            || self.outlive_parent
            || self.hc_notes
    }
}

/// Analiza las opciones de `ez delegate` (para el modo local y las pruebas).
pub fn parse_delegate_options(args: &[String], cwd: &Path) -> Result<DelegateOptions, String> {
    let mut parsed = Parsed::parse(args)?;
    let options = delegate_options(&mut parsed, cwd, true).map_err(describe)?;
    parsed.check_empty().map_err(describe)?;
    Ok(options)
}

/// Las opciones de `delegate`.
///
/// `validate_local` decide si se validan las del modo local (el tipo, el
/// nombre y la ubicación); en una tarea del árbitro esas opciones no existen
/// y se avisa antes de mirarlas.
fn delegate_options(
    parsed: &mut Parsed,
    cwd: &Path,
    validate_local: bool,
) -> Result<DelegateOptions, CliError> {
    let role = parsed.word("Falta el papel del subagente.")?;
    if role.trim().is_empty() {
        return Err(CliError::Usage(
            "El papel del subagente no puede estar vacío.".to_string(),
        ));
    }
    let brief = existing(&parsed.required("brief")?, cwd)?;
    let flags = LocalFlags::take(parsed);
    let placement = match (flags.show.as_deref(), validate_local) {
        (Some(text), true) => match Placement::parse(text) {
            Some(placement) => placement,
            None => {
                return Err(CliError::Usage(format!(
                    "--show debe ser tree, tab, right o down (no «{text}»)."
                )))
            }
        },
        _ => Placement::Tree,
    };
    let mut options = DelegateOptions {
        role: role.trim().to_string(),
        agent: flags.agent.clone(),
        model: flags.model.clone(),
        effort: flags.effort.clone(),
        brief,
        name: flags.name.clone(),
        show: placement,
        cwd: flags.cwd.clone(),
        // DECISION: los avisos al padre los da EzAgent (§13); `hc` solo avisa
        // si alguien lo pide con `--hc-notes`.
        notify: flags.hc_notes,
        outlive_parent: flags.outlive_parent,
        local_options: flags.any(),
    };
    if validate_local {
        validate_local_options(&mut options)?;
    }
    Ok(options)
}

/// Las opciones del modo local: el tipo es obligatorio y el resto se valida.
fn validate_local_options(options: &mut DelegateOptions) -> Result<(), CliError> {
    let agent = options.agent.clone().unwrap_or_default();
    if agent.trim().is_empty() {
        return Err(CliError::Usage(
            "Falta --agent: el tipo del subagente.".to_string(),
        ));
    }
    if let Some(name) = options.name.clone() {
        if !ez_herdr::herdr::valid_name(&name) {
            return Err(CliError::Usage(format!(
                "El nombre «{name}» no vale: usa minúsculas, números, «-», «_» o «.», hasta 32 caracteres."
            )));
        }
    }
    Ok(())
}

/// La ruta del encargo, contra la carpeta actual.
fn existing(value: &str, cwd: &Path) -> Result<String, CliError> {
    let path = cwd.join(value);
    if !path.exists() {
        return Err(CliError::MissingFile(path));
    }
    Ok(path.to_string_lossy().into_owned())
}

/// El texto de un error de uso.
fn describe(error: CliError) -> String {
    match error {
        CliError::Usage(detail) => detail,
        CliError::MissingFile(path) => format!("No existe el archivo: {}", path.display()),
        CliError::NoEnv => crate::cli::MISSING_ENV.to_string(),
    }
}

/// Delega en un subagente desde la terminal `parent`.
pub fn delegate(request: &LocalDelegate, parent: &str) -> Result<Map<String, Value>, LocalError> {
    let brief = read_brief(&request.brief_path)?;
    let cfg = VoiceConfig::load(None).map_err(LocalError::Unavailable)?;
    let herdr = Arc::new(Herdr::new(cfg.clone()));
    let driver = AgentDriver::new(Arc::clone(&herdr), cfg);
    let launcher = driver.clone();
    let check = |agent: &str, model: Option<&str>, effort: Option<&str>| {
        driver.validate_choice(agent, model, effort)
    };
    let delegate_request = DelegateRequest {
        role: request.role.clone(),
        agent: request.agent.clone(),
        model: request.model.clone(),
        effort: request.effort.clone(),
        brief,
        name: request.name.clone(),
        show: request.show,
        cwd: request.cwd.clone(),
        parent: parent.to_string(),
        notify: request.notify,
        outlive_parent: request.outlive_parent,
    };
    herdr
        .delegate(launcher, &check, &delegate_request)
        .map_err(|error| match error {
            DelegateError::Rejected(message) => LocalError::Rejected(message),
            DelegateError::Unavailable(message) => LocalError::Unavailable(message),
        })
}

/// El encargo: un archivo UTF-8 que no pase de 200 000 bytes.
fn read_brief(path: &str) -> Result<String, LocalError> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| LocalError::Usage(format!("No pude leer el encargo: {error}")))?;
    if metadata.len() > MAX_BRIEF_BYTES {
        return Err(LocalError::Usage(format!(
            "El encargo es demasiado grande: máximo {MAX_BRIEF_BYTES} bytes."
        )));
    }
    let bytes = std::fs::read(path)
        .map_err(|error| LocalError::Usage(format!("No pude leer el encargo: {error}")))?;
    String::from_utf8(bytes).map_err(|_| LocalError::Usage("El encargo no es UTF-8.".to_string()))
}

/// La línea legible del resultado (§4).
pub fn summary(row: &Map<String, Value>) -> String {
    let mut details = vec![text(row, "label")];
    let model = text(row, "model");
    if !model.is_empty() {
        details.push(model);
    }
    let effort = text(row, "effort");
    if !effort.is_empty() {
        details.push(effort);
    }
    format!(
        "Subagente {} ({}) lanzado: {}, hijo de {}, {}.",
        text(row, "name"),
        text(row, "terminal"),
        details.join(" · "),
        text(row, "parent"),
        placement_text(&text(row, "placed"))
    )
}

/// Los avisos que dejó la delegación.
pub fn warnings(row: &Map<String, Value>) -> Vec<String> {
    match row.get("warnings") {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Cómo se ve el subagente, en palabras.
fn placement_text(placed: &str) -> &'static str {
    match placed {
        "tab" => "en una pestaña",
        "right" => "en un panel a la derecha",
        "down" => "en un panel debajo",
        _ => "en el árbol",
    }
}

/// Un texto del resultado.
fn text(row: &Map<String, Value>, key: &str) -> String {
    match row.get(key) {
        Some(Value::String(value)) => value.clone(),
        _ => String::new(),
    }
}

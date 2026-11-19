//! Uso y límites de cada agente (IR-2): lectores locales de solo lectura,
//! `planes.toml` y el estado de la ventana.
//!
//! Diseño: `docs/designs/inventario-recursos.md` §5 e
//! `rust-migration/reports/IR-0-r1.md` §7–§9. Nada de esto gasta cuota ni abre
//! un agente: solo lee archivos locales y nunca toca credenciales ni
//! conversaciones. Todo dato que falte queda `desconocido`; no se adivina.

use std::path::PathBuf;

use ez_core::config::VoiceConfig;
use ez_herdr::driver::text_list;
use serde::Serialize;
use serde_json::Value;

use crate::probe::Probe;

pub mod planes;

mod claude;
mod codex;
mod depleted;
mod jsonl;
mod mcode;
mod opencode;
mod pi;

pub use planes::Priority;

/// Ventana por omisión cuando `planes.toml` no declara otra (§5).
pub const DEFAULT_WINDOW_HOURS: f64 = 5.0;

/// Estado de la ventana de uso de un agente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageState {
    /// Por debajo del 80 % de la ventana o del límite declarado.
    Libre,
    /// Desde el 80 % de la ventana o del límite declarado.
    Cerca,
    /// Sin cupo: el 100 % o un aviso de límite vigente.
    Agotado,
    /// No hay manera local de saberlo.
    Desconocido,
}

/// Tokens vistos en la ventana, por tipo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub total: u64,
}

impl TokenUsage {
    /// Suma los cuatro contadores (el total que se compara con lo declarado).
    pub fn new(input: u64, output: u64, cache_read: u64, cache_write: u64) -> Self {
        Self {
            input,
            output,
            cache_read,
            cache_write,
            total: input
                .saturating_add(output)
                .saturating_add(cache_read)
                .saturating_add(cache_write),
        }
    }
}

/// El límite que se compara con el uso: tokens, mensajes o el 100 % observado.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageLimit {
    /// `tokens`, `messages` o `percent`.
    pub unit: String,
    pub value: f64,
}

impl UsageLimit {
    pub(crate) fn tokens(value: f64) -> Self {
        Self {
            unit: "tokens".to_string(),
            value,
        }
    }

    pub(crate) fn messages(value: f64) -> Self {
        Self {
            unit: "messages".to_string(),
            value,
        }
    }

    pub(crate) fn percent(value: f64) -> Self {
        Self {
            unit: "percent".to_string(),
            value,
        }
    }
}

/// De dónde salió cada dato, para poder explicarlo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Sources {
    pub state: String,
    pub tokens: Option<String>,
    pub messages: Option<String>,
    pub cost: Option<String>,
    pub limit: Option<String>,
    pub reset: Option<String>,
    pub plan: Option<String>,
}

/// Lo que se pudo leer de un agente en la ventana.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentUsage {
    pub key: String,
    pub label: String,
    pub installed: bool,
    /// La ventana evaluada, en horas (declarada o 5 h por omisión).
    pub window_hours: f64,
    /// Porcentaje observado de la ventana, cuando el agente lo guarda.
    pub used_percent: Option<f64>,
    /// Horas de la ventana observada (Codex `window_minutes`), si se conoce.
    pub observed_window_hours: Option<f64>,
    pub tokens: Option<TokenUsage>,
    pub messages: Option<u64>,
    pub cost_usd: Option<f64>,
    pub limit: Option<UsageLimit>,
    /// Reinicio de la ventana, en milisegundos desde la época.
    pub resets_ms: Option<u64>,
    /// El reinicio en hora local, legible.
    pub reset_text: Option<String>,
    pub plan: Option<String>,
    /// `primero`, `normal` o `reservar`, si la persona lo declaró.
    pub priority: Option<Priority>,
    pub state: UsageState,
    /// Cuándo se observó el último dato, en milisegundos desde la época.
    pub observed_ms: Option<u64>,
    pub sources: Sources,
    pub warnings: Vec<String>,
}

/// El informe completo de `ez usage`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageReport {
    pub generated_ms: u64,
    pub agents: Vec<AgentUsage>,
    /// Avisos del archivo de planes o del catálogo.
    pub warnings: Vec<String>,
}

/// Las carpetas que leen los lectores.
///
/// Se puede construir a mano en las pruebas: leerlas del entorno real no es
/// necesario para probar los formatos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageDirs {
    pub home: PathBuf,
    pub appdata: PathBuf,
    pub codex: PathBuf,
    pub minimax: PathBuf,
    pub opencode_data: PathBuf,
    pub subagents: PathBuf,
}

impl UsageDirs {
    /// Las carpetas del usuario actual.
    pub fn from_env() -> Self {
        Self::from_lookup(&|name| std::env::var(name).ok())
    }

    /// Las carpetas según un entorno inyectado (el de las pruebas de `ez`).
    ///
    /// DECISION: `OPENCODE_DATA_DIR` no existe en OpenCode; se admite para
    /// apuntar la base a una carpeta falsa sin tocar la del usuario.
    pub fn from_lookup(lookup: &dyn Fn(&str) -> Option<String>) -> Self {
        let var = |name: &str| {
            lookup(name)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        let home = var("USERPROFILE")
            .or_else(|| var("HOME"))
            .unwrap_or_else(|| PathBuf::from("."));
        let appdata = var("APPDATA")
            .map(|base| base.join("EzAgent"))
            .or_else(|| var("XDG_CONFIG_HOME").map(|base| base.join("EzAgent")))
            .unwrap_or_else(|| home.join(".config").join("EzAgent"));
        let codex = var("CODEX_HOME").unwrap_or_else(|| home.join(".codex"));
        let minimax = var("MINIMAX_DATA_DIR").unwrap_or_else(|| home.join(".minimax"));
        let opencode_data = var("OPENCODE_DATA_DIR")
            .unwrap_or_else(|| home.join(".local").join("share").join("opencode"));
        let subagents = var("EZ_SUBAGENTS_DIR").unwrap_or_else(|| {
            var("LOCALAPPDATA")
                .unwrap_or_else(|| home.clone())
                .join("EzAgent")
                .join("subagentes")
        });
        Self {
            home,
            appdata,
            codex,
            minimax,
            opencode_data,
            subagents,
        }
    }
}

/// Milisegundos desde la época.
pub fn now_ms() -> u64 {
    jsonl::now_ms()
}

/// La ruta de `planes.toml` (`%APPDATA%\EzAgent\planes.toml`).
pub fn planes_path(dirs: &UsageDirs) -> PathBuf {
    dirs.appdata.join(planes::FILE_NAME)
}

/// Lee `planes.toml`; sin archivo devuelve valores vacíos.
pub fn load_planes(dirs: &UsageDirs) -> planes::Planes {
    planes::load(&planes_path(dirs))
}

/// Propone `planes.toml` con lo detectado si el archivo no existe.
///
/// Devuelve `true` si lo creó; nunca sobrescribe uno existente.
pub fn propose_planes(dirs: &UsageDirs, report: &UsageReport) -> Result<bool, String> {
    planes::propose(&planes_path(dirs), report)
}

/// Uso y límites de los tipos del catálogo de `voice.json`.
pub fn report(cfg: &VoiceConfig) -> UsageReport {
    let dirs = UsageDirs::from_env();
    let planes = load_planes(&dirs);
    report_with(cfg, &dirs, &planes, now_ms())
}

/// Como [`report`], con las carpetas dadas y el medidor real de instalación.
pub fn report_with(
    cfg: &VoiceConfig,
    dirs: &UsageDirs,
    planes: &planes::Planes,
    now_ms: u64,
) -> UsageReport {
    report_at(cfg, dirs, planes, now_ms, &|program| {
        crate::RealProbe.which(program).is_some()
    })
}

/// Como [`report_with`], con la detección de instalado inyectada (pruebas).
pub fn report_at(
    cfg: &VoiceConfig,
    dirs: &UsageDirs,
    planes: &planes::Planes,
    now_ms: u64,
    installed: &dyn Fn(&str) -> bool,
) -> UsageReport {
    let Some(Value::Object(catalog)) = cfg.get("agents") else {
        return UsageReport {
            generated_ms: now_ms,
            agents: Vec::new(),
            warnings: planes.warnings.clone(),
        };
    };
    let profiles = cfg.get("agent_profiles").and_then(Value::as_object);
    let warnings = planes.warnings.clone();
    let mut seen_programs: Vec<String> = Vec::new();
    let mut agents = Vec::new();
    for (key, command) in catalog {
        let program = text_list(Some(command))
            .into_iter()
            .next()
            .unwrap_or_default();
        if program.is_empty() {
            continue;
        }
        // Dos claves del catálogo pueden apuntar al mismo programa
        // (`mcode` y `minimax`): se informa una sola vez.
        if seen_programs
            .iter()
            .any(|seen| seen.eq_ignore_ascii_case(&program))
        {
            continue;
        }
        seen_programs.push(program.clone());
        agents.push(inspect_agent(
            key, &program, profiles, planes, dirs, now_ms, installed,
        ));
    }
    UsageReport {
        generated_ms: now_ms,
        agents,
        warnings,
    }
}

fn inspect_agent(
    key: &str,
    program: &str,
    profiles: Option<&serde_json::Map<String, Value>>,
    planes: &planes::Planes,
    dirs: &UsageDirs,
    now_ms: u64,
    installed: &dyn Fn(&str) -> bool,
) -> AgentUsage {
    let is_installed = installed(program);
    let plane = planes::for_agent(planes, key);
    let window_hours = plane
        .and_then(|plane| plane.window_hours)
        .unwrap_or(DEFAULT_WINDOW_HOURS);
    let window = Window::new(now_ms, window_hours);
    let mut reading = if is_installed {
        read_agent(key, dirs, &window, now_ms)
    } else {
        Reading::default()
    };
    if !is_installed {
        reading.warnings.push(format!(
            "{} no está instalado; no hay uso que leer.",
            label_of(key, profiles)
        ));
    }
    assemble(
        key,
        label_of(key, profiles),
        is_installed,
        plane,
        window_hours,
        reading,
        depleted::entry(dirs, key, now_ms),
    )
}

struct Declared {
    plan: Option<String>,
    limit: Option<UsageLimit>,
    priority: Option<Priority>,
}

/// Lo declarado gana sobre lo detectado (§2 del diseño).
fn declared_usage(plane: Option<&planes::Plane>, reading: &Reading) -> (Declared, Option<String>) {
    let declared_plan = plane.and_then(|plane| plane.plan.clone());
    let warning = match (declared_plan.as_deref(), reading.plan.as_deref()) {
        (Some(declared), Some(detected)) if !declared.eq_ignore_ascii_case(detected) => Some(
            format!("El plan declarado ({declared}) no coincide con el detectado ({detected})."),
        ),
        _ => None,
    };
    let declared = Declared {
        plan: declared_plan.or_else(|| reading.plan.clone()),
        limit: plane
            .and_then(|plane| plane.limit.clone())
            .or_else(|| reading.limit.clone()),
        priority: plane.and_then(|plane| plane.priority),
    };
    (declared, warning)
}

fn limit_warnings(limit: Option<&UsageLimit>, reading: &Reading) -> Vec<String> {
    let Some(limit) = limit else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    if limit.unit == "messages" && reading.messages.is_none() {
        warnings.push(
            "Hay un límite en mensajes, pero este agente no deja contar mensajes.".to_string(),
        );
    }
    if limit.unit == "tokens" && reading.tokens.is_none() {
        warnings.push(
            "Hay un límite en tokens, pero no pude leer los tokens de la ventana.".to_string(),
        );
    }
    warnings
}

fn assemble(
    key: &str,
    label: String,
    installed: bool,
    plane: Option<&planes::Plane>,
    window_hours: f64,
    mut reading: Reading,
    depleted: Option<String>,
) -> AgentUsage {
    if let Some(source) = depleted {
        reading.depleted = Some(source);
    }
    let (declared, plan_warning) = declared_usage(plane, &reading);
    if let Some(warning) = plan_warning {
        reading.warnings.push(warning);
    }
    let (state, state_source) = state_of(&reading, declared.limit.as_ref());
    let extra = limit_warnings(declared.limit.as_ref(), &reading);
    reading.warnings.extend(extra);
    let mut sources = reading.sources;
    if declared.plan.is_some() && sources.plan.is_none() {
        sources.plan = Some("planes.toml".to_string());
    }
    if plane.and_then(|plane| plane.limit.as_ref()).is_some() {
        sources.limit = Some("planes.toml".to_string());
    }
    sources.state = state_source;
    let plan = declared.plan;
    let limit = declared.limit;
    let priority = declared.priority;
    AgentUsage {
        key: key.to_string(),
        label,
        installed,
        window_hours,
        used_percent: reading.used_percent,
        observed_window_hours: reading.observed_window_hours,
        tokens: reading.tokens,
        messages: reading.messages,
        cost_usd: reading.cost_usd,
        limit,
        resets_ms: reading.resets_ms,
        reset_text: reading.resets_ms.map(jsonl::format_local),
        plan,
        priority,
        state,
        observed_ms: reading.observed_ms,
        sources,
        warnings: reading.warnings,
    }
}

fn read_agent(key: &str, dirs: &UsageDirs, window: &Window, now_ms: u64) -> Reading {
    match key {
        "claude" => claude::read(dirs, window, now_ms),
        "codex" => codex::read(dirs, window, now_ms),
        "mcode" | "minimax" => mcode::read(dirs, window, now_ms),
        "opencode" => opencode::read(dirs, window, now_ms),
        "pi" => pi::read(dirs, window, now_ms),
        // Qwen Code no está instalado y su formato no está verificado (IR-0 §8.7).
        "qwen" => Reading::default(),
        other => {
            let mut reading = Reading::default();
            reading
                .warnings
                .push(format!("No hay lector de uso para el tipo «{other}»."));
            reading
        }
    }
}

fn state_of(reading: &Reading, limit: Option<&UsageLimit>) -> (UsageState, String) {
    if let Some(source) = reading.depleted.as_deref() {
        return (UsageState::Agotado, source.to_string());
    }
    if let Some(stale) = reading.stale.as_deref() {
        return (UsageState::Desconocido, stale.to_string());
    }
    if let Some(limit) = limit {
        if let Some(ratio) = used_ratio(reading, limit) {
            let percent = ratio * 100.0;
            let state = bucket(percent);
            let source = format!("{percent:.1} % de lo declarado en planes.toml");
            return (state, source);
        }
    }
    if let Some(percent) = reading.used_percent {
        let state = bucket(percent);
        let source = format!("{percent:.1} % observado de la ventana");
        return (state, source);
    }
    (
        UsageState::Desconocido,
        "sin porcentaje ni límite declarado que comparar".to_string(),
    )
}

fn used_ratio(reading: &Reading, limit: &UsageLimit) -> Option<f64> {
    if !limit.value.is_finite() || limit.value <= 0.0 {
        return None;
    }
    let used = match limit.unit.as_str() {
        "tokens" => reading.tokens.map(|tokens| tokens.total as f64)?,
        "messages" => reading.messages.map(|messages| messages as f64)?,
        _ => return None,
    };
    Some(used / limit.value)
}

fn bucket(percent: f64) -> UsageState {
    if percent >= 100.0 {
        UsageState::Agotado
    } else if percent >= 80.0 {
        UsageState::Cerca
    } else {
        UsageState::Libre
    }
}

fn label_of(key: &str, profiles: Option<&serde_json::Map<String, Value>>) -> String {
    profiles
        .and_then(|profiles| profiles.get(key))
        .and_then(|profile| profile.get("label"))
        .and_then(Value::as_str)
        .filter(|label| !label.trim().is_empty())
        .unwrap_or(key)
        .to_string()
}

/// La ventana que evalúan los lectores: desde `start_ms` hasta ahora.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Window {
    pub(crate) start_ms: u64,
    pub(crate) hours: f64,
}

impl Window {
    pub(crate) fn new(now_ms: u64, hours: f64) -> Self {
        let hours = if hours.is_finite() && hours > 0.0 {
            hours
        } else {
            DEFAULT_WINDOW_HOURS
        };
        let span = (hours * 3_600_000.0).round().max(0.0) as u64;
        Self {
            start_ms: now_ms.saturating_sub(span),
            hours,
        }
    }

    pub(crate) fn minutes(&self) -> i64 {
        (self.hours * 60.0).round() as i64
    }
}

/// Lo que devuelve un lector antes de mezclarlo con `planes.toml`.
#[derive(Debug, Clone, Default)]
pub(crate) struct Reading {
    pub(crate) tokens: Option<TokenUsage>,
    pub(crate) messages: Option<u64>,
    pub(crate) cost_usd: Option<f64>,
    pub(crate) used_percent: Option<f64>,
    pub(crate) limit: Option<UsageLimit>,
    pub(crate) resets_ms: Option<u64>,
    pub(crate) plan: Option<String>,
    pub(crate) observed_ms: Option<u64>,
    pub(crate) observed_window_hours: Option<f64>,
    /// Fuente del aviso de agotado, si la hay.
    pub(crate) depleted: Option<String>,
    /// Motivo por el que el dato no es actual; invalida el porcentaje.
    pub(crate) stale: Option<String>,
    pub(crate) warnings: Vec<String>,
    pub(crate) sources: Sources,
}

#[cfg(test)]
mod tests;

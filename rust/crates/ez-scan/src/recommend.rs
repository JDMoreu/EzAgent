//! Recomendación de tipo, modelo y esfuerzo (§6 de `inventario-recursos.md`).
//!
//! Los datos salen de `modelos.toml` (`%APPDATA%\EzAgent`, editable; sin
//! archivo valen los de fábrica), del informe de uso (IR-2, que da el estado
//! por agente y por modelo) y de `planes.toml` (prioridad). Nada de esto abre
//! un agente ni gasta cuota: solo lee lo que ya midió [`crate::usage`].

use std::path::PathBuf;

use ez_core::config::VoiceConfig;
use ez_herdr::driver::text_list;
use serde::Serialize;
use serde_json::Value;

use crate::usage::{ModelUsage, Priority, UsageDirs, UsageReport, UsageState};

pub mod modelos;
#[cfg(test)]
mod tests;

pub use modelos::{Catalog, Cost, Level, ModelEntry, FILE_NAME};

/// Tamaño del encargo, que puede subir el nivel mínimo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Size {
    S,
    #[default]
    M,
    L,
}

impl Size {
    /// `s`, `m` o `l` (sin distinguir mayúsculas).
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "s" => Some(Self::S),
            "m" => Some(Self::M),
            "l" => Some(Self::L),
            _ => None,
        }
    }

    /// El nombre corto que se guarda.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::S => "s",
            Self::M => "m",
            Self::L => "l",
        }
    }
}

/// Los recursos justos de §4, ya medidos por el llamador.
///
/// La memoria es la de los subagentes vivos de cada tipo; cero si no hay
/// ninguno (DECISION: un tipo sin procesos corriendo es el que menos usa).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pressure {
    pub tight: bool,
    /// El motivo de la guarda, para el aviso.
    pub reason: Option<String>,
    pub memory_mb: Vec<(String, u64)>,
}

impl Pressure {
    /// La memoria viva de un tipo (0 si no hay dato).
    pub fn memory_of(&self, agent: &str) -> u64 {
        self.memory_mb
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(agent))
            .map(|(_, memory)| *memory)
            .unwrap_or(0)
    }
}

/// Lo que se le pide a la recomendación.
#[derive(Debug, Clone, Default)]
pub struct Request<'a> {
    pub role: &'a str,
    pub size: Size,
    /// Restringe a un tipo (`--agent`).
    pub agent: Option<&'a str>,
    /// Restringe a un modelo (`--model`).
    pub model: Option<&'a str>,
    /// Esfuerzo ya pedido: no se toca.
    pub effort: Option<&'a str>,
    /// El modelo del último `developer` del mismo padre (registro).
    pub last_developer_model: Option<&'a str>,
}

/// La elección con sus razones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Recommendation {
    pub role: String,
    pub size: String,
    pub agent: String,
    pub label: String,
    /// `proveedor/modelo`, listo para `ez delegate`.
    pub model: String,
    /// `None` si el agente no acepta niveles de esfuerzo.
    pub effort: Option<String>,
    pub level: Level,
    pub cost: Cost,
    pub family: String,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
}

/// El nivel mínimo y el esfuerzo que pide un papel (§6).
pub fn required_level(role: &str, size: Size) -> Level {
    match normalized(role).as_str() {
        "planner" | "plan_reviewer" | "qa_validator" => Level::Alto,
        "developer" if size == Size::L => Level::Alto,
        _ => Level::Medio,
    }
}

/// El esfuerzo que pide un papel (§6).
pub fn desired_effort(role: &str) -> &'static str {
    match normalized(role).as_str() {
        "planner" | "plan_reviewer" | "qa_validator" => "high",
        _ => "medium",
    }
}

fn normalized(role: &str) -> String {
    role.trim().to_lowercase()
}

/// Elige tipo, modelo y esfuerzo con las reglas de §6.
pub fn choose(
    cfg: &VoiceConfig,
    report: &UsageReport,
    catalog: &Catalog,
    request: &Request<'_>,
    pressure: &Pressure,
) -> Result<Recommendation, String> {
    let role = normalized(request.role);
    let needed = required_level(&role, request.size);
    let desired = desired_effort(&role);
    let developer_family = request
        .last_developer_model
        .and_then(|model| catalog.family_of(model))
        .map(str::to_string);
    let mut warnings = catalog.warnings.clone();

    let mut candidates = Vec::new();
    let mut same_family = Vec::new();
    // DECISION: no se filtra por el catálogo del perfil: `modelos.toml` manda y
    // el lanzamiento valida el modelo; así valen los modelos descubiertos que
    // la persona declare (por ejemplo, los `*-free` de OpenCode).
    for (index, entry) in catalog.entries.iter().enumerate() {
        let Some(usage) = agent_usage(report, &entry.agent) else {
            continue;
        };
        if !usage.installed || usage.state == UsageState::Agotado {
            continue;
        }
        if !matches_filter(request.agent, &entry.agent)
            || !matches_model(request.model, &entry.model)
        {
            continue;
        }
        if entry.level < needed {
            continue;
        }
        let state = model_state(usage, &entry.model);
        if state == UsageState::Agotado {
            continue;
        }
        let picked = Candidate {
            entry,
            state,
            memory_mb: pressure.memory_of(&entry.agent),
            priority: usage.priority.unwrap_or(Priority::Normal),
            order: index,
        };
        let family_clash =
            role == "qa_validator" && developer_family.as_deref() == Some(entry.family.as_str());
        if family_clash {
            same_family.push(picked);
        } else {
            candidates.push(picked);
        }
    }

    if candidates.is_empty() && !same_family.is_empty() {
        // DECISION: si no hay otra familia instalada, se relaja la regla con
        // aviso en vez de dejar al padre sin qa_validator.
        let family = developer_family.as_deref().unwrap_or("la del developer");
        warnings.push(format!(
            "No hay un modelo de familia distinta a {family}; uso uno de la misma familia."
        ));
        candidates = same_family;
    }
    let Some(picked) = candidates
        .into_iter()
        .min_by_key(|item| item.key(pressure.tight))
    else {
        return Err(no_candidate_text(&role, needed, report));
    };

    let entry = picked.entry;
    let usage = agent_usage(report, &entry.agent)
        .ok_or_else(|| format!("El tipo {} desapareció del informe de uso.", entry.agent))?;
    let model = display_model(cfg, &entry.agent, &entry.model);
    let (effort, effort_reason) = resolve_effort(cfg, entry, request.effort, desired);
    let mut reasons = vec![
        format!(
            "El papel «{role}» pide como mínimo nivel {} y esfuerzo {desired}.",
            needed.name()
        ),
        format!(
            "{} · {model} es de nivel {}, costo {} y familia {}.",
            usage.label,
            entry.level.name(),
            entry.cost.name(),
            entry.family
        ),
    ];
    reasons.push(state_reason(picked.state, &usage.label));
    if let Some(priority) = usage.priority {
        reasons.push(format!(
            "planes.toml le da prioridad {}.",
            priority_name(priority)
        ));
    }
    if role == "qa_validator" {
        if let Some(family) = developer_family.as_deref() {
            reasons.push(format!(
                "La familia {} es distinta a la del último developer del padre ({family}).",
                entry.family
            ));
        }
    }
    if pressure.tight {
        reasons.push(format!(
            "Recursos justos: {} es el que menos memoria usa ({} MB).",
            usage.label, picked.memory_mb
        ));
        if let Some(reason) = pressure.reason.as_deref() {
            warnings.push(format!("Recursos justos: {reason}."));
        }
    }
    if let Some(reason) = effort_reason {
        reasons.push(reason);
    }
    Ok(Recommendation {
        role,
        size: request.size.as_str().to_string(),
        agent: entry.agent.clone(),
        label: usage.label.clone(),
        model,
        effort,
        level: entry.level,
        cost: entry.cost,
        family: entry.family.clone(),
        reasons,
        warnings,
    })
}

/// Un candidato ya filtrado, antes de ordenar.
struct Candidate<'a> {
    entry: &'a ModelEntry,
    state: UsageState,
    memory_mb: u64,
    priority: Priority,
    order: usize,
}

impl Candidate<'_> {
    /// El orden de §6: uso, memoria (si hay recursos justos), nivel, costo,
    /// prioridad y, al final, el orden del catálogo para que sea estable.
    fn key(&self, tight: bool) -> (u8, u64, u8, u8, u8, usize) {
        (
            state_rank(self.state),
            if tight { self.memory_mb } else { 0 },
            self.entry.level.rank(),
            self.entry.cost.rank(),
            priority_rank(self.priority),
            self.order,
        )
    }
}

fn state_rank(state: UsageState) -> u8 {
    match state {
        UsageState::Libre => 0,
        UsageState::Cerca => 1,
        UsageState::Desconocido => 2,
        UsageState::Agotado => 3,
    }
}

fn priority_rank(priority: Priority) -> u8 {
    match priority {
        Priority::Primero => 0,
        Priority::Normal => 1,
        Priority::Reservar => 2,
    }
}

fn priority_name(priority: Priority) -> &'static str {
    match priority {
        Priority::Primero => "primero",
        Priority::Normal => "normal",
        Priority::Reservar => "reservar",
    }
}

fn agent_usage<'a>(report: &'a UsageReport, key: &str) -> Option<&'a crate::usage::AgentUsage> {
    report
        .agents
        .iter()
        .find(|agent| agent.key.eq_ignore_ascii_case(key))
}

/// El estado del modelo: el suyo manda; si no dice nada, el del agente.
fn model_state(usage: &crate::usage::AgentUsage, model: &str) -> UsageState {
    match usage
        .models
        .iter()
        .find(|item| same_model(&item.model, model))
    {
        Some(ModelUsage {
            state: UsageState::Desconocido,
            ..
        })
        | None => usage.state,
        Some(found) => found.state,
    }
}

fn matches_filter(filter: Option<&str>, value: &str) -> bool {
    filter.is_none_or(|wanted| wanted.eq_ignore_ascii_case(value))
}

fn matches_model(filter: Option<&str>, value: &str) -> bool {
    filter.is_none_or(|wanted| same_model(wanted, value))
}

fn same_model(left: &str, right: &str) -> bool {
    model_tail(left).eq_ignore_ascii_case(model_tail(right))
}

/// El nombre sin `proveedor/` ni `#nivel`.
fn model_tail(model: &str) -> &str {
    let model = model.split('#').next().unwrap_or(model);
    model.rsplit('/').next().unwrap_or(model)
}

/// `proveedor/modelo` con el prefijo del perfil cuando el modelo no lo trae.
fn display_model(cfg: &VoiceConfig, agent: &str, model: &str) -> String {
    if model.contains('/') {
        return model.to_string();
    }
    let prefix = profile(cfg, agent)
        .and_then(|profile| profile.get("model_prefix"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    format!("{prefix}{model}")
}

/// Los esfuerzos válidos para un modelo (los suyos o los del perfil).
fn effort_levels(cfg: &VoiceConfig, agent: &str, model: &str) -> Vec<String> {
    let Some(profile) = profile(cfg, agent) else {
        return Vec::new();
    };
    if let Some(by_model) = profile.get("model_efforts").and_then(Value::as_object) {
        let tail = model_tail(model);
        if let Some(levels) = by_model
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(tail))
            .map(|(_, levels)| text_list(Some(levels)))
        {
            if !levels.is_empty() {
                return levels;
            }
        }
    }
    text_list(profile.get("efforts"))
}

/// El esfuerzo pedido o el válido más cercano del modelo.
fn resolve_effort(
    cfg: &VoiceConfig,
    entry: &ModelEntry,
    wanted: Option<&str>,
    desired: &str,
) -> (Option<String>, Option<String>) {
    if let Some(wanted) = wanted.map(str::trim).filter(|text| !text.is_empty()) {
        return (Some(wanted.to_string()), None);
    }
    let available = effort_levels(cfg, &entry.agent, &entry.model);
    if available.is_empty() {
        return (None, None);
    }
    if let Some(exact) = available
        .iter()
        .find(|level| level.eq_ignore_ascii_case(desired))
    {
        return (Some(exact.clone()), None);
    }
    let chosen = nearest_effort(&available, desired);
    let reason = chosen
        .as_ref()
        .map(|chosen| format!("El modelo no tiene el esfuerzo {desired}; uso {chosen}."));
    (chosen, reason)
}

/// El nivel más cercano al pedido; a igual distancia, el más alto.
///
/// DECISION: `medium` no existe en algunos modelos de OpenCode (solo `low`,
/// `high` y `max`); se elige el nivel válido más cercano y, en empate, el más
/// alto para no bajar la calidad sin decirlo.
fn nearest_effort(available: &[String], desired: &str) -> Option<String> {
    let wanted = effort_rank(desired)?;
    available
        .iter()
        .filter_map(|level| effort_rank(level).map(|rank| (rank.abs_diff(wanted), level)))
        .min_by_key(|(distance, level)| (*distance, std::cmp::Reverse(effort_rank(level))))
        .map(|(_, level)| level.clone())
}

fn effort_rank(level: &str) -> Option<u8> {
    match level.trim().to_lowercase().as_str() {
        "off" | "none" => Some(0),
        "minimal" => Some(1),
        "low" => Some(2),
        "default" | "medium" => Some(3),
        "high" => Some(4),
        "xhigh" => Some(5),
        "max" | "ultra" => Some(6),
        _ => None,
    }
}

fn state_reason(state: UsageState, label: &str) -> String {
    match state {
        UsageState::Libre => format!("El uso de {label} está libre."),
        UsageState::Cerca => {
            format!("El uso de {label} está cerca del límite; lo penalicé frente a uno libre.")
        }
        UsageState::Desconocido => {
            format!("No puedo saber el uso de {label}; lo penalicé frente a uno libre.")
        }
        UsageState::Agotado => "El uso está agotado; no debería haber llegado aquí.".to_string(),
    }
}

fn no_candidate_text(role: &str, needed: Level, report: &UsageReport) -> String {
    let installed: Vec<&str> = report
        .agents
        .iter()
        .filter(|agent| agent.installed)
        .map(|agent| agent.key.as_str())
        .collect();
    if installed.is_empty() {
        return format!(
            "No hay ningún agente del catálogo instalado para recomendar un modelo de {role}."
        );
    }
    format!(
        "No encontré un modelo de nivel {} o más para «{role}» entre los agentes instalados ({}); revisa modelos.toml.",
        needed.name(),
        installed.join(", ")
    )
}

/// La ruta de `modelos.toml`.
pub fn path(dirs: &UsageDirs) -> PathBuf {
    dirs.appdata.join(FILE_NAME)
}

/// Lee `modelos.toml`; sin archivo, los valores de fábrica.
pub fn load(dirs: &UsageDirs) -> Catalog {
    Catalog::load(&path(dirs))
}

fn profile<'a>(cfg: &'a VoiceConfig, key: &str) -> Option<&'a serde_json::Map<String, Value>> {
    cfg.get("agent_profiles")?
        .as_object()?
        .get(key)?
        .as_object()
}

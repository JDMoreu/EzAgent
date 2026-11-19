//! `ez init`, `ez resources` y `ez inventory --local` (siempre locales).
//!
//! No hablan con el árbitro ni abren agentes: miden el equipo, leen el catálogo
//! y muestran lo detectado. `ez init` es el único que escribe (`entorno.json`).

use std::io::Write;
use std::path::{Path, PathBuf};

use ez_core::config::VoiceConfig;
use ez_scan::recommend::Size;
use ez_scan::usage::{self, UsageDirs};
use serde_json::json;

use crate::cli::{CliError, Command, Parsed};

/// Los comandos de recursos, inventario y uso.
pub(crate) enum ScanCommand {
    Init,
    Resources,
    Inventory,
    Usage,
    /// `ez recommend <papel> [--size s|m|l]`.
    Recommend {
        role: String,
        size: Size,
    },
}

/// `ez init [--refresh]`.
///
/// DECISION: `ez init` siempre vuelve a medir y a escribir; `--refresh` se
/// acepta para que el comando sea el mismo que usará `ez delegate` (IR-3), que
/// solo escanea cuando el archivo falta o tiene más de 7 días.
pub(crate) fn build_init(parsed: &mut Parsed) -> Result<Command, CliError> {
    let _ = parsed.flag("refresh");
    parsed.check_empty()?;
    Ok(Command::Scan(ScanCommand::Init))
}

/// `ez resources [--json]`.
pub(crate) fn build_resources(parsed: &mut Parsed) -> Result<Command, CliError> {
    parsed.check_empty()?;
    Ok(Command::Scan(ScanCommand::Resources))
}

/// `ez inventory --local [--json]` (el `--local` ya lo consumió `cli`).
pub(crate) fn build_inventory(parsed: &mut Parsed) -> Result<Command, CliError> {
    parsed.check_empty()?;
    Ok(Command::Scan(ScanCommand::Inventory))
}

/// `ez usage [--json]`.
pub(crate) fn build_usage(parsed: &mut Parsed) -> Result<Command, CliError> {
    parsed.check_empty()?;
    Ok(Command::Scan(ScanCommand::Usage))
}

/// `ez recommend <papel> [--size s|m|l] [--json]`.
pub(crate) fn build_recommend(parsed: &mut Parsed) -> Result<Command, CliError> {
    let role = parsed.word("Falta el papel para la recomendación.")?;
    if role.trim().is_empty() {
        return Err(CliError::Usage(
            "El papel no puede estar vacío.".to_string(),
        ));
    }
    let size = match parsed.take("size") {
        Some(text) => Size::parse(&text)
            .ok_or_else(|| CliError::Usage(format!("--size debe ser s, m o l (no «{text}»).")))?,
        None => Size::M,
    };
    parsed.check_empty()?;
    Ok(Command::Scan(ScanCommand::Recommend {
        role: role.trim().to_string(),
        size,
    }))
}

/// Ejecuta el comando y devuelve el código de salida.
pub(crate) fn run(
    command: &ScanCommand,
    env: &dyn Fn(&str) -> Option<String>,
    cwd: &Path,
    json_output: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    match command {
        ScanCommand::Init => run_init(env, cwd, json_output, out, err),
        ScanCommand::Resources => run_resources(env, cwd, json_output, out, err),
        ScanCommand::Inventory => run_inventory(json_output, out, err),
        ScanCommand::Usage => run_usage(env, json_output, out, err),
        ScanCommand::Recommend { role, size } => {
            run_recommend(env, cwd, role, *size, json_output, out, err)
        }
    }
}

fn run_init(
    env: &dyn Fn(&str) -> Option<String>,
    cwd: &Path,
    json_output: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    match ez_scan::entorno::init(cwd) {
        Ok((path, entorno)) => {
            let planes = propose_planes(env);
            if let Some(problem) = planes.error.as_deref() {
                let _ = writeln!(err, "Aviso: no pude proponer planes.toml: {problem}");
            }
            if json_output {
                print_json(
                    out,
                    &json!({
                        "ok": true,
                        "path": path,
                        "entorno": entorno,
                        "planes": {"path": planes.path, "created": planes.created},
                    }),
                );
                return 0;
            }
            let _ = writeln!(out, "Entorno guardado en {}.", path.display());
            write_system(out, &entorno.system);
            write_agents(out, &entorno.agents);
            write_missing(out, &entorno.missing);
            if planes.created {
                let _ = writeln!(
                    out,
                    "Planes: propuesta nueva en {}; revísala y confírmala.",
                    planes.path.display()
                );
            } else if planes.path.exists() {
                let _ = writeln!(
                    out,
                    "Planes: ya existe {}; no lo sobrescribo.",
                    planes.path.display()
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

struct PlanProposal {
    path: PathBuf,
    created: bool,
    error: Option<String>,
}

/// Propone `planes.toml` con lo detectado; nunca sobrescribe uno existente.
fn propose_planes(env: &dyn Fn(&str) -> Option<String>) -> PlanProposal {
    let dirs = UsageDirs::from_lookup(env);
    let path = usage::planes_path(&dirs);
    let planes = usage::load_planes(&dirs);
    let report = usage::report_with(&load_config(), &dirs, &planes, usage::now_ms());
    match usage::propose_planes(&dirs, &report) {
        Ok(created) => PlanProposal {
            path,
            created,
            error: None,
        },
        Err(error) => PlanProposal {
            path,
            created: false,
            error: Some(error),
        },
    }
}

fn run_usage(
    env: &dyn Fn(&str) -> Option<String>,
    json_output: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let dirs = UsageDirs::from_lookup(env);
    let planes = usage::load_planes(&dirs);
    let report = usage::report_with(&load_config(), &dirs, &planes, usage::now_ms());
    if json_output {
        print_json(
            out,
            &json!({
                "ok": true,
                "planes": usage::planes_path(&dirs),
                "usage": report,
            }),
        );
        return 0;
    }
    for warning in &report.warnings {
        let _ = writeln!(err, "Aviso: {warning}");
    }
    let path = usage::planes_path(&dirs);
    if path.is_file() {
        let _ = writeln!(out, "Uso y límites (planes.toml: {}):", path.display());
    } else {
        let _ = writeln!(
            out,
            "Uso y límites (sin planes.toml; ventana de 5 h por omisión):"
        );
    }
    for agent in &report.agents {
        write_usage_agent(out, agent);
    }
    0
}

fn run_recommend(
    env: &dyn Fn(&str) -> Option<String>,
    cwd: &Path,
    role: &str,
    size: ez_scan::recommend::Size,
    json_output: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let cfg = load_config();
    let parent = env("HC_TERMINAL_ID")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let inputs = crate::local::choice::gather(env, &cfg, &parent, cwd);
    match crate::local::choice::recommend_for(&inputs, role, size) {
        Ok(recommendation) => {
            for warning in &recommendation.warnings {
                let _ = writeln!(err, "Aviso: {warning}");
            }
            if json_output {
                print_json(out, &json!({"ok": true, "recommendation": recommendation}));
                return 0;
            }
            let effort = recommendation.effort.as_deref().unwrap_or("sin esfuerzo");
            let _ = writeln!(
                out,
                "Recomiendo {} · {} · {} para {} (tamaño {}).",
                recommendation.label,
                recommendation.model,
                effort,
                recommendation.role,
                recommendation.size
            );
            let _ = writeln!(out, "Razones:");
            for reason in &recommendation.reasons {
                let _ = writeln!(out, "  - {reason}");
            }
            0
        }
        Err(error) => {
            let _ = writeln!(err, "{error}");
            1
        }
    }
}

fn run_resources(
    env: &dyn Fn(&str) -> Option<String>,
    cwd: &Path,
    json_output: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let parent = env("HC_TERMINAL_ID")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_default();
    match ez_scan::resources(&parent, cwd) {
        Ok(resources) => {
            if json_output {
                print_json(
                    out,
                    &json!({"ok": true, "parent": parent, "resources": resources}),
                );
                return 0;
            }
            let _ = writeln!(
                out,
                "RAM libre: {} · memoria virtual libre: {} · CPU: {:.1} % · disco libre: {}",
                format_mb(resources.free_ram_mb),
                format_mb(resources.free_commit_mb),
                resources.cpu_percent,
                format_mb(resources.disk_free_mb)
            );
            if parent.is_empty() {
                let _ = writeln!(
                    out,
                    "Subagentes: no estoy dentro de una terminal de Herdr (falta HC_TERMINAL_ID)."
                );
            } else if resources.subagents.is_empty() {
                let _ = writeln!(out, "Subagentes de {parent}: ninguno vivo.");
            } else {
                let _ = writeln!(out, "Subagentes de {parent}:");
                for agent in &resources.subagents {
                    let heavy = agent
                        .heavy_reason
                        .as_deref()
                        .map(|reason| format!(" — pesado: {reason}"))
                        .unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "  {} ({}, {}): {}{}",
                        agent.name,
                        agent.terminal,
                        agent.agent,
                        format_mb(agent.memory_mb),
                        heavy
                    );
                }
            }
            0
        }
        Err(error) => {
            let _ = writeln!(err, "{error}");
            1
        }
    }
}

fn run_inventory(json_output: bool, out: &mut dyn Write, _err: &mut dyn Write) -> i32 {
    let cfg = load_config();
    let agents = ez_scan::agents(&cfg);
    if json_output {
        let missing = ez_scan::entorno::missing_declarations(&agents);
        print_json(
            out,
            &json!({"ok": true, "agents": agents, "missing": missing}),
        );
        return 0;
    }
    write_agents(out, &agents);
    0
}

/// DECISION: un `voice.json` ilegible no impide el inventario; se avisa y se
/// usan los valores de fábrica.
fn load_config() -> VoiceConfig {
    VoiceConfig::load(None).unwrap_or_else(|error| {
        tracing::warn!("No pude leer voice.json: {error}");
        VoiceConfig::new()
    })
}

fn write_system(out: &mut dyn Write, system: &ez_scan::SystemInfo) {
    let os = if system.os_version.is_empty() {
        system.os.clone()
    } else {
        format!("{} {}", system.os, system.os_version)
    };
    let gpus = if system.gpus.is_empty() {
        "sin GPU detectada".to_string()
    } else {
        system
            .gpus
            .iter()
            .map(|gpu| format!("{} ({})", gpu.name, format_mb(gpu.dedicated_vram_mb)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let _ = writeln!(
        out,
        "Sistema: {} · CPU {} ({} físicos, {} lógicos) · RAM {} · disco libre {} · GPU {}",
        os,
        system.cpu_model,
        system.physical_cores,
        system.logical_cores,
        format_mb(system.total_ram_mb),
        format_mb(system.disk_free_mb),
        gpus
    );
    let hc = system.hc_version.as_deref().unwrap_or("desconocida");
    let app = system.herdr_app_version.as_deref().unwrap_or("desconocida");
    let _ = writeln!(out, "Herdr: {hc} · {app}");
}

fn write_agents(out: &mut dyn Write, agents: &[ez_scan::AgentInfo]) {
    let _ = writeln!(out, "Agentes:");
    for agent in agents {
        if !agent.installed {
            let _ = writeln!(out, "  {} ({}): no instalado", agent.label, agent.key);
            continue;
        }
        let version = agent.version.as_deref().unwrap_or("versión desconocida");
        let path = agent.path.as_deref().unwrap_or("");
        let _ = writeln!(
            out,
            "  {} ({}): {} — {}",
            agent.label, agent.key, version, path
        );
        if !agent.models.is_empty() {
            let _ = writeln!(out, "    modelos: {}", agent.models.join(", "));
        }
        if !agent.efforts.is_empty() {
            let _ = writeln!(out, "    esfuerzos: {}", agent.efforts.join(", "));
        }
    }
}

fn write_missing(out: &mut dyn Write, missing: &[String]) {
    if missing.is_empty() {
        let _ = writeln!(out, "Nada que declarar.");
        return;
    }
    let _ = writeln!(out, "Falta declarar:");
    for line in missing {
        let _ = writeln!(out, "  {line}");
    }
}

fn write_usage_agent(out: &mut dyn Write, agent: &ez_scan::usage::AgentUsage) {
    let _ = writeln!(
        out,
        "  {} ({}): {}",
        agent.label,
        agent.key,
        state_text(agent.state)
    );
    write_usage_facts(out, agent);
    write_usage_models(out, &agent.models);
    write_usage_sources(out, agent);
    for warning in &agent.warnings {
        let _ = writeln!(out, "    aviso: {warning}");
    }
}

fn write_usage_facts(out: &mut dyn Write, agent: &ez_scan::usage::AgentUsage) {
    let mut facts = Vec::new();
    if let Some(tokens) = agent.tokens {
        facts.push(format!(
            "{} tokens en la ventana",
            format_tokens(tokens.total)
        ));
    }
    if let Some(messages) = agent.messages {
        facts.push(format!("{messages} mensajes"));
    }
    if let Some(percent) = agent.used_percent {
        facts.push(format!("{percent:.1} % observado"));
    }
    if let Some(cost) = agent.cost_usd {
        facts.push(format!("{cost:.2} USD"));
    }
    facts.push(format!("ventana {:.1} h", agent.window_hours));
    if let Some(hours) = agent.observed_window_hours {
        if (hours - agent.window_hours).abs() > 0.01 {
            facts.push(format!("ventana observada {hours:.1} h"));
        }
    }
    match &agent.limit {
        Some(limit) => facts.push(format!(
            "límite {} {}",
            format_amount(limit.value),
            unit_text(&limit.unit)
        )),
        None => facts.push("límite no declarado".to_string()),
    }
    facts.push(match &agent.reset_text {
        Some(reset) => format!("reinicio {reset}"),
        None => "reinicio desconocido".to_string(),
    });
    if let Some(plan) = &agent.plan {
        facts.push(format!("plan {plan}"));
    }
    if let Some(priority) = agent.priority {
        facts.push(format!(
            "prioridad {}",
            match priority {
                ez_scan::usage::Priority::Primero => "primero",
                ez_scan::usage::Priority::Normal => "normal",
                ez_scan::usage::Priority::Reservar => "reservar",
            }
        ));
    }
    let _ = writeln!(out, "    {}", facts.join(" · "));
}

fn write_usage_models(out: &mut dyn Write, models: &[ez_scan::usage::ModelUsage]) {
    if models.is_empty() {
        return;
    }
    let _ = writeln!(out, "    modelos:");
    for model in models {
        let mut facts = Vec::new();
        if let Some(tokens) = model.tokens {
            facts.push(format!("{} tokens", format_tokens(tokens.total)));
        }
        if let Some(messages) = model.messages {
            facts.push(format!("{messages} mensajes"));
        }
        if let Some(cost) = model.cost_usd {
            facts.push(format!("{cost:.2} USD"));
        }
        facts.push(state_text(model.state).to_string());
        let _ = writeln!(out, "      {}: {}", model.model, facts.join(" · "));
    }
}

fn write_usage_sources(out: &mut dyn Write, agent: &ez_scan::usage::AgentUsage) {
    let mut sources = Vec::new();
    for (name, value) in [
        ("uso", &agent.sources.tokens),
        ("mensajes", &agent.sources.messages),
        ("costo", &agent.sources.cost),
        ("límite", &agent.sources.limit),
        ("reinicio", &agent.sources.reset),
        ("plan", &agent.sources.plan),
    ] {
        if let Some(value) = value {
            sources.push(format!("{name}={value}"));
        }
    }
    sources.push(format!("estado={}", agent.sources.state));
    let _ = writeln!(out, "    fuentes: {}", sources.join(" · "));
}

fn state_text(state: ez_scan::usage::UsageState) -> &'static str {
    match state {
        ez_scan::usage::UsageState::Libre => "libre",
        ez_scan::usage::UsageState::Cerca => "cerca",
        ez_scan::usage::UsageState::Agotado => "agotado",
        ez_scan::usage::UsageState::Desconocido => "desconocido",
    }
}

fn unit_text(unit: &str) -> &'static str {
    match unit {
        "tokens" => "tokens",
        "messages" => "mensajes",
        _ => "% de la ventana",
    }
}

fn format_amount(value: f64) -> String {
    if (value - value.round()).abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1} M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1} K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

fn print_json(out: &mut dyn Write, value: &serde_json::Value) {
    if let Ok(text) = serde_json::to_string(value) {
        let _ = writeln!(out, "{text}");
    }
}

fn format_mb(mb: u64) -> String {
    if mb >= 1_024 {
        format!("{:.1} GB", mb as f64 / 1_024.0)
    } else {
        format!("{mb} MB")
    }
}

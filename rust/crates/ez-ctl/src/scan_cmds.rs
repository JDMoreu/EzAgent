//! `ez init`, `ez resources` y `ez inventory --local` (siempre locales).
//!
//! No hablan con el árbitro ni abren agentes: miden el equipo, leen el catálogo
//! y muestran lo detectado. `ez init` es el único que escribe (`entorno.json`).

use std::io::Write;
use std::path::Path;

use ez_core::config::VoiceConfig;
use serde_json::json;

use crate::cli::{CliError, Command, Parsed};

/// Los comandos de recursos e inventario.
pub(crate) enum ScanCommand {
    Init,
    Resources,
    Inventory,
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
        ScanCommand::Init => run_init(cwd, json_output, out, err),
        ScanCommand::Resources => run_resources(env, cwd, json_output, out, err),
        ScanCommand::Inventory => run_inventory(json_output, out, err),
    }
}

fn run_init(cwd: &Path, json_output: bool, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match ez_scan::entorno::init(cwd) {
        Ok((path, entorno)) => {
            if json_output {
                print_json(out, &json!({"ok": true, "path": path, "entorno": entorno}));
                return 0;
            }
            let _ = writeln!(out, "Entorno guardado en {}.", path.display());
            write_system(out, &entorno.system);
            write_agents(out, &entorno.agents);
            write_missing(out, &entorno.missing);
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
                    let _ = writeln!(
                        out,
                        "  {} ({}, {}): {}",
                        agent.name,
                        agent.terminal,
                        agent.agent,
                        format_mb(agent.memory_mb)
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

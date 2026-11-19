//! Recursos en vivo (§4): RAM, memoria virtual, CPU, disco y subagentes.

use std::path::Path;

use serde::Serialize;

use crate::capacity::Thresholds;
use crate::probe::Probe;

/// Bytes a MB, redondeando hacia abajo.
pub(crate) fn bytes_to_mb(bytes: u64) -> u64 {
    bytes / 1_048_576
}

/// Un subagente vivo y la memoria de su árbol de procesos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubagentUsage {
    pub name: String,
    pub terminal: String,
    pub agent: String,
    pub memory_mb: u64,
    /// Compila, mide o pasa del umbral de memoria (§4).
    pub heavy: bool,
    /// El compilador encontrado o la memoria, si es pesado.
    pub heavy_reason: Option<String>,
}

/// ¿Es pesado? Devuelve el motivo (`rustc` o `2048 MB`) o `None`.
pub fn heavy_reason(
    memory_mb: u64,
    processes: &[String],
    thresholds: &Thresholds,
) -> Option<String> {
    for process in processes {
        let stem = process_stem(process);
        if thresholds
            .heavy_processes
            .iter()
            .any(|wanted| wanted.eq_ignore_ascii_case(&stem))
        {
            return Some(stem);
        }
    }
    (memory_mb >= thresholds.heavy_mb).then(|| format!("{memory_mb} MB"))
}

fn process_stem(name: &str) -> String {
    std::path::Path::new(name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| name.to_lowercase())
}

/// Lo que muestra `ez resources`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Resources {
    pub free_ram_mb: u64,
    pub free_commit_mb: u64,
    pub cpu_percent: f32,
    pub disk_free_mb: u64,
    pub subagents: Vec<SubagentUsage>,
}

/// Mide los recursos con `probe`; `parent` vacío no lista subagentes.
pub fn collect(
    probe: &dyn Probe,
    parent: &str,
    workdir: &Path,
    thresholds: &Thresholds,
) -> Result<Resources, String> {
    Ok(Resources {
        free_ram_mb: probe.free_ram_mb()?,
        free_commit_mb: probe.free_commit_mb()?,
        cpu_percent: probe.cpu_percent()?,
        disk_free_mb: probe.disk_free_mb(workdir).unwrap_or(0),
        subagents: probe.subagents(parent, thresholds)?,
    })
}

#[cfg(test)]
mod tests;

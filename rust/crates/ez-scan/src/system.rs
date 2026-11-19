//! Datos del sistema que guarda `entorno.json` (§3).

use serde::Serialize;

use crate::probe::Probe;

/// Una GPU con su memoria dedicada (DXGI, vía `sysinfo`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub dedicated_vram_mb: u64,
}

/// SO, CPU, RAM, disco, GPU y versiones de Herdr.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemInfo {
    /// Nombre largo del SO («Windows 11 Pro») o el corto si no se pudo.
    pub os: String,
    /// Versión del SO («10.0.26200»), vacía si no se pudo leer.
    pub os_version: String,
    pub cpu_model: String,
    pub physical_cores: usize,
    pub logical_cores: usize,
    pub total_ram_mb: u64,
    /// Espacio libre del disco de la carpeta de trabajo.
    pub disk_free_mb: u64,
    pub gpus: Vec<GpuInfo>,
    /// La línea de `hc --version` (por ejemplo «hc 0.1.21»), si se pudo.
    pub hc_version: Option<String>,
    /// La línea de `herdr-app --version`, si se pudo.
    pub herdr_app_version: Option<String>,
}

/// El sistema real de esta máquina.
pub fn system(workdir: &std::path::Path) -> Result<SystemInfo, String> {
    crate::RealProbe.system(workdir)
}

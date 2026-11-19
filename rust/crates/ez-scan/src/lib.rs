//! Sistema, recursos en vivo e inventario de agentes (IR-1).
//!
//! Diseño: `docs/designs/inventario-recursos.md` §2–§4. Nada de esto gasta
//! cuota: solo lee archivos locales y ejecuta `--version` o listados locales;
//! nunca abre una sesión con un agente. Las mediciones reales van por el trait
//! [`Probe`], que las pruebas sustituyen por un doble.

use std::path::Path;

use ez_core::config::VoiceConfig;

pub mod agents;
pub mod capacity;
pub mod entorno;
pub mod probe;
pub mod recommend;
pub mod resources;
pub mod system;
pub mod usage;

pub use agents::AgentInfo;
pub use capacity::Thresholds;
pub use probe::Probe;
pub use real::RealProbe;
pub use recommend::{Catalog, Level, Pressure, Recommendation, Size};
pub use resources::{Resources, SubagentUsage};
pub use system::{GpuInfo, SystemInfo};
pub use usage::{UsageDirs, UsageReport};

mod real;

/// El sistema donde corre EzAgent, con el disco libre de `workdir`.
pub fn system(workdir: &Path) -> Result<SystemInfo, String> {
    RealProbe.system(workdir)
}

/// Recursos en vivo del equipo y de los subagentes vivos de `parent`.
///
/// `parent` vacío mide solo el equipo (no lista terminales). Los umbrales de
/// «pesado» se leen de `voice.json` (`resources.*`).
pub fn resources(parent: &str, workdir: &Path) -> Result<Resources, String> {
    let thresholds = Thresholds::load(None);
    resources::collect(&RealProbe, parent, workdir, &thresholds)
}

/// Inventario de los tipos del catálogo de `voice.json`; nunca abre un agente.
pub fn agents(cfg: &VoiceConfig) -> Vec<AgentInfo> {
    agents::inventory(&RealProbe, cfg)
}

/// La guarda de `ez delegate` (§4): `Err` con el texto de «Recursos justos».
pub fn check_capacity(parent: &str, thresholds: &Thresholds) -> Result<(), String> {
    capacity::check(&RealProbe, parent, thresholds)
}

/// Uso y límites de cada agente del catálogo (§5); nunca abre un agente.
pub fn usage_report(cfg: &VoiceConfig) -> UsageReport {
    usage::report(cfg)
}

//! Lo que el escaneo le pide al sistema.
//!
//! Es un trait para que las pruebas usen un sistema falso: solo la prueba real
//! acotada de [`crate::system`] mide de verdad.

use std::path::{Path, PathBuf};

use crate::resources::SubagentUsage;
use crate::system::SystemInfo;

/// Medidor inyectable (el real es [`crate::RealProbe`]).
pub trait Probe: Send + Sync {
    /// SO, CPU, RAM, disco de `workdir`, GPU y versiones de Herdr.
    fn system(&self, workdir: &Path) -> Result<SystemInfo, String>;

    /// RAM libre en MB.
    fn free_ram_mb(&self) -> Result<u64, String>;

    /// Memoria virtual (commit) libre en MB.
    fn free_commit_mb(&self) -> Result<u64, String>;

    /// Uso de CPU en porcentaje, con dos muestras.
    fn cpu_percent(&self) -> Result<f32, String>;

    /// Espacio libre en MB del disco de `workdir`.
    fn disk_free_mb(&self, workdir: &Path) -> Option<u64>;

    /// Terminales vivas con `ez.parent=<parent>` y la memoria de su árbol.
    fn subagents(&self, parent: &str) -> Result<Vec<SubagentUsage>, String>;

    /// `shutil.which`: el programa del catálogo, resuelto en el PATH.
    fn which(&self, program: &str) -> Option<PathBuf>;

    /// La primera línea de `<program> --version`, con tope de 10 s.
    fn version(&self, program: &Path) -> Option<String>;

    /// `opencode models` si el programa se puede ejecutar sin shell.
    fn opencode_models(&self, program: &Path) -> Vec<String>;
}

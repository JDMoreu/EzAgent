//! Recursos en vivo (§4): RAM, memoria virtual, CPU, disco y subagentes.

use std::path::Path;

use serde::Serialize;

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
pub fn collect(probe: &dyn Probe, parent: &str, workdir: &Path) -> Result<Resources, String> {
    Ok(Resources {
        free_ram_mb: probe.free_ram_mb()?,
        free_commit_mb: probe.free_commit_mb()?,
        cpu_percent: probe.cpu_percent()?,
        disk_free_mb: probe.disk_free_mb(workdir).unwrap_or(0),
        subagents: probe.subagents(parent)?,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::system::SystemInfo;

    struct Fake {
        ram: u64,
        commit: u64,
        cpu: f32,
        disk: Option<u64>,
        subagents: Vec<SubagentUsage>,
    }

    impl Probe for Fake {
        fn system(&self, _workdir: &Path) -> Result<SystemInfo, String> {
            Err("no se usa".into())
        }
        fn free_ram_mb(&self) -> Result<u64, String> {
            Ok(self.ram)
        }
        fn free_commit_mb(&self) -> Result<u64, String> {
            Ok(self.commit)
        }
        fn cpu_percent(&self) -> Result<f32, String> {
            Ok(self.cpu)
        }
        fn disk_free_mb(&self, _workdir: &Path) -> Option<u64> {
            self.disk
        }
        fn subagents(&self, _parent: &str) -> Result<Vec<SubagentUsage>, String> {
            Ok(self.subagents.clone())
        }
        fn which(&self, _program: &str) -> Option<PathBuf> {
            None
        }
        fn version(&self, _program: &Path) -> Option<String> {
            None
        }
        fn opencode_models(&self, _program: &Path) -> Vec<String> {
            Vec::new()
        }
    }

    fn fake() -> Fake {
        Fake {
            ram: 3_200,
            commit: 8_000,
            cpu: 12.5,
            disk: Some(40_000),
            subagents: vec![SubagentUsage {
                name: "dev1-opencode".into(),
                terminal: "t12".into(),
                agent: "opencode".into(),
                memory_mb: 640,
            }],
        }
    }

    #[test]
    fn recoge_todo_lo_medido() {
        let resources = collect(&fake(), "t6h", Path::new("C:\\ws")).expect("medición");
        assert_eq!(resources.free_ram_mb, 3_200);
        assert_eq!(resources.free_commit_mb, 8_000);
        assert_eq!(resources.cpu_percent, 12.5);
        assert_eq!(resources.disk_free_mb, 40_000);
        assert_eq!(resources.subagents.len(), 1);
        assert_eq!(resources.subagents[0].name, "dev1-opencode");
    }

    #[test]
    fn sin_disco_queda_en_cero() {
        let mut probe = fake();
        probe.disk = None;
        let resources = collect(&probe, "", Path::new("C:\\ws")).expect("medición");
        assert_eq!(resources.disk_free_mb, 0);
    }

    #[test]
    fn los_bytes_se_redondean_hacia_abajo() {
        assert_eq!(bytes_to_mb(1_048_575), 0);
        assert_eq!(bytes_to_mb(2_097_152), 2);
    }
}

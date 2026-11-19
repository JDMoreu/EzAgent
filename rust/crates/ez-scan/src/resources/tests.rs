//! Pruebas de recursos con un sistema falso.

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
    fn subagents(
        &self,
        _parent: &str,
        _thresholds: &Thresholds,
    ) -> Result<Vec<SubagentUsage>, String> {
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
            heavy: false,
            heavy_reason: None,
        }],
    }
}

#[test]
fn recoge_todo_lo_medido() {
    let resources =
        collect(&fake(), "t6h", Path::new("C:\\ws"), &Thresholds::default()).expect("medición");
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
    let resources =
        collect(&probe, "", Path::new("C:\\ws"), &Thresholds::default()).expect("medición");
    assert_eq!(resources.disk_free_mb, 0);
}

#[test]
fn los_bytes_se_redondean_hacia_abajo() {
    assert_eq!(bytes_to_mb(1_048_575), 0);
    assert_eq!(bytes_to_mb(2_097_152), 2);
}

#[test]
fn un_compilador_en_el_arbol_es_el_motivo() {
    let names = vec!["powershell.exe".to_string(), "rustc.exe".to_string()];
    assert_eq!(
        heavy_reason(100, &names, &Thresholds::default()).as_deref(),
        Some("rustc")
    );
}

#[test]
fn sin_compilador_manda_la_memoria() {
    let thresholds = Thresholds::default();
    let names = vec!["opencode.exe".to_string()];
    assert_eq!(heavy_reason(1_499, &names, &thresholds), None);
    assert_eq!(
        heavy_reason(1_500, &names, &thresholds).as_deref(),
        Some("1500 MB")
    );
}

#[test]
fn la_lista_de_compiladores_se_respeta() {
    let config = Thresholds {
        heavy_processes: vec!["cargo".to_string()],
        heavy_mb: 3_000,
        ..Thresholds::default()
    };
    let names = vec!["rustc.exe".to_string()];
    assert_eq!(heavy_reason(2_000, &names, &config), None);
    let names = vec!["cargo.exe".to_string()];
    assert_eq!(heavy_reason(100, &names, &config).as_deref(), Some("cargo"));
}

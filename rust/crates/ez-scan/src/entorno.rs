//! `ez init` y `%APPDATA%\EzAgent\entorno.json` (§3).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ez_core::config::VoiceConfig;
use ez_core::dirs;
use serde::Serialize;

use crate::agents::{self, AgentInfo};
use crate::system::{self, SystemInfo};

/// El archivo que escribe `ez init`.
pub const FILE_NAME: &str = "entorno.json";

/// Lo que `ez delegate` tolera antes de volver a escanear (7 días).
pub const MAX_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1_000;

/// El contenido de `entorno.json`.
#[derive(Debug, Clone, Serialize)]
pub struct Entorno {
    pub version: u32,
    /// Milisegundos desde la época.
    pub generated_ms: u64,
    pub system: SystemInfo,
    pub agents: Vec<AgentInfo>,
    /// Lo que la persona debe declarar (planes y límites de uso).
    pub missing: Vec<String>,
}

/// La ruta de `entorno.json`.
pub fn path() -> PathBuf {
    dirs::app_data_dir().join(FILE_NAME)
}

/// Escanea el sistema y los agentes del catálogo.
///
/// DECISION: un `voice.json` ilegible no impide el escaneo; se avisa y se usan
/// los valores de fábrica (el inventario sirve igual para la primera vez).
pub fn build(workdir: &Path) -> Result<Entorno, String> {
    let cfg = VoiceConfig::load(None).unwrap_or_else(|error| {
        tracing::warn!("No pude leer voice.json: {error}");
        VoiceConfig::new()
    });
    let system = system::system(workdir)?;
    let agents = agents::inventory(&crate::RealProbe, &cfg);
    let missing = missing_declarations(&agents);
    Ok(Entorno {
        version: 1,
        generated_ms: now_ms(),
        system,
        agents,
        missing,
    })
}

/// Escanea y escribe `entorno.json` de forma atómica.
pub fn init(workdir: &Path) -> Result<(PathBuf, Entorno), String> {
    let entorno = build(workdir)?;
    let path = path();
    write_atomic(&path, &entorno)?;
    Ok((path, entorno))
}

/// Guarda el entorno: temporal en la misma carpeta y renombrado atómico.
pub fn write_atomic(path: &Path, entorno: &Entorno) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("No pude crear {}: {error}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(entorno)
        .map_err(|error| format!("No pude serializar el entorno: {error}"))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, format!("{body}\n").as_bytes())
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|error| {
            let _ = std::fs::remove_file(&tmp);
            format!("No pude escribir {}: {error}", path.display())
        })
}

/// Planes y límites que no se pueden detectar.
pub fn missing_declarations(agents: &[AgentInfo]) -> Vec<String> {
    agents
        .iter()
        .map(|agent| {
            if agent.installed {
                format!(
                    "Falta declarar el plan y el límite de uso de {} (planes.toml).",
                    agent.label
                )
            } else {
                format!(
                    "{} no está instalado; instálalo o quítalo del catálogo.",
                    agent.label
                )
            }
        })
        .collect()
}

fn now_ms() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    u64::try_from(millis).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entorno() -> Entorno {
        Entorno {
            version: 1,
            generated_ms: 7,
            system: SystemInfo {
                os: "Windows de prueba".into(),
                os_version: "1.0".into(),
                cpu_model: "CPU de prueba".into(),
                physical_cores: 2,
                logical_cores: 4,
                total_ram_mb: 8_192,
                disk_free_mb: 1_000,
                gpus: Vec::new(),
                hc_version: None,
                herdr_app_version: None,
            },
            agents: Vec::new(),
            missing: Vec::new(),
        }
    }

    #[test]
    fn escribe_de_forma_atomica_y_legible() {
        let root = std::env::temp_dir().join(format!("ez-scan-entorno-{}", std::process::id()));
        let path = root.join("entorno.json");
        write_atomic(&path, &entorno()).expect("escribir");
        let text = std::fs::read_to_string(&path).expect("leer");
        assert!(text.contains("\"generated_ms\": 7"), "{text}");
        assert!(text.ends_with('\n'));
        assert!(!root
            .join(format!(".entorno.json.{}.tmp", std::process::id()))
            .exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn lo_que_falta_declarar_incluye_planes_y_no_instalados() {
        let installed = AgentInfo {
            key: "opencode".into(),
            label: "OpenCode".into(),
            installed: true,
            path: None,
            version: None,
            models: Vec::new(),
            efforts: Vec::new(),
        };
        let missing = missing_declarations(&[installed]);
        assert_eq!(missing.len(), 1);
        assert!(missing[0].contains("OpenCode") && missing[0].contains("planes.toml"));
    }
}

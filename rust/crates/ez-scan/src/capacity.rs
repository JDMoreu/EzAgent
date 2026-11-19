//! La guarda de recursos de `ez delegate` (§4). IR-3 la conecta al comando.

use std::path::Path;

use ez_core::config::int_value;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::probe::Probe;
use crate::resources;

/// Umbrales de `voice.json` (`resources.*`), con los valores de fábrica de §4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Thresholds {
    pub min_free_ram_mb: u64,
    pub min_free_commit_mb: u64,
    pub max_heavy_subagents: usize,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            min_free_ram_mb: 2_500,
            min_free_commit_mb: 4_000,
            max_heavy_subagents: 3,
        }
    }
}

impl Thresholds {
    /// Lee `resources.*` del `voice.json` en `path` (o el del usuario).
    ///
    /// DECISION: se lee el archivo crudo porque `VoiceConfig` solo conserva sus
    /// campos conocidos y `resources` todavía no es uno de ellos; así la
    /// persona puede editar los umbrales sin tocar el núcleo.
    pub fn load(path: Option<&Path>) -> Self {
        let path = path
            .map(Path::to_path_buf)
            .unwrap_or_else(ez_core::config::config_path);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let Ok(Value::Object(data)) = serde_json::from_str::<Value>(&text) else {
            return Self::default();
        };
        match data.get("resources") {
            Some(Value::Object(resources)) => Self::from_object(resources),
            _ => Self::default(),
        }
    }

    /// Los umbrales de un objeto `resources`; lo que falte queda de fábrica.
    fn from_object(resources: &Map<String, Value>) -> Self {
        let base = Self::default();
        let number = |key: &str, fallback: u64| -> u64 {
            let value = resources.get(key).unwrap_or(&Value::Null);
            u64::try_from(int_value(value, fallback as i64)).unwrap_or(0)
        };
        Self {
            min_free_ram_mb: number("min_free_ram_mb", base.min_free_ram_mb),
            min_free_commit_mb: number("min_free_commit_mb", base.min_free_commit_mb),
            max_heavy_subagents: usize::try_from(number(
                "max_heavy_subagents",
                base.max_heavy_subagents as u64,
            ))
            .unwrap_or(0),
        }
    }
}

/// ¿Se puede lanzar otro subagente? `Err` trae el texto para la persona.
pub fn check(probe: &dyn Probe, parent: &str, thresholds: &Thresholds) -> Result<(), String> {
    let workdir = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let resources = resources::collect(probe, parent, &workdir)?;
    if let Some(reason) = refusal(&resources, thresholds) {
        return Err(format!(
            "Recursos justos: {reason}. Espera a que termine alguno o usa --force."
        ));
    }
    Ok(())
}

/// Qué umbral no se cumple, o `None` si todo está bien.
///
/// DECISION: sin forma de saber si un subagente está compilando o midiendo, se
/// cuentan todos los vivos de este padre como pesados (criterio conservador).
fn refusal(resources: &resources::Resources, thresholds: &Thresholds) -> Option<String> {
    let mut reasons = Vec::new();
    if resources.free_ram_mb < thresholds.min_free_ram_mb {
        reasons.push(format!(
            "RAM libre {} MB (mínimo {})",
            resources.free_ram_mb, thresholds.min_free_ram_mb
        ));
    }
    if resources.free_commit_mb < thresholds.min_free_commit_mb {
        reasons.push(format!(
            "memoria virtual libre {} MB (mínimo {})",
            resources.free_commit_mb, thresholds.min_free_commit_mb
        ));
    }
    if resources.subagents.len() >= thresholds.max_heavy_subagents {
        reasons.push(format!(
            "ya hay {} subagentes vivos (máximo {})",
            resources.subagents.len(),
            thresholds.max_heavy_subagents
        ));
    }
    (!reasons.is_empty()).then(|| reasons.join(", "))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::*;
    use crate::system::SystemInfo;

    struct Fake {
        ram: u64,
        commit: u64,
        subagents: usize,
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
            Ok(1.0)
        }
        fn disk_free_mb(&self, _workdir: &Path) -> Option<u64> {
            Some(10_000)
        }
        fn subagents(&self, _parent: &str) -> Result<Vec<crate::resources::SubagentUsage>, String> {
            Ok((0..self.subagents)
                .map(|index| crate::resources::SubagentUsage {
                    name: format!("dev{index}-opencode"),
                    terminal: format!("t{index}"),
                    agent: "opencode".into(),
                    memory_mb: 100,
                })
                .collect())
        }
        fn which(&self, _program: &str) -> Option<std::path::PathBuf> {
            None
        }
        fn version(&self, _program: &Path) -> Option<String> {
            None
        }
        fn opencode_models(&self, _program: &Path) -> Vec<String> {
            Vec::new()
        }
    }

    fn fake(ram: u64, commit: u64, subagents: usize) -> Fake {
        Fake {
            ram,
            commit,
            subagents,
        }
    }

    #[test]
    fn con_recursos_de_sobra_no_rechaza() {
        let thresholds = Thresholds::default();
        assert!(check(&fake(5_000, 9_000, 2), "t6h", &thresholds).is_ok());
    }

    #[test]
    fn rechaza_por_ram_y_lo_dice() {
        let thresholds = Thresholds::default();
        let error = check(&fake(1_200, 9_000, 0), "t6h", &thresholds).expect_err("debe rechazar");
        assert!(
            error.starts_with("Recursos justos: RAM libre 1200 MB (mínimo 2500)."),
            "{error}"
        );
        assert!(error.ends_with("Espera a que termine alguno o usa --force."));
    }

    #[test]
    fn rechaza_por_memoria_virtual_y_por_subagentes() {
        let thresholds = Thresholds::default();
        let error = check(&fake(5_000, 3_900, 3), "t6h", &thresholds).expect_err("debe rechazar");
        assert!(
            error.contains("memoria virtual libre 3900 MB (mínimo 4000)"),
            "{error}"
        );
        assert!(
            error.contains("ya hay 3 subagentes vivos (máximo 3)"),
            "{error}"
        );
    }

    #[test]
    fn los_umbrales_salen_de_voice_json() {
        let path =
            std::env::temp_dir().join(format!("ez-scan-thresholds-{}.json", std::process::id()));
        std::fs::write(
            &path,
            json!({
                "resources": {
                    "min_free_ram_mb": 1000,
                    "min_free_commit_mb": 2000,
                    "max_heavy_subagents": 1
                }
            })
            .to_string(),
        )
        .expect("voice.json temporal");
        let thresholds = Thresholds::load(Some(&path));
        assert_eq!(thresholds.min_free_ram_mb, 1_000);
        assert_eq!(thresholds.min_free_commit_mb, 2_000);
        assert_eq!(thresholds.max_heavy_subagents, 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sin_resources_van_los_valores_de_fabrica() {
        let path = std::env::temp_dir().join(format!("ez-scan-sin-{}.json", std::process::id()));
        std::fs::write(&path, "{}").expect("voice.json temporal");
        assert_eq!(Thresholds::load(Some(&path)), Thresholds::default());
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            Thresholds::load(Some(Path::new("C:\\no-existe-ez-scan\\voice.json"))),
            Thresholds::default()
        );
    }
}

//! La guarda de recursos de `ez delegate` (§4). IR-3 la conecta al comando.

use std::path::Path;

use ez_core::config::int_value;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::probe::Probe;
use crate::resources;

/// Umbrales de `voice.json` (`resources.*`), con los valores de fábrica de §4.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Thresholds {
    pub min_free_ram_mb: u64,
    pub min_free_commit_mb: u64,
    pub max_heavy_subagents: usize,
    /// Nombres de proceso que cuentan como pesados (sin `.exe`).
    pub heavy_processes: Vec<String>,
    /// Memoria del árbol a partir de la cual el hijo es pesado.
    pub heavy_mb: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            min_free_ram_mb: 2_500,
            min_free_commit_mb: 4_000,
            max_heavy_subagents: 3,
            heavy_processes: ["cargo", "rustc", "link", "lld-link", "cl"]
                .iter()
                .map(|name| name.to_string())
                .collect(),
            heavy_mb: 1_500,
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
        let heavy_processes = match resources.get("heavy_processes") {
            Some(value) => string_list(value),
            None => base.heavy_processes.clone(),
        };
        Self {
            min_free_ram_mb: number("min_free_ram_mb", base.min_free_ram_mb),
            min_free_commit_mb: number("min_free_commit_mb", base.min_free_commit_mb),
            max_heavy_subagents: usize::try_from(number(
                "max_heavy_subagents",
                base.max_heavy_subagents as u64,
            ))
            .unwrap_or(0),
            heavy_processes,
            heavy_mb: number("heavy_mb", base.heavy_mb),
        }
    }
}

/// Una lista de textos de `voice.json` (vacía si el tipo no sirve).
fn string_list(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        Value::String(text) if !text.trim().is_empty() => vec![text.trim().to_string()],
        _ => Vec::new(),
    }
}

/// ¿Se puede lanzar otro subagente? `Err` trae el texto para la persona.
///
/// No mide CPU ni disco: la guarda de §4 solo mira memoria y subagentes
/// pesados, y así no paga las dos muestras de CPU.
pub fn check(probe: &dyn Probe, parent: &str, thresholds: &Thresholds) -> Result<(), String> {
    let resources = resources::Resources {
        free_ram_mb: probe.free_ram_mb()?,
        free_commit_mb: probe.free_commit_mb()?,
        cpu_percent: 0.0,
        disk_free_mb: 0,
        subagents: probe.subagents(parent, thresholds)?,
    };
    if let Some(reason) = refusal(&resources, thresholds) {
        return Err(format!(
            "Recursos justos: {reason}. Espera a que termine alguno o usa --force."
        ));
    }
    Ok(())
}

/// Qué umbral no se cumple, o `None` si todo está bien.
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
    let heavy = resources
        .subagents
        .iter()
        .filter(|child| child.heavy)
        .count();
    if heavy >= thresholds.max_heavy_subagents {
        reasons.push(format!(
            "ya hay {heavy} subagentes pesados (máximo {})",
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
    use crate::resources::{heavy_reason, SubagentUsage};
    use crate::system::SystemInfo;

    struct Fake {
        ram: u64,
        commit: u64,
        children: Vec<(u64, Option<&'static str>)>,
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
        fn subagents(
            &self,
            _parent: &str,
            thresholds: &Thresholds,
        ) -> Result<Vec<SubagentUsage>, String> {
            Ok(self
                .children
                .iter()
                .enumerate()
                .map(|(index, (memory_mb, process))| {
                    let names: Vec<String> = process
                        .map(|name| vec![name.to_string()])
                        .unwrap_or_default();
                    let reason = heavy_reason(*memory_mb, &names, thresholds);
                    SubagentUsage {
                        name: format!("dev{index}-opencode"),
                        terminal: format!("t{index}"),
                        agent: "opencode".into(),
                        memory_mb: *memory_mb,
                        heavy: reason.is_some(),
                        heavy_reason: reason,
                    }
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

    fn fake(ram: u64, commit: u64, children: Vec<(u64, Option<&'static str>)>) -> Fake {
        Fake {
            ram,
            commit,
            children,
        }
    }

    #[test]
    fn cuatro_vivos_con_dos_compilando_pasan() {
        let thresholds = Thresholds::default();
        let probe = fake(
            5_000,
            9_000,
            vec![
                (100, Some("rustc")),
                (100, Some("cargo")),
                (100, None),
                (100, None),
            ],
        );
        assert!(check(&probe, "t6h", &thresholds).is_ok());
    }

    #[test]
    fn tres_compilando_rechazan() {
        let thresholds = Thresholds::default();
        let probe = fake(
            5_000,
            9_000,
            vec![
                (100, Some("rustc")),
                (100, Some("cargo")),
                (100, Some("cl")),
                (100, None),
            ],
        );
        let error = check(&probe, "t6h", &thresholds).expect_err("debe rechazar");
        assert_eq!(
            error,
            "Recursos justos: ya hay 3 subagentes pesados (máximo 3). \
             Espera a que termine alguno o usa --force."
        );
    }

    #[test]
    fn un_hijo_de_dos_gb_sin_compilador_es_pesado() {
        let thresholds = Thresholds {
            max_heavy_subagents: 1,
            ..Thresholds::default()
        };
        let probe = fake(5_000, 9_000, vec![(2_048, None)]);
        let error = check(&probe, "t6h", &thresholds).expect_err("debe rechazar");
        assert!(
            error.contains("ya hay 1 subagentes pesados (máximo 1)"),
            "{error}"
        );
    }

    #[test]
    fn rechaza_por_ram_y_lo_dice() {
        let thresholds = Thresholds::default();
        let error =
            check(&fake(1_200, 9_000, Vec::new()), "t6h", &thresholds).expect_err("debe rechazar");
        assert!(
            error.starts_with("Recursos justos: RAM libre 1200 MB (mínimo 2500)."),
            "{error}"
        );
        assert!(error.ends_with("Espera a que termine alguno o usa --force."));
    }

    #[test]
    fn rechaza_por_memoria_virtual() {
        let thresholds = Thresholds::default();
        let error =
            check(&fake(5_000, 3_900, Vec::new()), "t6h", &thresholds).expect_err("debe rechazar");
        assert!(
            error.contains("memoria virtual libre 3900 MB (mínimo 4000)"),
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
                    "max_heavy_subagents": 1,
                    "heavy_mb": 2500,
                    "heavy_processes": ["cargo", "rustc"]
                }
            })
            .to_string(),
        )
        .expect("voice.json temporal");
        let thresholds = Thresholds::load(Some(&path));
        assert_eq!(thresholds.min_free_ram_mb, 1_000);
        assert_eq!(thresholds.min_free_commit_mb, 2_000);
        assert_eq!(thresholds.max_heavy_subagents, 1);
        assert_eq!(thresholds.heavy_mb, 2_500);
        assert_eq!(thresholds.heavy_processes, vec!["cargo", "rustc"]);
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

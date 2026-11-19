//! Los agotamientos que anotó el vigilante en `uso.json` (NV-1).
//!
//! El archivo es una lista en la raíz del registro de subagentes. Una entrada
//! **sin** `modelo` agota al agente entero (compatibilidad); una con `modelo`
//! agota solo ese modelo. Las entradas vencidas no cuentan: `hasta` pasado, o
//! sin `hasta` y más viejas que 24 h si el modelo es `*-free`, o que la
//! ventana del agente en los demás casos (la regla de IR-2b).

use std::path::Path;

use serde::Deserialize;

use super::{jsonl, load_planes, planes, UsageDirs, DEFAULT_WINDOW_HOURS};

/// 24 h para un modelo gratuito (el plan `Free` de IR-2b).
const FREE_LIFE_MS: u64 = 24 * 3_600_000;

/// Una entrada de `uso.json` (mismos campos que escribe NV-1).
#[derive(Debug, Deserialize)]
struct Entry {
    agente: String,
    #[serde(default)]
    modelo: Option<String>,
    #[serde(default)]
    hasta: Option<u64>,
    #[serde(default)]
    visto_ms: Option<u64>,
}

/// Un agotamiento por modelo, vigente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelDepletion {
    pub(crate) model: String,
}

/// Lo que `uso.json` dice de un agente en este momento.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Depletion {
    /// Una entrada sin modelo: agota todo el agente.
    pub(crate) whole_agent: Option<String>,
    /// Las entradas por modelo, vigentes.
    pub(crate) models: Vec<ModelDepletion>,
}

/// Las entradas vigentes de `key` en `uso.json`; vacío si no hay archivo.
///
/// Una entrada sin `hasta` vence como los avisos sin hora de IR-2b: `*-free`
/// dura 24 h y el resto la ventana del agente (`planes.toml` o 5 h), contando
/// desde `visto_ms` o, si falta, la fecha de modificación de `uso.json`.
pub(crate) fn entries(dirs: &UsageDirs, key: &str, now_ms: u64) -> Depletion {
    let path = dirs.subagents.join("uso.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Depletion::default();
    };
    let Ok(entries) = serde_json::from_str::<Vec<Entry>>(&text) else {
        return Depletion::default();
    };
    let modified = modified_ms(&path);
    let window_hours = window_hours(dirs, key);
    let mut depletion = Depletion::default();
    for entry in entries {
        if !belongs(&entry.agente, key) || !active(&entry, now_ms, modified, window_hours) {
            continue;
        }
        let model = entry
            .modelo
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty());
        match model {
            Some(model) => {
                if !depletion
                    .models
                    .iter()
                    .any(|known| same_model(&known.model, model))
                {
                    depletion.models.push(ModelDepletion {
                        model: model.to_string(),
                    });
                }
            }
            // DECISION: una entrada sin modelo mantiene el comportamiento
            // anterior (agente entero) para los archivos viejos.
            None => depletion.whole_agent = Some(source_text(entry.hasta, entry.visto_ms)),
        }
    }
    depletion
}

/// ¿La entrada sigue vigente?
fn active(entry: &Entry, now_ms: u64, modified: Option<u64>, window_hours: f64) -> bool {
    match entry.hasta {
        Some(until) => until > now_ms,
        None => {
            // DECISION: sin `hasta` ni momento de anotación no se puede saber
            // la edad; como IR-2b con un aviso sin `observed_ms`, no cuenta.
            let Some(seen) = entry.visto_ms.or(modified) else {
                return false;
            };
            seen.saturating_add(life_ms(entry.modelo.as_deref(), window_hours)) > now_ms
        }
    }
}

/// Lo que dura una entrada sin `hasta`: 24 h si es gratuita, la ventana si no.
fn life_ms(model: Option<&str>, window_hours: f64) -> u64 {
    if model.is_some_and(is_free_model) {
        return FREE_LIFE_MS;
    }
    let hours = if window_hours.is_finite() && window_hours > 0.0 {
        window_hours
    } else {
        DEFAULT_WINDOW_HOURS
    };
    (hours * 3_600_000.0).round().max(0.0) as u64
}

fn is_free_model(model: &str) -> bool {
    model_tail(model).to_lowercase().ends_with("-free")
}

/// La ventana declarada para el agente, o la de 5 h.
///
/// DECISION: se lee `planes.toml` aquí (es un archivo pequeño) para no tocar
/// `usage.rs` y respetar el alcance de IR-3c; es la misma ventana que ya usa
/// el informe.
fn window_hours(dirs: &UsageDirs, key: &str) -> f64 {
    let planes = load_planes(dirs);
    planes::for_agent(&planes, key)
        .and_then(|plane| plane.window_hours)
        .filter(|hours| hours.is_finite() && *hours > 0.0)
        .unwrap_or(DEFAULT_WINDOW_HOURS)
}

/// La fecha de modificación de `uso.json`, en milisegundos desde epoch.
fn modified_ms(path: &Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let elapsed = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    u64::try_from(elapsed.as_millis()).ok()
}

fn belongs(agent: &str, key: &str) -> bool {
    agent.eq_ignore_ascii_case(key) || (key == "mcode" && agent.eq_ignore_ascii_case("minimax"))
}

fn source_text(hasta: Option<u64>, visto_ms: Option<u64>) -> String {
    match hasta {
        Some(until) => format!(
            "vigilante: agotado hasta {} (uso.json)",
            jsonl::format_local(until)
        ),
        None => {
            let seen = visto_ms
                .map(|seen| format!(", visto {}", jsonl::format_local(seen)))
                .unwrap_or_default();
            format!("vigilante: agotado sin reinicio conocido (uso.json{seen})")
        }
    }
}

/// El nombre sin `proveedor/` ni `#nivel`.
fn model_tail(model: &str) -> &str {
    let model = model.split('#').next().unwrap_or(model);
    model.rsplit('/').next().unwrap_or(model)
}

/// La misma comparación que usa `usage.rs` para los modelos de `uso.json`.
pub(crate) fn same_model(left: &str, right: &str) -> bool {
    model_tail(left).eq_ignore_ascii_case(model_tail(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(root: &std::path::Path) -> UsageDirs {
        UsageDirs {
            home: root.to_path_buf(),
            appdata: root.to_path_buf(),
            codex: root.to_path_buf(),
            minimax: root.to_path_buf(),
            opencode_data: root.to_path_buf(),
            subagents: root.to_path_buf(),
        }
    }

    fn write_usage(root: &std::path::Path, text: &str) {
        std::fs::create_dir_all(root).expect("carpeta");
        std::fs::write(root.join("uso.json"), text).expect("uso.json");
    }

    #[test]
    fn una_entrada_por_modelo_no_agota_el_agente() {
        let root = std::env::temp_dir().join(format!("ez-scan-uso-modelo-{}", std::process::id()));
        write_usage(
            &root,
            r#"[{"agente":"opencode","modelo":"opencode/hy4-flash-free","hasta":null,"visto_ms":100}]"#,
        );
        let depletion = entries(&dirs(&root), "opencode", 200);
        assert!(depletion.whole_agent.is_none());
        assert_eq!(
            depletion.models,
            vec![ModelDepletion {
                model: "opencode/hy4-flash-free".to_string()
            }]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn una_entrada_sin_modelo_agota_el_agente() {
        let root = std::env::temp_dir().join(format!("ez-scan-uso-agente-{}", std::process::id()));
        write_usage(
            &root,
            r#"[{"agente":"opencode","modelo":null,"hasta":null,"visto_ms":100}]"#,
        );
        let depletion = entries(&dirs(&root), "opencode", 200);
        assert!(depletion.whole_agent.is_some());
        assert!(depletion.models.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn una_entrada_vencida_no_cuenta() {
        let root = std::env::temp_dir().join(format!("ez-scan-uso-vencido-{}", std::process::id()));
        write_usage(
            &root,
            r#"[{"agente":"opencode","modelo":"opencode/hy4-flash-free","hasta":200,"visto_ms":100}]"#,
        );
        let dirs = dirs(&root);
        assert_eq!(entries(&dirs, "opencode", 100).models.len(), 1);
        assert_eq!(entries(&dirs, "opencode", 200), Depletion::default());
        assert_eq!(entries(&dirs, "opencode", 300), Depletion::default());
        assert_eq!(entries(&dirs, "claude", 100), Depletion::default());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn el_alias_minimax_cuenta_para_mcode() {
        let root = std::env::temp_dir().join(format!("ez-scan-uso-alias-{}", std::process::id()));
        write_usage(
            &root,
            r#"[{"agente":"minimax","modelo":"minimax-m3.1","hasta":null,"visto_ms":100}]"#,
        );
        assert_eq!(entries(&dirs(&root), "mcode", 200).models.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sin_hasta_y_reciente_cuenta_pero_pasada_la_ventana_no() {
        let root = std::env::temp_dir().join(format!("ez-scan-uso-ventana-{}", std::process::id()));
        let seen = 1_000_000_000u64;
        write_usage(
            &root,
            r#"[{"agente":"opencode","modelo":"opencode-go/deepseek-v4.1-flash","hasta":null,"visto_ms":1000000000}]"#,
        );
        let dirs = dirs(&root);
        assert_eq!(entries(&dirs, "opencode", seen + 3_600_000).models.len(), 1);
        assert!(entries(&dirs, "opencode", seen + 6 * 3_600_000)
            .models
            .is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn un_modelo_gratuito_sin_hasta_dura_24_horas() {
        let root = std::env::temp_dir().join(format!("ez-scan-uso-free-{}", std::process::id()));
        let seen = 1_000_000_000u64;
        write_usage(
            &root,
            r#"[{"agente":"opencode","modelo":"opencode/hy4-flash-free","hasta":null,"visto_ms":1000000000}]"#,
        );
        let dirs = dirs(&root);
        assert_eq!(
            entries(&dirs, "opencode", seen + 23 * 3_600_000)
                .models
                .len(),
            1
        );
        assert!(entries(&dirs, "opencode", seen + 25 * 3_600_000)
            .models
            .is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sin_hasta_ni_visto_usa_la_fecha_del_archivo() {
        let root = std::env::temp_dir().join(format!("ez-scan-uso-mtime-{}", std::process::id()));
        write_usage(
            &root,
            r#"[{"agente":"opencode","modelo":"opencode-go/deepseek-v4.1-flash","hasta":null}]"#,
        );
        let modified = modified_ms(&root.join("uso.json")).expect("fecha del archivo");
        let dirs = dirs(&root);
        assert_eq!(
            entries(&dirs, "opencode", modified + 3_600_000)
                .models
                .len(),
            1
        );
        assert!(entries(&dirs, "opencode", modified + 6 * 3_600_000)
            .models
            .is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn la_ventana_sin_hasta_sale_de_planes_toml() {
        let root = std::env::temp_dir().join(format!("ez-scan-uso-planes-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("carpeta");
        std::fs::write(root.join("planes.toml"), "[opencode]\nventana_horas = 10\n")
            .expect("planes.toml");
        let seen = 1_000_000_000u64;
        write_usage(
            &root,
            r#"[{"agente":"opencode","modelo":"opencode-go/deepseek-v4.1-flash","hasta":null,"visto_ms":1000000000}]"#,
        );
        let dirs = dirs(&root);
        assert_eq!(
            entries(&dirs, "opencode", seen + 6 * 3_600_000)
                .models
                .len(),
            1
        );
        assert!(entries(&dirs, "opencode", seen + 11 * 3_600_000)
            .models
            .is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}

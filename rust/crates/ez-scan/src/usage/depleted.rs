//! Los agotamientos que anotó el vigilante en `uso.json` (NV-1).
//!
//! El archivo es una lista en la raíz del registro de subagentes. Una entrada
//! **sin** `modelo` agota al agente entero (compatibilidad); una con `modelo`
//! agota solo ese modelo. Las entradas vencidas (`hasta` pasado) no cuentan.

use serde::Deserialize;

use super::{jsonl, UsageDirs};

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
pub(crate) fn entries(dirs: &UsageDirs, key: &str, now_ms: u64) -> Depletion {
    let path = dirs.subagents.join("uso.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Depletion::default();
    };
    let Ok(entries) = serde_json::from_str::<Vec<Entry>>(&text) else {
        return Depletion::default();
    };
    let mut depletion = Depletion::default();
    for entry in entries {
        if !belongs(&entry.agente, key) || entry.hasta.is_some_and(|until| until <= now_ms) {
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
}

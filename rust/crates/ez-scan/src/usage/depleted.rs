//! Los agotamientos que anotó el vigilante en `uso.json` (NV-1).
//!
//! El archivo es una lista en la raíz del registro de subagentes; aquí solo se
//! miran el agente, la hora de fin y cuándo se vio.

use serde::Deserialize;

use super::{jsonl, UsageDirs};

/// Una entrada de `uso.json` (mismos campos que escribe NV-1).
#[derive(Debug, Deserialize)]
struct Entry {
    agente: String,
    #[serde(default)]
    hasta: Option<u64>,
    #[serde(default)]
    visto_ms: Option<u64>,
}

/// El aviso vigente para `key`, con su fuente, o `None`.
pub(crate) fn entry(dirs: &UsageDirs, key: &str, now_ms: u64) -> Option<String> {
    let path = dirs.subagents.join("uso.json");
    let text = std::fs::read_to_string(path).ok()?;
    let entries: Vec<Entry> = serde_json::from_str(&text).ok()?;
    let canonical = |name: &str| {
        name.eq_ignore_ascii_case(key) || (key == "mcode" && name.eq_ignore_ascii_case("minimax"))
    };
    let entry = entries
        .into_iter()
        .find(|entry| canonical(&entry.agente) && entry.hasta.is_none_or(|until| until > now_ms))?;
    match entry.hasta {
        Some(until) => Some(format!(
            "vigilante: agotado hasta {} (uso.json)",
            jsonl::format_local(until)
        )),
        None => {
            let seen = entry
                .visto_ms
                .map(|seen| format!(", visto {}", jsonl::format_local(seen)))
                .unwrap_or_default();
            Some(format!(
                "vigilante: agotado sin reinicio conocido (uso.json{seen})"
            ))
        }
    }
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

    #[test]
    fn una_entrada_vencida_no_cuenta() {
        let root = std::env::temp_dir().join(format!("ez-scan-uso-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("carpeta");
        std::fs::write(
            root.join("uso.json"),
            r#"[{"agente":"codex","modelo":null,"hasta":200,"visto_ms":100}]"#,
        )
        .expect("uso.json");
        let dirs = dirs(&root);
        assert!(entry(&dirs, "codex", 100).is_some());
        assert!(entry(&dirs, "codex", 300).is_none());
        assert!(entry(&dirs, "opencode", 100).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn el_alias_minimax_cuenta_para_mcode() {
        let root = std::env::temp_dir().join(format!("ez-scan-uso2-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("carpeta");
        std::fs::write(
            root.join("uso.json"),
            r#"[{"agente":"minimax","hasta":null,"visto_ms":100}]"#,
        )
        .expect("uso.json");
        assert!(entry(&dirs(&root), "mcode", 200).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }
}

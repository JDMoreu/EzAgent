//! Auditoría de aprobaciones sin pantallas (10.15).
//!
//! No se guarda texto de pantalla: solo el tipo de diálogo, la opción elegida,
//! el primer token del comando con su hash, la ruta editada, la dirección y la
//! decisión. La bitácora usa la misma redacción.

use ez_core::pyvalue::head_chars;
use ez_core::redact::redact;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::audit::{AuditEntry, AuditLog};

/// Texto de la opción elegida que se conserva.
pub const MAX_OPTION_CHARS: usize = 60;

/// Lo que el bucle sabe de una aprobación cuando la registra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRecord<'a> {
    /// `exec`, `edit`, `trust` u `other`.
    pub kind: &'a str,
    pub option: Option<&'a str>,
    pub command: Option<&'a str>,
    pub path: Option<&'a str>,
    pub address: &'a str,
    /// `approve`, `deny` o `ask`.
    pub decision: &'a str,
}

/// Primer token del comando, que es lo único que se conserva.
pub fn first_token(command: &str) -> &str {
    command.split_whitespace().next().unwrap_or("")
}

/// SHA-256 del texto en 16 caracteres hexadecimales.
pub fn short_hash(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Datos saneados para la línea `autonomous.approve` de la auditoría.
pub fn approval_detail(record: &ApprovalRecord<'_>) -> Value {
    let mut detail = Map::new();
    detail.insert("kind".into(), json!(record.kind));
    if let Some(option) = record.option {
        detail.insert(
            "option".into(),
            json!(head_chars(&redact(option), MAX_OPTION_CHARS)),
        );
    }
    if let Some(command) = record.command {
        let token = first_token(command);
        if !token.is_empty() {
            detail.insert("command".into(), json!(redact(token)));
            detail.insert("command_sha256".into(), json!(short_hash(token)));
        }
    }
    if let Some(path) = record.path {
        detail.insert("path".into(), json!(redact(path)));
    }
    detail.insert("address".into(), json!(redact(record.address)));
    detail.insert("decision".into(), json!(record.decision));
    Value::Object(detail)
}

/// Copia `value` con todas las cadenas redactadas.
pub fn redact_value(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(redact(text)),
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), redact_value(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Escribe la línea `autonomous.approve` de la auditoría (10.15).
///
/// A3 la llama con cada aprobación o denegación; nunca se guarda la pantalla.
pub fn log_approval(audit: &AuditLog, task_id: &str, record: &ApprovalRecord<'_>) {
    let detail = approval_detail(record);
    let turn_id = format!("autonomous:{task_id}");
    audit.log(&AuditEntry {
        turn_id: &turn_id,
        tool: "autonomous.approve",
        level: 2,
        args: &detail,
        decision: record.decision,
        taint: false,
        result_ok: record.decision == "approve",
        ms: 0.0,
        error_kind: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_type_option_hash_path_and_decision_but_not_the_screen() {
        let detail = approval_detail(&ApprovalRecord {
            kind: "exec",
            option: Some("Yes, allow once"),
            command: Some("git status"),
            path: None,
            address: "h1:abc",
            decision: "approve",
        });
        assert_eq!(detail["kind"], "exec");
        assert_eq!(detail["option"], "Yes, allow once");
        assert_eq!(detail["command"], "git");
        assert_eq!(
            detail["command_sha256"],
            short_hash("git"),
            "hash del primer token"
        );
        assert_eq!(detail["address"], "h1:abc");
        assert_eq!(detail["decision"], "approve");
        assert!(!detail.to_string().contains("status"));
        assert!(detail.get("screen").is_none());
    }

    #[test]
    fn the_chosen_option_is_cut_to_sixty_characters() {
        let long = "Yes, allow once ".repeat(10);
        let detail = approval_detail(&ApprovalRecord {
            kind: "exec",
            option: Some(&long),
            command: None,
            path: None,
            address: "h1:abc",
            decision: "approve",
        });
        let option = detail["option"].as_str().unwrap_or_default();
        assert_eq!(option.chars().count(), MAX_OPTION_CHARS);
        assert!(option.starts_with("Yes, allow once"));
    }

    #[test]
    fn a_fake_key_never_reaches_the_detail() {
        let detail = approval_detail(&ApprovalRecord {
            kind: "edit",
            option: Some("api_key=sk-abc123def456"),
            command: Some("cat sk-abc123def456"),
            path: Some("C:/Users/JD/.ssh/id_rsa"),
            address: "h1:abc",
            decision: "deny",
        });
        let text = detail.to_string();
        assert!(!text.contains("sk-abc123def456"));
        assert!(text.contains("[REDACTED]"));
    }

    #[test]
    fn the_audit_line_never_contains_the_screen_or_a_key() {
        let path = std::env::temp_dir().join(format!(
            "ez-autonomo-audit-{}.jsonl",
            uuid::Uuid::new_v4().simple()
        ));
        let audit = AuditLog::new(&path);
        log_approval(
            &audit,
            "20261116-120000-abcd",
            &ApprovalRecord {
                kind: "exec",
                option: Some("Yes, allow once"),
                command: Some("cat .env"),
                path: None,
                address: "h1:abc",
                decision: "approve",
            },
        );
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(text.contains("\"tool\": \"autonomous.approve\""));
        assert!(text.contains("autonomous:20261116-120000-abcd"));
        assert!(!text.contains("sk-"));
        assert!(!text.contains("screen"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn redacts_nested_values() {
        let value = json!({"a": "sk-abc123def456", "b": ["token=xyz"], "n": 1});
        let text = redact_value(&value).to_string();
        assert!(!text.contains("sk-abc123def456"));
        assert!(!text.contains("xyz"));
        assert!(text.contains("\"n\":1"));
    }
}

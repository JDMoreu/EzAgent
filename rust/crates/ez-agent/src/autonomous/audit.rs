//! Auditoría de aprobaciones sin pantallas (10.15).
//!
//! No se guarda texto de pantalla: solo el tipo de diálogo, la opción elegida,
//! el primer token del comando con su hash, la ruta editada, la dirección y la
//! decisión. La bitácora usa la misma redacción.

use ez_core::pyvalue::head_chars;
use ez_core::redact::redact;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

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
    fn redacts_nested_values() {
        let value = json!({"a": "sk-abc123def456", "b": ["token=xyz"], "n": 1});
        let text = redact_value(&value).to_string();
        assert!(!text.contains("sk-abc123def456"));
        assert!(!text.contains("xyz"));
        assert!(text.contains("\"n\":1"));
    }
}

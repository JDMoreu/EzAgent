//! Tipos del protocolo entre `ez` y EzAgent (diseño §7).

use std::collections::HashMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Quién hace una petición: el elefante del espacio o uno de sus peces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caller {
    Elephant { space: String },
    Fish { space: String, fish: String },
}

/// Una petición del protocolo: `{"command": "...", "args": {...}}`.
///
/// `args` vale `{}` cuando el cuerpo no lo trae.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub command: String,
    #[serde(default = "empty_args")]
    pub args: Value,
}

fn empty_args() -> Value {
    Value::Object(serde_json::Map::new())
}

/// La respuesta del árbitro: `{"ok": bool, "message": "...", "data": ...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    pub message: String,
    pub data: Value,
}

impl Reply {
    /// Acepta la petición.
    pub fn ok(message: impl Into<String>, data: Value) -> Reply {
        Reply {
            ok: true,
            message: message.into(),
            data,
        }
    }

    /// Rechaza la petición con un mensaje generado por el código.
    pub fn failure(message: impl Into<String>) -> Reply {
        Reply {
            ok: false,
            message: message.into(),
            data: Value::Null,
        }
    }
}

/// Atiende peticiones ya autenticadas. Lo implementa `ez-arbiter` en la ola 2.
pub trait Handler: Send + Sync {
    /// Responde a `request` en nombre de `caller`; nunca debe entrar en pánico.
    fn handle(&self, caller: &Caller, request: &Request) -> Reply;
}

/// Tokens de las terminales abiertas por EzAgent, cada uno con su [`Caller`].
#[derive(Default)]
pub struct Tokens {
    callers: RwLock<HashMap<String, Caller>>,
}

impl Tokens {
    /// Registro vacío.
    pub fn new() -> Tokens {
        Tokens::default()
    }

    /// Asocia `token` con `caller`; un token repetido se reemplaza.
    pub fn register(&self, token: &str, caller: Caller) {
        self.callers.write().insert(token.to_string(), caller);
    }

    /// Olvida `token`.
    pub fn revoke(&self, token: &str) {
        self.callers.write().remove(token);
    }

    /// El [`Caller`] de `token`, si sigue registrado.
    pub fn lookup(&self, token: &str) -> Option<Caller> {
        self.callers.read().get(token).cloned()
    }
}

/// Token nuevo: un uuid v4 en forma simple (32 caracteres hexadecimales).
pub fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tokens_register_lookup_and_revoke() {
        let tokens = Tokens::new();
        let elephant = Caller::Elephant {
            space: "espacio".to_string(),
        };
        tokens.register("abc", elephant.clone());
        assert_eq!(tokens.lookup("abc"), Some(elephant));
        assert_eq!(tokens.lookup("otro"), None);
        tokens.revoke("abc");
        assert_eq!(tokens.lookup("abc"), None);
    }

    #[test]
    fn new_tokens_are_simple_hex_uuids() {
        let first = new_token();
        let second = new_token();
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }

    #[test]
    fn reply_json_has_the_contract_fields() {
        let reply = Reply::failure("Token inválido o ausente.");
        let text = serde_json::to_string(&reply).expect("json");
        assert_eq!(
            text,
            r#"{"ok":false,"message":"Token inválido o ausente.","data":null}"#
        );
        let request: Request =
            serde_json::from_str(r#"{"command":"status"}"#).expect("request sin args");
        assert_eq!(request.args, json!({}));
        assert_eq!(request.command, "status");
    }
}

//! Registro de peticiones recibidas en `requests.jsonl`.
//!
//! Se guarda el cuerpo exacto y el JSON parseado para comparar después contra
//! el Python. Las cabeceras de autenticación nunca se escriben: solo se anota
//! si la petición traía clave (`has_api_key`).

use serde_json::{Map, Value};

use super::Route;

const ALLOWED_HEADERS: [&str; 4] = ["accept", "anthropic-version", "content-type", "user-agent"];

/// Una línea de `requests.jsonl`.
pub struct RecordEntry<'a> {
    pub number: u64,
    pub route: Route,
    pub method: &'a str,
    pub path: &'a str,
    pub stream: bool,
    pub headers: &'a [(String, String)],
    pub has_api_key: bool,
    pub raw_body: &'a str,
    pub body: Option<&'a Value>,
}

fn headers_map(headers: &[(String, String)]) -> Value {
    let mut map = Map::new();
    for (name, value) in headers {
        let name = name.to_lowercase();
        if ALLOWED_HEADERS.contains(&name.as_str()) {
            map.insert(name, Value::String(value.clone()));
        }
    }
    Value::Object(map)
}

/// Serializa la entrada como una línea JSON (sin salto final).
pub fn line(entry: &RecordEntry<'_>) -> String {
    let value = serde_json::json!({
        "n": entry.number,
        "route": entry.route.name(),
        "method": entry.method,
        "path": entry.path,
        "stream": entry.stream,
        "has_api_key": entry.has_api_key,
        "headers": headers_map(entry.headers),
        "body": entry.body.cloned().unwrap_or(Value::Null),
        "raw_body": entry.raw_body,
    });
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_writes_credentials() {
        let headers = vec![
            ("Authorization".to_string(), "Bearer sk-secreto".to_string()),
            ("x-api-key".to_string(), "sk-otro".to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        let body = serde_json::json!({"model": "m"});
        let text = line(&RecordEntry {
            number: 1,
            route: Route::OpenAi,
            method: "POST",
            path: "/v1/chat/completions",
            stream: true,
            headers: &headers,
            has_api_key: true,
            raw_body: "{\"model\":\"m\"}",
            body: Some(&body),
        });
        assert!(!text.contains("sk-secreto"));
        assert!(!text.contains("sk-otro"));
        assert!(!text.contains("Authorization"));
        let value: Value = serde_json::from_str(&text).expect("línea JSON");
        assert_eq!(value["headers"]["content-type"], "application/json");
        assert_eq!(value["has_api_key"], true);
        assert_eq!(value["route"], "openai");
        assert_eq!(value["raw_body"], "{\"model\":\"m\"}");
    }
}

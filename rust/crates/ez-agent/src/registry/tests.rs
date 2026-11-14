use super::*;
use ez_core::pyvalue::char_len;
use std::time::Duration;

fn echo() -> ToolSpec {
    let handler: Handler = Arc::new(|args| Ok(json!({"echo": args.get("text")}).into()));
    ToolSpec::new(
        "echo",
        "Repite",
        json!({"type": "object", "required": ["text"], "properties": {
            "text": {"type": "string", "minLength": 2, "maxLength": 5},
            "mode": {"enum": ["a", "b"]},
            "n": {"type": "integer", "minimum": 1}
        }, "additionalProperties": false}),
        0,
        handler,
    )
}

#[test]
fn validates_like_python() {
    let registry = ToolRegistry::new();
    registry.register(echo());
    let check = |args: Value| registry.validate("echo", &args);
    assert_eq!(
        check(json!({})),
        Some("Falta el parámetro obligatorio: 'text'.".into())
    );
    assert_eq!(
        check(json!({"text": 3})),
        Some("El parámetro 'text' debe ser de tipo texto.".into())
    );
    assert_eq!(
        check(json!({"text": "a"})),
        Some("El parámetro 'text' debe tener al menos 2 caracteres.".into())
    );
    assert_eq!(
        check(json!({"text": "abcdef"})),
        Some("El parámetro 'text' debe tener como máximo 5 caracteres.".into())
    );
    assert_eq!(
        check(json!({"text": "ok", "mode": "c"})),
        Some("El parámetro 'mode' debe ser uno de: 'a', 'b'.".into())
    );
    assert_eq!(
        check(json!({"text": "ok", "n": 0})),
        Some("El parámetro 'n' debe ser mayor o igual que 1.".into())
    );
    assert_eq!(
        check(json!({"text": "ok", "z": 1})),
        Some("El parámetro 'z' no está permitido.".into())
    );
    assert_eq!(check(json!({"text": "ok", "n": 2.0})), None);
    assert_eq!(
        registry.validate("nada", &json!({})),
        Some("Herramienta desconocida: nada.".into())
    );
    assert_eq!(
        registry.validate("echo", &json!([])),
        Some("Los argumentos de la herramienta deben ser un objeto JSON.".into())
    );
}

#[test]
fn runs_normalizes_and_times_out() {
    let registry = ToolRegistry::new();
    registry.register(echo());
    let result = registry.run("echo", &json!({"text": "hi"}));
    assert_eq!(result.data, json!({"echo": "hi"}));
    assert!(result.ok);
    let slow: Handler = Arc::new(|_| {
        std::thread::sleep(Duration::from_millis(300));
        Ok(json!(1).into())
    });
    let mut spec = ToolSpec::new("slow", "", json!({}), 0, slow);
    spec.timeout_s = 0.05;
    registry.register(spec);
    let result = registry.run("slow", &json!({}));
    assert_eq!(result.outcome, "unknown_outcome");
    assert!(result
        .error
        .unwrap_or_default()
        .starts_with("La herramienta superó el tiempo límite de 0.05 s."));
    let boom: Handler = Arc::new(|_| Err("ValueError: mal".into()));
    registry.register(ToolSpec::new("boom", "", json!({}), 0, boom));
    assert_eq!(
        registry.run("boom", &json!({})).error.as_deref(),
        Some("Error al ejecutar la herramienta: ValueError: mal")
    );
}

#[test]
fn renders_within_budget_as_json() {
    let result = ToolResult::ok(json!({"texto": "x".repeat(500)}));
    let text = ToolRegistry::render_for_llm(&result, 200);
    assert!(char_len(&text) <= 200);
    let parsed: Value = serde_json::from_str(&text).expect("JSON válido");
    assert_eq!(parsed["truncated"], true);
    assert_eq!(ToolRegistry::render_for_llm(&result, 1), "");
    assert_eq!(
        ToolRegistry::render_for_llm(&result, 30),
        "{\"truncated\": true}"
    );
    assert_eq!(
        ToolRegistry::render_for_llm(&ToolResult::ok(json!(1)), 2000).len(),
        62
    );
}

#[test]
fn schemas_respect_order_and_limit() {
    let registry = ToolRegistry::new();
    registry.register(echo());
    let names = vec!["nada".to_string(), "echo".to_string()];
    assert_eq!(registry.schemas(Some(&names), 14).len(), 1);
    assert!(registry.schemas(None, 0).is_empty());
    assert_eq!(
        unique_names(&["a".into(), "a".into()]),
        vec!["a".to_string()]
    );
}

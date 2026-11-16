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

fn value_tool(name: &str, value: Value) -> ToolSpec {
    let handler: Handler = Arc::new(move |_| Ok(ToolReturn::Value(value.clone())));
    ToolSpec::new(name, "", json!({}), 0, handler)
}

#[test]
fn normalizes_every_python_return_shape() {
    let registry = ToolRegistry::new();
    registry.register(ToolSpec::new(
        "direct",
        "",
        json!({}),
        0,
        Arc::new(|_| Ok(ToolReturn::Result(ToolResult::ok(json!(1))))),
    ));
    assert_eq!(registry.run("direct", &json!({})), ToolResult::ok(json!(1)));
    registry.register(value_tool("contract", json!({"ok": true, "data": [1]})));
    assert_eq!(
        registry.run("contract", &json!({})),
        ToolResult::ok(json!([1]))
    );
    registry.register(value_tool("error", json!({"error": "solo error"})));
    let failed = registry.run("error", &json!({}));
    assert!(!failed.ok);
    assert_eq!(failed.error.as_deref(), Some("solo error"));
    registry.register(value_tool("data", json!({"data": 5})));
    assert_eq!(registry.run("data", &json!({})), ToolResult::ok(json!(5)));
    registry.register(value_tool("none", Value::Null));
    assert_eq!(
        registry.run("none", &json!({})),
        ToolResult::ok(Value::Null)
    );
    registry.register(value_tool("text", json!("texto")));
    assert_eq!(
        registry.run("text", &json!({})),
        ToolResult::ok(json!("texto"))
    );
    registry.register(value_tool("number", json!(42)));
    assert_eq!(
        registry.run("number", &json!({})),
        ToolResult::ok(json!(42))
    );
    registry.register(value_tool("plain", json!({"cualquier": "cosa"})));
    assert_eq!(
        registry.run("plain", &json!({})),
        ToolResult::ok(json!({"cualquier": "cosa"}))
    );
}

#[test]
fn a_duplicate_replaces_the_previous_tool() {
    let registry = ToolRegistry::new();
    registry.register(value_tool("t", json!("viejo")));
    registry.register(value_tool("t", json!("nuevo")));
    assert_eq!(registry.names(), vec!["t".to_string()]);
    assert_eq!(registry.run("t", &json!({})).data, json!("nuevo"));
}

#[test]
fn registration_is_safe_to_share_between_threads() {
    let registry = Arc::new(ToolRegistry::new());
    let workers: Vec<_> = (0..8)
        .map(|worker| {
            let registry = Arc::clone(&registry);
            std::thread::spawn(move || {
                for index in 0..50 {
                    registry.register(value_tool(&format!("t{worker}-{index}"), json!(1)));
                    let _ = registry.names();
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("hilo");
    }
    assert_eq!(registry.names().len(), 400);
}

#[test]
fn validates_boolean_and_number_types() {
    let registry = ToolRegistry::new();
    let handler: Handler = Arc::new(|_| Ok(ToolResult::ok(Value::Null).into()));
    registry.register(ToolSpec::new(
        "bools",
        "",
        json!({"type": "object", "properties": {"x": {"type": "boolean"}}}),
        0,
        Arc::clone(&handler),
    ));
    for good in [json!(true), json!(false)] {
        assert_eq!(registry.validate("bools", &json!({"x": good})), None);
    }
    for bad in [json!(0), json!(1), json!("true"), Value::Null] {
        let error = registry
            .validate("bools", &json!({"x": bad}))
            .unwrap_or_default();
        assert!(error.contains("'x'"), "{bad} aceptado: {error}");
    }
    registry.register(ToolSpec::new(
        "numbers",
        "",
        json!({"type": "object", "properties": {"x": {"type": "number"}}}),
        0,
        handler,
    ));
    for good in [json!(0), json!(1.5), json!(-3)] {
        assert_eq!(registry.validate("numbers", &json!({"x": good})), None);
    }
    for bad in [json!("1"), Value::Null, json!(true)] {
        let error = registry
            .validate("numbers", &json!({"x": bad}))
            .unwrap_or_default();
        assert!(error.contains("'x'"), "{bad} aceptado: {error}");
    }
}

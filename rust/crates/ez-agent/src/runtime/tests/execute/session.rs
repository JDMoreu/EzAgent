//! Pruebas del runtime: herramientas extra, notificación, API del mensajero,
//! recarga de permisos y auditoría (separadas de `execute.rs` por tamaño).

use super::*;

#[test]
fn extra_tools_from_the_previous_turn_survive_exactly_one_turn() {
    let (_work, runtime) = fixture("carry");
    runtime.begin_turn("crea una carpeta");
    let first = function_names(&runtime.tools_for("crea una carpeta"));
    assert!(first.contains(&"create_dir".to_string()));
    runtime.begin_turn("en Documentos");
    let second = function_names(&runtime.tools_for("en Documentos"));
    assert!(second.contains(&"create_dir".to_string()));
    runtime.begin_turn("¿qué hora es?");
    let third = function_names(&runtime.tools_for("¿qué hora es?"));
    assert!(!third.contains(&"create_dir".to_string()));
}

#[test]
fn notify_address_needs_a_request_and_a_watched_agent() {
    let (_work, runtime) = fixture("notify");
    assert_eq!(runtime.pending_notify_address(), None);
    runtime.set_last_agent_address(Some("t1:2"));
    for text in [
        "avísame cuando termine",
        "VIGILA ese agente",
        "dime qué pasó",
    ] {
        runtime.begin_turn(text);
        assert_eq!(
            runtime.pending_notify_address(),
            Some("t1:2".into()),
            "{text}"
        );
    }
    for text in ["hola, buen día", ""] {
        runtime.begin_turn(text);
        assert_eq!(runtime.pending_notify_address(), None, "{text}");
    }
    runtime.set_last_agent_address(None);
    runtime.begin_turn("avísame");
    assert_eq!(runtime.pending_notify_address(), None);
}

#[test]
fn confirmation_api_delegates_and_handles_a_missing_gate() {
    let (_work, runtime) = fixture("confirm-api");
    runtime.set_confirm_gate(None);
    assert!(!runtime.confirm_pending());
    assert_eq!(runtime.pending_confirmation_token(), None);
    assert_eq!(runtime.submit_confirmation("sí", None), None);
    runtime.arm_confirmation();
    runtime.submit_confirmation_click(true);
    runtime.cancel_confirmation();
    assert!(runtime.confirm_gate().is_none());
    let gate = Arc::new(ScriptedGate::new(true));
    runtime.set_confirm_gate(Some(Arc::clone(&gate) as Arc<dyn ConfirmationGate>));
    assert!(runtime.confirm_gate().is_some());
}

#[test]
fn render_result_limits_what_the_model_sees() {
    let (_work, runtime) = fixture("render");
    let output = runtime.render_result(&ToolResult::ok(json!("z".repeat(10_000))));
    assert!(output.chars().count() <= 2000);
    let payload: Value = serde_json::from_str(&output).expect("json");
    assert_eq!(payload["truncated"], true);
    assert!(payload["original_chars"].as_u64().unwrap_or(0) > 2000);
}

#[test]
fn the_agent_runtime_api_exposes_the_base_tools_and_serialized_results() {
    use ez_llm::messenger::AgentRuntimeApi;

    let (_work, runtime) = fixture("api");
    assert_eq!(
        AgentRuntimeApi::remaining_tool_calls(&runtime),
        Some(runtime.remaining_tool_calls())
    );
    let schemas = AgentRuntimeApi::base_tool_schemas(&runtime).expect("esquemas");
    assert!(schemas.len() <= BASE_TOOLS.len());
    for name in function_names(&schemas) {
        assert!(BASE_TOOLS.contains(&name.as_str()), "{name}");
    }
    let result = AgentRuntimeApi::execute(&runtime, "get_datetime", "{}").expect("resultado");
    assert_eq!(result["ok"], true);
    let rendered = AgentRuntimeApi::render_result(&runtime, result);
    assert!(rendered.contains("\"ok\": true"));
    let prompt = AgentRuntimeApi::system_prompt(&runtime);
    assert!(prompt.starts_with(crate::system_prompt::SAFETY_PREAMBLE_TEXT));
}

#[test]
fn a_broken_tool_group_does_not_take_down_the_registry() {
    let (_work, runtime) = fixture("broken-group");
    runtime.add_tool_group(Arc::new(|_| panic!("tools.d roto")));
    assert!(runtime.registry().get("get_datetime").is_some());
    let audit = std::fs::read_to_string(runtime.config().audit_path).unwrap_or_default();
    assert!(audit.contains("No pude cargar un grupo de herramientas"));
}

#[test]
fn the_messenger_uses_the_runtime_as_its_agent_core() {
    use ez_llm::messenger::{AgentRuntimeApi, Messenger, MessengerCallbacks};

    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AgentRuntime>();
    let (_work, runtime) = fixture("messenger");
    let core: Arc<dyn AgentRuntimeApi> = Arc::new(runtime.clone());
    let messenger = Messenger::new(
        runtime.voice_config(),
        MessengerCallbacks::default(),
        Some(core),
    );
    assert!(messenger
        .system_prompt()
        .starts_with(crate::system_prompt::SAFETY_PREAMBLE_TEXT));
    assert_eq!(messenger.messages()[0]["role"], "system");
}

#[test]
fn refresh_applies_permissions_and_skills_only_when_they_change() {
    let (_work, runtime) = fixture("refresh");
    let skills_dir = runtime.config().skills_dir;
    // Sin cambios en disco no se rehace nada.
    runtime.refresh();
    assert_eq!(runtime.remaining_tool_calls(), 100);
    let marker = format!("tmap3-skill-{}", std::process::id());
    assert!(!runtime.skills().names().contains(&marker));

    let text = std::fs::read_to_string(runtime.config().permissions_path).expect("permisos");
    rewrite_permissions(
        &runtime,
        &text.replace(
            "max_tool_calls_per_turn = 100",
            "max_tool_calls_per_turn = 7",
        ),
    );
    runtime.refresh();
    assert_eq!(runtime.remaining_tool_calls(), 7);

    write_skill(&skills_dir, &marker, "primera");
    runtime.refresh();
    assert!(runtime.skills().names().contains(&marker));
}

#[test]
fn changing_read_roots_warns_and_keeps_the_old_roots() {
    let (work, runtime) = fixture("read-roots");
    let old_roots = runtime.config().paths.read_roots.clone();
    let other = work.parent().expect("raíz").join("nuevo");
    std::fs::create_dir_all(&other).expect("carpeta nueva");
    let text = std::fs::read_to_string(runtime.config().permissions_path).expect("permisos");
    rewrite_permissions(
        &runtime,
        &text.replace(
            &old_roots[0].replace('\\', "/"),
            &other.display().to_string().replace('\\', "/"),
        ),
    );
    runtime.refresh();
    assert_eq!(runtime.config().paths.read_roots, old_roots);
    let audit = std::fs::read_to_string(runtime.config().audit_path).unwrap_or_default();
    assert!(audit.to_lowercase().contains("reinicia"));
}

#[test]
fn the_audit_records_a_known_error_kind() {
    let (_work, runtime) = fixture("audit-kind");
    let handler: crate::registry::Handler = Arc::new(|_| {
        Ok(ToolResult {
            ok: false,
            data: Value::Null,
            error: Some("x".to_string()),
            outcome: "completed".to_string(),
            error_kind: Some("timeout".to_string()),
        }
        .into())
    });
    runtime.registry().register(ToolSpec::new(
        "falla",
        "d",
        json!({"type": "object", "properties": {}}),
        0,
        handler,
    ));
    runtime.begin_turn("prueba");
    assert_eq!(
        runtime.execute("falla", &json!({})).error_kind.as_deref(),
        Some("timeout")
    );
    let audit = std::fs::read_to_string(runtime.config().audit_path).expect("auditoría");
    let record = audit
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|row| row["tool"] == "falla")
        .expect("registro de la herramienta");
    assert_eq!(record["error_kind"], "timeout");
}

#[test]
fn the_real_confirmation_token_reaches_the_callback() {
    let tokens: Arc<Mutex<Vec<Option<String>>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&tokens);
    let callbacks = RuntimeCallbacks {
        on_confirm_ask: Some(Arc::new(move |_summary, _strong, token| {
            seen.lock().expect("tokens").push(token.map(str::to_string));
        })),
        ..RuntimeCallbacks::default()
    };
    let (_work, runtime) = fixture_with("confirm-token", callbacks);
    let mut config = runtime.config();
    config.general.confirm_timeout_s = 5.0;
    runtime.set_config(config);
    runtime.rebuild_confirm();
    let gate = runtime.confirm_gate().expect("gestor de confirmación");
    let answer = Arc::new(Mutex::new(None));
    let result = Arc::clone(&answer);
    let thread = std::thread::spawn(move || {
        *result.lock().expect("respuesta") = Some(gate.ask("¿Sigo?", false, None));
    });
    let mut token = None;
    for _ in 0..200 {
        token = runtime.pending_confirmation_token();
        if token.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let token = token.expect("token pendiente");
    assert_eq!(*tokens.lock().expect("tokens"), vec![Some(token.clone())]);
    assert_eq!(runtime.submit_confirmation("sí", Some(&token)), Some(true));
    thread.join().expect("hilo de confirmación");
    assert_eq!(*answer.lock().expect("respuesta"), Some(true));
    assert_eq!(runtime.pending_confirmation_token(), None);
}

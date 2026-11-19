//! Pruebas de los subagentes: el lanzamiento con padre y su ubicación.
//!
//! El doble de Herdr (`Fake`) registra cada argv y responde lo preparado; no
//! se ejecuta `hc` ni `herdr-app` reales.

use super::*;

// ------------------------------------------------------------------ lanzamiento

#[test]
fn con_padre_el_lanzamiento_va_por_hc_run_con_parent() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")]);
    let mut labels = Map::new();
    labels.insert("ez.role".to_string(), Value::from("developer"));
    labels.insert("ez.agent".to_string(), Value::from("opencode"));
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                task: "trabaja",
                parent: Some("t6h"),
                hc_name: Some("dev1-opencode"),
                labels: Some(labels),
                show: Placement::Tree,
                outlive_parent: true,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], true);
    let argv = fake.argv_of("hc", "run").expect("hc run");
    assert_eq!(
        argv,
        vec![
            "hc".to_string(),
            "run".to_string(),
            "--json".to_string(),
            "--bg".to_string(),
            "--parent".to_string(),
            "t6h".to_string(),
            "--name".to_string(),
            "dev1-opencode".to_string(),
            "--cwd".to_string(),
            "C:\\ws".to_string(),
            "--no-notify".to_string(),
            "--outlive-parent".to_string(),
            "--label".to_string(),
            "agent=claude".to_string(),
            "--label".to_string(),
            "ez.agent=opencode".to_string(),
            "--label".to_string(),
            "ez.role=developer".to_string(),
            "--".to_string(),
            "claude".to_string(),
        ]
    );
    assert!(!argv.contains(&"--no-parent".to_string()));
    assert!(fake.app_calls("tab.new").is_empty());
}

#[test]
fn con_padre_el_entorno_usa_la_misma_envoltura() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")]);
    let mut env = Map::new();
    env.insert("OPENCODE_CONFIG_CONTENT".to_string(), Value::from("{}"));
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                task: "trabaja",
                parent: Some("t6h"),
                env: Some(env),
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], true);
    let argv = fake.argv_of("hc", "run").expect("hc run");
    let tail: Vec<String> = argv[argv.len() - 4..].to_vec();
    assert_eq!(tail[0], "powershell.exe");
    assert_eq!(tail[1], "-NoProfile");
    assert_eq!(tail[2], "-EncodedCommand");
    assert!(!tail[3].is_empty());
}

#[test]
fn sin_padre_el_lanzamiento_sigue_siendo_el_de_hoy() {
    let fake = Fake::new().route("hc:run", vec![hc_run_ok()]);
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                task: "trabaja",
                background: true,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], true);
    let argv = fake.argv_of("hc", "run").expect("hc run");
    assert!(argv.contains(&"--no-parent".to_string()));
    assert!(!argv.contains(&"--parent".to_string()));
    // DECISION: `hc` rechaza la etiqueta legible; se le pasa su versión válida.
    assert_eq!(
        arg_after(&argv, "--name").as_deref(),
        Some("claude-voz"),
        "{argv:?}"
    );
}

// ------------------------------------------------------------------- ubicación

#[test]
fn en_el_arbol_no_se_llama_a_herdr_app() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")]);
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tree,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["placed"], "tree");
    assert!(fake.app_calls("terminal.keep").is_empty());
    assert!(fake.app_calls("pane.focus").is_empty());
}

#[test]
fn en_pestana_se_llama_a_terminal_keep() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")])
        .route(
            "app:terminal.keep",
            vec![Reply::json(&json!({"ok": true, "tab": "t12"}))],
        );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tab,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["placed"], "tab");
    assert_eq!(result["tab"], "t12");
    let keep = fake
        .app_calls("terminal.keep")
        .first()
        .cloned()
        .expect("terminal.keep");
    assert_eq!(
        keep,
        vec![
            "herdr-app".to_string(),
            "do".to_string(),
            "terminal.keep".to_string(),
            "--terminal".to_string(),
            "t12".to_string(),
            "--place".to_string(),
            "tab".to_string(),
        ]
    );
}

#[test]
fn terminal_keep_reintenta_mientras_herdr_no_ve_la_terminal() {
    let fake = Fake::new()
        .ticking(5.0)
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")])
        .route(
            "app:terminal.keep",
            vec![
                Reply::plain("no running terminal \"t12\""),
                Reply::plain("no running terminal \"t12\""),
                Reply::json(&json!({"ok": true, "tab": "t12"})),
            ],
        );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tab,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["placed"], "tab");
    assert_eq!(fake.app_calls("terminal.keep").len(), 3);
}

#[test]
fn en_panel_se_enfoca_el_padre_antes_de_colocar() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")])
        .route("app:pane.focus", vec![Reply::json(&json!({"ok": true}))])
        .route(
            "app:terminal.keep",
            vec![Reply::json(&json!({"ok": true, "pane": "p1"}))],
        );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Right,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["placed"], "right");
    assert_eq!(result["pane"], "p1");
    let focus = fake
        .app_calls("pane.focus")
        .first()
        .cloned()
        .expect("pane.focus");
    assert_eq!(
        focus,
        vec![
            "herdr-app".to_string(),
            "do".to_string(),
            "pane.focus".to_string(),
            "--terminal".to_string(),
            "t6h".to_string(),
        ]
    );
    let keep = fake
        .app_calls("terminal.keep")
        .first()
        .cloned()
        .expect("terminal.keep");
    assert_eq!(keep[6], "right");
}

#[test]
fn sin_panel_del_padre_el_subagente_abre_en_pestana() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")])
        .route("app:pane.focus", vec![Reply::error("no hay panel")])
        .route(
            "app:terminal.keep",
            vec![Reply::json(&json!({"ok": true, "tab": "t12"}))],
        );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Down,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["placed"], "tab");
    let warnings = result["warnings"].as_array().expect("avisos");
    assert!(warnings
        .iter()
        .any(|warning| warning
            == "El agente padre no tiene panel; abrí el subagente en una pestaña."));
}

#[test]
fn un_error_de_herdr_deja_el_subagente_en_el_arbol() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")])
        .route(
            "app:terminal.keep",
            vec![Reply::error("herdr-app no responde")],
        );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tab,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], true);
    assert_eq!(result["placed"], "tree");
    let warnings = result["warnings"].as_array().expect("avisos");
    assert!(warnings.iter().any(|warning| warning
        == "Herdr no pudo ubicar el subagente (herdr-app no responde); \
           quedó en el árbol bajo su agente."));
}

#[test]
fn el_resultado_informa_el_padre_el_nombre_y_las_etiquetas() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")]);
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tree,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["parent"], "t6h");
    assert_eq!(result["name"], "dev1-opencode");
    assert_eq!(result["placed"], "tree");
    assert_eq!(result["labels"]["agent"], "opencode");
    assert_eq!(result["labels"]["ez.role"], "developer");
}

#[test]
fn un_padre_distinto_del_pedido_se_avisa() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t9z")]);
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tree,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["parent"], "t9z");
    let warnings = result["warnings"].as_array().expect("avisos");
    assert!(warnings
        .iter()
        .any(|warning| warning == "El padre de t12 es t9z, no t6h."));
}

#[test]
fn el_resultado_dice_si_el_subagente_se_ve_o_queda_en_el_arbol() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")]);
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tree,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["visible"], false);
    assert_eq!(result["background"], true);
}

#[test]
fn un_subagente_en_pestana_se_ve_y_no_queda_en_el_arbol() {
    let fake = Fake::new()
        .route("hc:run", vec![hc_run_ok()])
        .route("hc:get", vec![hc_get_ok("t6h")])
        .route(
            "app:terminal.keep",
            vec![Reply::json(&json!({"ok": true, "tab": "t12"}))],
        );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                show: Placement::Tab,
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["visible"], true);
    assert_eq!(result["background"], false);
}

#[test]
fn un_nombre_ocupado_vuelve_como_error() {
    let fake = Fake::new().route(
        "hc:run",
        vec![Reply::json(&json!({
            "error": {
                "kind": "name_taken",
                "message": "a live terminal is already named \"dev1-opencode\"",
            }
        }))],
    );
    let result = herdr_de(&fake)
        .run_agent(
            Driver::default(),
            RunAgent {
                agent: "claude",
                parent: Some("t6h"),
                hc_name: Some("dev1-opencode"),
                ..RunAgent::default()
            },
        )
        .expect("sin proceso");
    assert_eq!(result["ok"], false);
    assert_eq!(result["name_taken"], true);
    assert_eq!(
        result["error"],
        "a live terminal is already named \"dev1-opencode\""
    );
}

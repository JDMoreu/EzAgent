use serde_json::{json, Value};

use super::*;

fn args(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("objeto")
}

#[test]
fn action_se_recorta_y_baja() {
    assert_eq!(action(&args(json!({"action": "  LIST "}))), "list");
    assert_eq!(action(&args(json!({"action": 3}))), "");
    assert_eq!(action(&Map::new()), "");
}

#[test]
fn checked_action_valida_la_lista() {
    let valid = ["list", "kill"];
    assert_eq!(
        checked_action(
            &args(json!({"action": "list"})),
            &valid,
            "delegate_to_agent"
        )
        .expect("válida"),
        "list"
    );
    assert_eq!(
        checked_action(&Map::new(), &valid, "delegate_to_agent")
            .expect_err("falta")
            .0,
        "Falta la acción de «delegate_to_agent». Acciones válidas: list, kill."
    );
    assert_eq!(
        checked_action(&args(json!({"action": "x"})), &valid, "delegate_to_agent")
            .expect_err("desconocida")
            .0,
        "Acción desconocida para «delegate_to_agent»: 'x'. Acciones válidas: list, kill."
    );
}

#[test]
fn required_text_exige_texto_no_vacio() {
    assert_eq!(
        required_text(&args(json!({"address": " t1 "})), "address", "read").expect("texto"),
        "t1"
    );
    for value in [json!({}), json!({"address": "  "}), json!({"address": 4})] {
        assert_eq!(
            required_text(&args(value), "address", "read")
                .expect_err("falta")
                .0,
            "Para la acción «read» falta el parámetro «address»."
        );
    }
}

#[test]
fn optional_text_distingue_ausente_de_invalido() {
    assert_eq!(
        optional_text(&Map::new(), "model", "run").expect("nada"),
        None
    );
    assert_eq!(
        optional_text(&args(json!({"model": null})), "model", "run").expect("nulo"),
        None
    );
    assert_eq!(
        optional_text(&args(json!({"model": "  "})), "model", "run").expect("vacío"),
        None
    );
    assert_eq!(
        optional_text(&args(json!({"model": " opus "})), "model", "run").expect("texto"),
        Some("opus".to_string())
    );
    assert_eq!(
        optional_text(&args(json!({"model": 2})), "model", "run")
            .expect_err("número")
            .0,
        "El parámetro «model» de la acción «run» debe ser texto."
    );
}

#[test]
fn optional_bool_exige_booleano() {
    assert!(!optional_bool(&Map::new(), "gui", "run").expect("ausente"));
    assert!(optional_bool(&args(json!({"gui": true})), "gui", "run").expect("true"));
    assert_eq!(
        optional_bool(&args(json!({"gui": "sí"})), "gui", "run")
            .expect_err("texto")
            .0,
        "El parámetro «gui» de la acción «run» debe ser verdadero o falso."
    );
}

#[test]
fn optional_lines_valida_rango_y_tipo() {
    assert_eq!(optional_lines(&Map::new(), "read").expect("ausente"), None);
    assert_eq!(
        optional_lines(&args(json!({"lines": 40})), "read").expect("válido"),
        Some(40)
    );
    assert_eq!(
        optional_lines(&args(json!({"lines": 9})), "read")
            .expect_err("rango")
            .0,
        "El parámetro «lines» debe estar entre 10 y 120."
    );
    for value in [
        json!({"lines": "40"}),
        json!({"lines": 40.0}),
        json!({"lines": true}),
    ] {
        assert_eq!(
            optional_lines(&args(value), "read").expect_err("tipo").0,
            "El parámetro «lines» de la acción «read» debe ser un número entero entre 10 y 120."
        );
    }
    assert_eq!(
        optional_lines(&args(json!({"lines": 18446744073709551615u64})), "read")
            .expect_err("fuera de rango")
            .0,
        "El parámetro «lines» debe estar entre 10 y 120."
    );
}

#[test]
fn wrapped_normaliza_errores_de_herdr() {
    let ok = wrapped(args(json!({"ok": true, "address": "t1"})));
    assert!(ok.ok);
    assert_eq!(ok.data["address"], json!("t1"));
    let failed = wrapped(args(json!({"ok": false, "error": "boom"})));
    assert!(!failed.ok);
    assert_eq!(failed.error.as_deref(), Some("boom"));
    let empty = wrapped(args(json!({"ok": false, "error": ""})));
    assert_eq!(
        empty.error.as_deref(),
        Some("Herdr no pudo completar la acción.")
    );
}

#[test]
fn detecta_las_expresiones_de_voz() {
    assert!(wants_notify("avísame cuando termine"));
    assert!(wants_notify("vigílame"));
    assert!(!wants_notify("hola"));
    assert!(wants_deliver("dame el resultado"));
    assert!(!wants_deliver("abre la terminal"));
    assert!(wants_close("ciérrate al terminar"));
    assert!(!wants_close("sigue escuchando"));
}

#[test]
fn user_text_baja_y_shown_usa_el_nombre() {
    let callbacks = HerdrCallbacks {
        get_last_user_text: Arc::new(|| "Avísame Cuando Termine".to_string()),
        ..HerdrCallbacks::default()
    };
    assert_eq!(user_text(&callbacks), "avísame cuando termine");
    assert_eq!(
        shown("t1", &|_: &str| " Claude Code ".to_string()),
        "Claude Code"
    );
    assert_eq!(shown("", &|_: &str| "x".to_string()), "el agente indicado");
    assert_eq!(
        shown("t1", &|_: &str| "  ".to_string()),
        "el agente indicado"
    );
}

#[test]
fn niveles_de_riesgo() {
    assert_eq!(delegate_level("list"), Some(0));
    assert_eq!(delegate_level("run"), Some(1));
    assert_eq!(delegate_level("kill"), Some(2));
    assert_eq!(delegate_level("otra"), None);
}

#[test]
fn estado_compartido_guarda_la_ultima_direccion() {
    let state = shared_state();
    assert_eq!(last_address(&state), None);
    set_last_address(&state, Some("t1".to_string()));
    assert_eq!(last_address(&state), Some("t1".to_string()));
    set_last_address(&state, None);
    assert_eq!(last_address(&state), None);
}

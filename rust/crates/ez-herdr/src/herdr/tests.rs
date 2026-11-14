//! Pruebas del adaptador de Herdr: texto, lectura, lanzamiento y subprocesos.

mod launch;
mod parsing;
mod subprocess;

use super::*;
use serde_json::json;

#[test]
fn strip_ansi_quita_colores_osc_y_cajas() {
    assert_eq!(strip_ansi("\x1b[31mrojo\x1b[0m"), "rojo");
    assert_eq!(strip_ansi("\x1b]0;título\x07hola"), "hola");
    assert_eq!(strip_ansi("\x1b]0;x\x1b\\hola"), "hola");
    assert_eq!(strip_ansi("a\r\nb\rc"), "a\nb\nc");
    assert_eq!(strip_ansi("┌──┐\n│ x │"), "x");
    assert_eq!(strip_ansi("  hola   mundo  \n\n  fin "), "hola mundo\nfin");
    assert_eq!(strip_ansi("\x1b[1;32mPS C:\\x>\x1b[0m"), "PS C:\\x>");
    assert_eq!(strip_ansi(""), "");
}

#[test]
fn visible_name_prefiere_espacio_y_pestana() {
    let cases = [
        (
            json!({"where": "EzAgent / Claude Code"}),
            "Claude Code de EzAgent",
        ),
        (json!({"where": "EzAgent / 2"}), "pestaña 2 de EzAgent"),
        (
            json!({"name": "2", "space": "EzAgent"}),
            "pestaña 2 de EzAgent",
        ),
        (json!({"name": "Claude Code"}), "Claude Code"),
        (
            json!({"agent": "claude-code", "cwd": "C:\\work\\proyecto"}),
            "Claude Code en proyecto",
        ),
        (json!({"agent": "", "cwd": ""}), "el agente"),
        (
            json!({"where": "  EzAgent   /   Claude   Code "}),
            "Claude Code de EzAgent",
        ),
        (json!({"where": "solo"}), "solo"),
        (
            json!({"agent": "opencode", "cwd": "C:\\Users\\x"}),
            "OpenCode en x",
        ),
        (json!({"agent": "otro", "cwd": ""}), "otro"),
        (
            json!({"name": "２", "space": "EzAgent"}),
            "pestaña ２ de EzAgent",
        ),
        (
            json!({"agent": "Gemini", "space": "EzAgent", "name": "3"}),
            "pestaña 3 de EzAgent",
        ),
        (json!({"cwd": "C:\\solo\\carpeta"}), "el agente"),
        (json!({"address": "t51"}), "el agente"),
    ];
    for (item, expected) in cases {
        assert_eq!(
            visible_name(item.as_object().expect("objeto")),
            expected,
            "{item}"
        );
    }
}

#[test]
fn command_line_cita_lo_necesario() {
    assert_eq!(
        super::text::command_line(
            &["echo".to_string(), "hola mundo".to_string()],
            &Map::new(),
            None
        ),
        "echo 'hola mundo'"
    );
    let env: Map<String, Value> = [
        ("XDG_STATE_HOME".to_string(), Value::from("C:\\x")),
        ("B".to_string(), Value::from("a'b")),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        super::text::command_line(
            &["a".to_string(), "b".to_string()],
            &env,
            Some(&["C:\\bin".to_string()])
        ),
        "$env:XDG_STATE_HOME='C:\\x'; $env:B='a''b'; $env:PATH='C:\\bin;'+$env:PATH; a b"
    );
    assert_eq!(super::text::command_line(&[], &Map::new(), None), "");
}

#[test]
fn valid_keys_normaliza_y_valida() {
    let enter = vec!["Enter".to_string(), " ESC ".to_string()];
    assert_eq!(
        super::text::valid_keys(&enter),
        Some(vec!["enter".to_string(), "esc".to_string()])
    );
    assert!(super::text::valid_keys(&["ctrl+t".to_string()]).is_some());
    assert!(super::text::valid_keys(&["f12".to_string()]).is_some());
    assert!(super::text::valid_keys(&["f13".to_string()]).is_none());
    assert!(super::text::valid_keys(&[]).is_none());
    assert!(super::text::valid_keys(&["ctrl++".to_string()]).is_none());
    assert!(super::text::valid_keys(&vec!["a".to_string(); 13]).is_none());
}

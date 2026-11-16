use ez_core::config::VoiceConfig;
use ez_herdr::herdr::Herdr;
use serde_json::{json, Map, Value};

use super::*;
use crate::registry::ToolResult;

fn args(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("objeto")
}

fn label(_: &str) -> String {
    "Claude Code".to_string()
}

#[test]
fn resumenes_de_delegate() {
    let cases = [
        (json!({"action": "list"}), "Listar los agentes vivos de Herdr."),
        (json!({"action": "get", "address": "t1"}), "Consultar Claude Code."),
        (json!({"action": "get"}), "Consultar un agente de Herdr."),
        (
            json!({"action": "read", "address": "t1", "lines": 40}),
            "Leer las últimas 40 líneas dClaude Code.",
        ),
        (
            json!({"action": "read", "address": "t1", "lines": 18446744073709551615u64}),
            "Leer las últimas 18446744073709551615 líneas dClaude Code.",
        ),
        (
            json!({"action": "read", "address": "t1"}),
            "Leer el buffer dClaude Code.",
        ),
        (
            json!({"action": "read"}),
            "Leer el buffer de un agente de Herdr.",
        ),
        (
            json!({"action": "send", "address": "t1", "task": "haz X"}),
            "Enviar a Claude Code la tarea: haz X",
        ),
        (
            json!({"action": "send", "address": "t1"}),
            "Enviar una tarea a Claude Code.",
        ),
        (
            json!({"action": "send"}),
            "Enviar una tarea a un agente de Herdr.",
        ),
        (
            json!({"action": "agents"}),
            "Consultar los agentes disponibles y sus modelos.",
        ),
        (
            json!({"action": "guide", "agent": "claude"}),
            "Leer el manual del agente claude.",
        ),
        (
            json!({"action": "guide"}),
            "Leer el manual del agente indicado.",
        ),
        // El Python repite las teclas como destino; se reproduce igual.
        (
            json!({"action": "keys", "address": "t1", "keys": ["enter", "esc"]}),
            "Pulsar enter esc en enter esc.",
        ),
        (
            json!({"action": "keys", "address": "t1"}),
            "Pulsar teclas en teclas.",
        ),
        (
            json!({"action": "configure", "address": "t1", "model": "opus", "effort": "high"}),
            "Cambiar modelo opus y esfuerzo high de Claude Code.",
        ),
        (
            json!({"action": "configure", "address": "t1", "model": "opus"}),
            "Cambiar modelo opus de Claude Code.",
        ),
        (
            json!({"action": "configure", "address": "t1"}),
            "Cambiar la configuración de Claude Code.",
        ),
        (
            json!({"action": "configure"}),
            "Cambiar la configuración de un agente.",
        ),
        (
            json!({"action": "run", "agent": "claude"}),
            "Abrir el agente claude en una pestaña visible.",
        ),
        (
            json!({"action": "run"}),
            "Abrir el agente un agente en una pestaña visible.",
        ),
        (
            json!({"action": "run", "agent": "claude", "task": "haz X"}),
            "Lanzar el agente claude en una pestaña visible con la tarea: haz X",
        ),
        (
            json!({
                "action": "run",
                "agent": "claude",
                "task": "haz X",
                "background": true,
                "gui": true,
                "model": "opus",
                "effort": "high",
                "input_from": "t2"
            }),
            "Lanzar el agente claude en segundo plano con interfaz gráfica \
             (modelo opus, esfuerzo high, a partir de Claude Code) con la tarea: haz X",
        ),
        (
            json!({
                "action": "run",
                "agent": "claude",
                "task": "haz X",
                "then": [{"agent": "codex", "model": "m"}]
            }),
            "Lanzar el agente claude en una pestaña visible; después: codex (m) con la tarea: haz X",
        ),
        (
            json!({"action": "run", "agent": "claude", "task": "haz", "then": "mal"}),
            "Lanzar el agente claude en una pestaña visible con la tarea: haz",
        ),
        (
            json!({"action": "open_terminal", "name": "x"}),
            "Abrir una terminal x en Herdr.",
        ),
        (
            json!({"action": "open_terminal"}),
            "Abrir una terminal en Herdr.",
        ),
        (json!({"action": "focus", "address": "t1"}), "Enfocar Claude Code."),
        (json!({"action": "focus"}), "Enfocar un agente de Herdr."),
        (json!({"action": "kill", "address": "t1"}), "Terminar Claude Code."),
        (json!({"action": "kill"}), "Terminar un agente de Herdr."),
        (
            json!({"action": "watch", "address": "t1"}),
            "Vigilar Claude Code y avisar al terminar.",
        ),
        (
            json!({"action": "watch"}),
            "Vigilar un agente y avisar al terminar.",
        ),
        (json!({"action": "zzz"}), "Acción desconocida para Herdr."),
    ];
    for (value, expected) in cases {
        assert_eq!(
            delegate_summary(&args(value.clone()), &label),
            expected,
            "{value}"
        );
    }
}

#[test]
fn resumenes_de_voz() {
    let cases = [
        (
            json!({"action": "close"}),
            "Cerrar la escucha al terminar de hablar.",
        ),
        (
            json!({"action": "close_and_watch", "address": "t1"}),
            "Cerrar la escucha y avisar cuando termine Claude Code.",
        ),
        (
            json!({"action": "close_and_watch"}),
            "Cerrar la escucha y avisar cuando termine el agente.",
        ),
        (
            json!({"action": "mic_mute", "muted": true}),
            "Silenciar el micrófono.",
        ),
        (
            json!({"action": "mic_mute", "muted": false}),
            "Reactivar el micrófono.",
        ),
        (
            json!({"action": "mic_mute"}),
            "Cambiar el estado del micrófono.",
        ),
        (
            json!({"action": "repeat_last"}),
            "Repetir el último resumen.",
        ),
        (
            json!({"action": "mute", "minutes": 5}),
            "Silenciar los avisos durante 5 minutos.",
        ),
        (json!({"action": "mute"}), "Silenciar los avisos."),
        (
            json!({"action": "mute", "minutes": 5.0}),
            "Silenciar los avisos.",
        ),
        (
            json!({"action": "mute", "minutes": 18446744073709551615u64}),
            "Silenciar los avisos durante 18446744073709551615 minutos.",
        ),
        (
            json!({"action": "zzz"}),
            "Acción desconocida de control por voz.",
        ),
    ];
    for (value, expected) in cases {
        assert_eq!(
            voice_summary(&args(value.clone()), &label),
            expected,
            "{value}"
        );
    }
}

#[test]
fn info_fija_nivel_y_confirmacion() {
    let list = delegate_info(&args(json!({"action": "list"})), &label);
    assert_eq!(list.level, 0);
    assert!(!list.always_confirm);
    assert_eq!(list.summary, "Listar los agentes vivos de Herdr.");
    let kill = delegate_info(&args(json!({"action": "kill"})), &label);
    assert_eq!(kill.level, 2);
    assert!(kill.always_confirm);
    let unknown = delegate_info(&args(json!({"action": "zzz"})), &label);
    assert_eq!(unknown.level, 2);
    assert!(unknown.always_confirm);
    let voice = voice_info(&args(json!({"action": "close"})), &label);
    assert_eq!(voice.level, 1);
    assert!(!voice.always_confirm);
    assert_eq!(voice.summary, "Cerrar la escucha al terminar de hablar.");
}

#[test]
fn labelled_pone_el_nombre_visible_y_quita_los_crudos() {
    let herdr = Herdr::new(VoiceConfig::new());
    let mut item = args(json!({
        "address": "t1",
        "where": "EzAgent / Claude Code",
        "title": "Claude Code",
        "name": "1"
    }));
    labelled(&|address| herdr.label_for(address), &mut item);
    assert_eq!(item["label"], json!("Claude Code de EzAgent"));
    for key in ["where", "title", "name"] {
        assert!(!item.contains_key(key), "{key}");
    }
    let mut labelled_already = args(json!({"address": "t2", "label": "Mía", "name": "x"}));
    labelled(&|address| herdr.label_for(address), &mut labelled_already);
    assert_eq!(labelled_already["label"], json!("Mía"));
    assert!(!labelled_already.contains_key("name"));
}

#[test]
fn with_labels_recorre_objetos_y_listas() {
    let herdr = Herdr::new(VoiceConfig::new());
    let object = with_labels(
        &|address| herdr.label_for(address),
        ToolResult::ok(json!({"address": "t1", "name": "1"})),
    );
    assert_eq!(object.data["label"], json!("pestaña 1"));
    let list = with_labels(
        &|address| herdr.label_for(address),
        ToolResult::ok(json!([{"address": "t1", "name": "1"}, {"agent": "claude"}])),
    );
    assert_eq!(list.data[0]["label"], json!("pestaña 1"));
    assert_eq!(list.data[1], json!({"agent": "claude"}));
}

#[test]
fn confirmations_never_say_the_internal_address() {
    let labels = |address: &str| match address {
        "t51" => "Claude Code de EzAgent".to_string(),
        "t5y" => "OpenCode de EzAgent".to_string(),
        _ => String::new(),
    };
    let pattern = regex::Regex::new(r"\bt[0-9a-z]{2,3}\b").expect("patrón");
    for value in [
        json!({"action": "kill", "address": "t5y"}),
        json!({"action": "send", "address": "t51", "task": "Iguala el espaciado"}),
        json!({"action": "read", "address": "t51", "lines": 20}),
        json!({"action": "watch", "address": "t51"}),
        json!({"action": "focus", "address": "t51"}),
        json!({"action": "keys", "address": "t51", "keys": ["enter"]}),
        json!({"action": "configure", "address": "t51", "model": "sonnet"}),
        json!({"action": "run", "agent": "claude", "task": "x", "input_from": "t5z"}),
    ] {
        let summary = delegate_summary(&args(value.clone()), &labels);
        assert!(!pattern.is_match(&summary), "{summary}");
        let unknown = delegate_summary(&args(value.clone()), &|_| String::new());
        assert!(!pattern.is_match(&unknown), "{unknown}");
    }
    let kill = delegate_summary(&args_kill(), &labels);
    assert_eq!(kill, "Terminar OpenCode de EzAgent.");
}

fn args_kill() -> Map<String, Value> {
    args(json!({"action": "kill", "address": "t5y"}))
}

#[test]
fn a_result_with_an_address_carries_only_its_visible_name() {
    let herdr = Herdr::new(VoiceConfig::new());
    let raw = json!({
        "address": "t51",
        "agent": "claude-code",
        "where": "EzAgent / Claude Code",
        "name": "Claude Code",
        "title": "Claude Code",
        "labels": {"where": "EzAgent / Claude Code"},
    });
    let listed = with_labels(
        &|address| herdr.label_for(address),
        ToolResult::ok(json!([raw.clone()])),
    );
    let item = &listed.data[0];
    assert_eq!(item["label"], json!("Claude Code de EzAgent"));
    for key in ["where", "name", "title", "labels"] {
        assert!(
            !item.as_object().expect("objeto").contains_key(key),
            "{key}"
        );
    }
    let single = with_labels(&|address| herdr.label_for(address), ToolResult::ok(raw));
    assert_eq!(single.data["label"], json!("Claude Code de EzAgent"));
    assert!(!single
        .data
        .as_object()
        .expect("objeto")
        .contains_key("where"));

    // Sin etiqueta en el contexto, el nombre visible se compone de sus campos.
    let fresh = with_labels(
        &|_| String::new(),
        ToolResult::ok(json!({"address": "t60", "name": "QA", "space": "EzAgent"})),
    );
    assert_eq!(fresh.data["label"], json!("QA de EzAgent"));
}

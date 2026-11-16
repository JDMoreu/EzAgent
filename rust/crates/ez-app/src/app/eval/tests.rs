//! Pruebas del subcomando `eval` (F8-02).

use ez_core::config::VoiceConfig;
use serde_json::{json, Value};

use super::payload::{first_step, parse_calls, text, tool_names};
use super::request::{configure, EvalRequest};
use super::{parse_args, Mode, Parsed};

fn argv(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn mode_defaults_to_live_and_accepts_both_spellings() {
    assert_eq!(parse_args(&argv(&[])), Ok(Parsed::Mode(Mode::Live)));
    assert_eq!(
        parse_args(&argv(&["--mode", "context"])),
        Ok(Parsed::Mode(Mode::Context))
    );
    assert_eq!(
        parse_args(&argv(&["--mode=live"])),
        Ok(Parsed::Mode(Mode::Live))
    );
    assert_eq!(parse_args(&argv(&["-h"])), Ok(Parsed::Help));
}

#[test]
fn mode_errors_follow_argparse() {
    assert_eq!(
        parse_args(&argv(&["--mode"])),
        Err("argument --mode: expected one argument".to_string())
    );
    assert_eq!(
        parse_args(&argv(&["--mode", "offline"])),
        Err(
            "argument --mode: invalid choice: 'offline' (choose from 'context', 'live')"
                .to_string()
        )
    );
    assert_eq!(
        parse_args(&argv(&["--bogus"])),
        Err("unrecognized arguments: --bogus".to_string())
    );
}

#[test]
fn request_reads_user_history_and_settings() {
    let request = EvalRequest::from_json(
        &json!({
            "user": "¿qué hora es?",
            "history": "[{\"role\": \"user\", \"content\": \"hola\"}]",
            "config": {"temperature": 0, "max_tokens": "64", "model": "m"},
        })
        .to_string(),
    )
    .expect("petición válida");
    assert_eq!(request.user, "¿qué hora es?");
    assert_eq!(request.history.len(), 1);
    assert_eq!(request.settings.get("temperature"), Some(&json!(0)));

    let empty = EvalRequest::from_json("{}").expect("petición vacía");
    assert_eq!(empty.user, "");
    assert!(empty.history.is_empty());
    assert!(empty.settings.is_empty());
}

#[test]
fn history_rejects_what_is_not_a_list() {
    let error = EvalRequest::from_json(&json!({"history": {"a": 1}}).to_string())
        .expect_err("un dict no es un historial");
    assert_eq!(error, "TypeError: 'dict' object is not iterable");
}

#[test]
fn configure_applies_python_conversions() {
    let mut cfg = VoiceConfig::new();
    let settings = json!({"temperature": "0.5", "max_tokens": 64.9, "model": "m1"});
    configure(&mut cfg, settings.as_object().expect("objeto")).expect("ajustes válidos");
    assert_eq!(cfg.get("llm_temperature"), Some(&json!(0.5)));
    assert_eq!(cfg.get("llm_max_tokens"), Some(&json!(64)));
    assert_eq!(cfg.get("llm_model"), Some(&json!("m1")));

    let mut cfg = VoiceConfig::new();
    configure(&mut cfg, &serde_json::Map::new()).expect("sin ajustes");
    assert_eq!(cfg.get("llm_temperature"), Some(&json!(0.0)));
}

#[test]
fn configure_reports_python_errors() {
    let mut cfg = VoiceConfig::new();
    let settings = json!({"temperature": "caliente"});
    let error = configure(&mut cfg, settings.as_object().expect("objeto"))
        .expect_err("temperatura inválida");
    assert_eq!(
        error,
        "ValueError: could not convert string to float: 'caliente'"
    );
}

#[test]
fn first_step_returns_the_assistant_after_the_user() {
    let messages = vec![
        json!({"role": "system", "content": "s"}),
        json!({"role": "user", "content": "hola"}),
        json!({"role": "assistant", "content": "primera"}),
        json!({"role": "user", "content": "otra"}),
        json!({"role": "assistant", "content": "segunda", "tool_calls": []}),
    ];
    assert_eq!(text(&first_step(&messages, "hola")), "primera");
    assert_eq!(text(&first_step(&messages, "otra")), "segunda");
    assert_eq!(
        first_step(&messages, "nada"),
        json!({"content": "", "tool_calls": []})
    );
    // `(content or "").strip()`: un nulo no se convierte en "None".
    assert_eq!(text(&json!({"content": null})), "");
}

#[test]
fn parse_calls_marks_invalid_arguments() {
    let step = json!({
        "content": " ",
        "tool_calls": [
            {"function": {"name": "get_datetime", "arguments": "{\"tz\": \"local\"}"}},
            {"function": {"name": "read_file", "arguments": "{roto"}},
            {"function": {"name": "open_path", "arguments": "[]"}},
            {"function": {"name": "forget"}},
        ],
    });
    let calls = parse_calls(&step);
    assert_eq!(calls.len(), 4);
    assert_eq!(calls[0]["arguments"], json!({"tz": "local"}));
    assert_eq!(calls[0]["arguments_valid"], json!(true));
    assert_eq!(calls[1]["arguments"], json!("{roto"));
    assert_eq!(calls[1]["arguments_valid"], json!(false));
    assert_eq!(calls[2]["arguments_valid"], json!(false));
    assert_eq!(calls[3]["arguments"], json!({}));
    assert_eq!(text(&step), "");
}

#[test]
fn tool_names_reads_the_function_block() {
    let tools: Vec<Value> = vec![
        json!({"type": "function", "function": {"name": "a"}}),
        json!({"type": "function", "function": {"name": "b"}}),
    ];
    assert_eq!(tool_names(&tools), vec![json!("a"), json!("b")]);
    assert!(tool_names(&[]).is_empty());
}

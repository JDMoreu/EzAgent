//! Pruebas de la línea de comandos y de los formatos de `--list-*` (F6-07).

use std::path::PathBuf;

use ez_audio::capture::InputDevice;
use serde_json::{json, Map};

use super::{
    agent_row, help_text, looks_negative, mic_row, parse_args, parse_env_value, program_name,
    usage, Cli, ParseEnd,
};

fn argv(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

const HELP: &str = "usage: app.py [-h] [--config CONFIG] [--ask ASK] [--say SAY] [--list-agents]\n              [--list-mics] [--demo-ui] [--mic MIC] [--write-config]\n\nEzAgent Voice — mensajero para Herdr\n\noptions:\n  -h, --help       show this help message and exit\n  --config CONFIG\n  --ask ASK        Pregunta única (sin micrófono) y sale\n  --say SAY        Dice un texto por Kokoro y sale\n  --list-agents\n  --list-mics\n  --demo-ui        Recorre los estados del HUD sin micrófono ni servidores\n  --mic MIC        Índice de micrófono\n  --write-config   Escribe voice.json con los valores actuales y sale\n";

#[test]
fn help_matches_argparse_word_for_word() {
    assert_eq!(help_text("app.py"), HELP);
    assert_eq!(
        usage("app.py"),
        HELP.lines().take(2).collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn parses_every_option() {
    let cli = parse_args(&argv(&[
        "app.py",
        "--config",
        "voice.json",
        "--ask=¿qué hora es?",
        "--say",
        "hola",
        "--list-agents",
        "--list-mics",
        "--demo-ui",
        "--mic",
        "3",
        "--write-config",
    ]))
    .expect("argumentos válidos");
    assert_eq!(
        cli,
        Cli {
            config: Some(PathBuf::from("voice.json")),
            ask: Some("¿qué hora es?".to_string()),
            say: Some("hola".to_string()),
            list_agents: true,
            list_mics: true,
            demo_ui: true,
            mic: Some(3),
            write_config: true,
            help: false,
        }
    );
}

#[test]
fn accepts_argparse_abbreviations_and_negative_mics() {
    let cli = parse_args(&argv(&["app.py", "--list-a", "--mic", "-1"]))
        .expect("prefijo único y micrófono negativo");
    assert!(cli.list_agents);
    assert_eq!(cli.mic, Some(-1));
    let cli = parse_args(&argv(&["app.py", "--mic", "1_6"])).expect("guion bajo de int()");
    assert_eq!(cli.mic, Some(16));
    assert!(looks_negative("-12"));
    assert!(!looks_negative("-1_2"));
}

#[test]
fn help_is_immediate() {
    assert_eq!(parse_args(&argv(&["app.py", "-h"])), Err(ParseEnd::Help));
    assert_eq!(
        parse_args(&argv(&["app.py", "--help"])),
        Err(ParseEnd::Help)
    );
    assert_eq!(
        parse_args(&argv(&["app.py", "--bogus", "--help"])),
        Err(ParseEnd::Help)
    );
}

#[test]
fn reports_argparse_errors() {
    assert_eq!(
        parse_args(&argv(&["app.py", "--bogus"])),
        Err(ParseEnd::Error(
            "unrecognized arguments: --bogus".to_string()
        ))
    );
    assert_eq!(
        parse_args(&argv(&["app.py", "extra"])),
        Err(ParseEnd::Error("unrecognized arguments: extra".to_string()))
    );
    assert_eq!(
        parse_args(&argv(&["app.py", "--ask"])),
        Err(ParseEnd::Error(
            "argument --ask: expected one argument".to_string()
        ))
    );
    assert_eq!(
        parse_args(&argv(&["app.py", "--ask", "--help"])),
        Err(ParseEnd::Error(
            "argument --ask: expected one argument".to_string()
        ))
    );
    assert_eq!(
        parse_args(&argv(&["app.py", "--mic", "x"])),
        Err(ParseEnd::Error(
            "argument --mic: invalid int value: 'x'".to_string()
        ))
    );
    assert_eq!(
        parse_args(&argv(&["app.py", "--list"])),
        Err(ParseEnd::Error(
            "ambiguous option: --list could match --list-agents, --list-mics".to_string()
        ))
    );
    assert_eq!(
        parse_args(&argv(&["app.py", "--list-agents=x"])),
        Err(ParseEnd::Error(
            "argument --list-agents: ignored explicit argument 'x'".to_string()
        ))
    );
}

#[test]
fn double_dash_stops_option_parsing() {
    assert_eq!(
        parse_args(&argv(&["app.py", "--", "--list-agents"])),
        Err(ParseEnd::Error(
            "unrecognized arguments: --list-agents".to_string()
        ))
    );
}

#[test]
fn program_name_uses_the_file_name() {
    assert_eq!(program_name(r"C:\ruta\app.py"), "app.py");
    assert_eq!(program_name("app.py"), "app.py");
}

#[test]
fn dotenv_values_follow_python_dotenv() {
    assert_eq!(parse_env_value("hola"), "hola");
    assert_eq!(parse_env_value("hola # comentario"), "hola");
    assert_eq!(parse_env_value("\"a\\nb\""), "a\nb");
    assert_eq!(parse_env_value("'a b'"), "a b");
}

#[test]
fn agent_rows_match_the_python_format() {
    let mut row = Map::new();
    row.insert("address".to_string(), json!("t1"));
    row.insert("agent".to_string(), json!("claude-code"));
    row.insert("state".to_string(), json!("done"));
    row.insert("cwd".to_string(), json!(r"C:\trabajo"));
    row.insert("where".to_string(), json!("EzAgent / Claude Code"));
    assert_eq!(
        agent_row(&row),
        "  t1  claude-code    done     C:\\trabajo  EzAgent / Claude Code"
    );
    assert_eq!(agent_row(&Map::new()), "      ?              ?          ");
}

#[test]
fn mic_rows_match_the_python_format() {
    let device = InputDevice {
        index: 2,
        name: "Micrófono".to_string(),
        rate: 16_000,
        channels: 1,
    };
    assert_eq!(
        mic_row(&device),
        "  2  Micrófono                                      16000 Hz  1 canales"
    );
}

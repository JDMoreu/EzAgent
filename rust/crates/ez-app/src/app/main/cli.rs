//! Análisis de argumentos con la semántica y los textos de `argparse`.

use std::path::{Path, PathBuf};

use ez_core::config::python_int;

const OPTIONS: [&str; 9] = [
    "--help",
    "--config",
    "--ask",
    "--say",
    "--list-agents",
    "--list-mics",
    "--demo-ui",
    "--mic",
    "--write-config",
];

/// Argumentos ya analizados (`args` de `argparse`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cli {
    pub config: Option<PathBuf>,
    pub ask: Option<String>,
    pub say: Option<String>,
    pub list_agents: bool,
    pub list_mics: bool,
    pub demo_ui: bool,
    pub mic: Option<i64>,
    pub write_config: bool,
    pub help: bool,
}

/// Fin temprano del análisis: ayuda pedida o error de uso.
#[derive(Clone, Debug, PartialEq)]
pub enum ParseEnd {
    Help,
    Error(String),
}

/// Ancho útil de `argparse.HelpFormatter` (`columns - 2`).
const HELP_WIDTH: usize = 78;

pub(crate) fn usage(prog: &str) -> String {
    let prefix = format!("usage: {prog} ");
    let indent = " ".repeat(prefix.chars().count());
    let items = [
        "[-h]",
        "[--config CONFIG]",
        "[--ask ASK]",
        "[--say SAY]",
        "[--list-agents]",
        "[--list-mics]",
        "[--demo-ui]",
        "[--mic MIC]",
        "[--write-config]",
    ];
    let mut lines: Vec<String> = Vec::new();
    let mut current = prefix.clone();
    for item in items {
        let addition = if current.ends_with(' ') {
            item.to_string()
        } else {
            format!(" {item}")
        };
        if current.chars().count() + addition.chars().count() > HELP_WIDTH
            && current.trim_end() != prefix.trim_end()
        {
            lines.push(current);
            current = format!("{indent}{item}");
        } else {
            current.push_str(&addition);
        }
    }
    lines.push(current);
    lines.join("\n")
}

/// Texto de `--help`, con los mismos renglones y textos que `argparse`.
pub fn help_text(prog: &str) -> String {
    format!(
        "{}\n\nEzAgent Voice — mensajero para Herdr\n\noptions:\n\
         \x20 -h, --help       show this help message and exit\n\
         \x20 --config CONFIG\n\
         \x20 --ask ASK        Pregunta única (sin micrófono) y sale\n\
         \x20 --say SAY        Dice un texto por Kokoro y sale\n\
         \x20 --list-agents\n\
         \x20 --list-mics\n\
         \x20 --demo-ui        Recorre los estados del HUD sin micrófono ni servidores\n\
         \x20 --mic MIC        Índice de micrófono\n\
         \x20 --write-config   Escribe voice.json con los valores actuales y sale\n",
        usage(prog)
    )
}

fn takes_value(option: &str) -> bool {
    matches!(option, "--config" | "--ask" | "--say" | "--mic")
}

fn resolve_long(name: &str) -> Result<Option<&'static str>, String> {
    if let Some(exact) = OPTIONS.iter().find(|option| **option == name) {
        return Ok(Some(exact));
    }
    let matches: Vec<&'static str> = OPTIONS
        .iter()
        .filter(|option| option.starts_with(name))
        .copied()
        .collect();
    match matches.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one)),
        many => Err(format!(
            "ambiguous option: {name} could match {}",
            many.join(", ")
        )),
    }
}

fn set_value(cli: &mut Cli, option: &str, value: &str) -> Result<(), String> {
    match option {
        "--config" => cli.config = Some(PathBuf::from(value)),
        "--ask" => cli.ask = Some(value.to_string()),
        "--say" => cli.say = Some(value.to_string()),
        "--mic" => {
            cli.mic = Some(
                python_int(value)
                    .ok_or_else(|| format!("argument --mic: invalid int value: '{value}'"))?,
            );
        }
        _ => {
            return Err(format!(
                "argument {option}: ignored explicit argument '{value}'"
            ))
        }
    }
    Ok(())
}

fn set_flag(cli: &mut Cli, option: &str) {
    match option {
        "--help" => cli.help = true,
        "--list-agents" => cli.list_agents = true,
        "--list-mics" => cli.list_mics = true,
        "--demo-ui" => cli.demo_ui = true,
        "--write-config" => cli.write_config = true,
        _ => {}
    }
}

fn missing_value(option: &str) -> String {
    format!("argument {option}: expected one argument")
}

/// `argparse.parse_args`: `argv[0]` es el programa y el resto, las opciones.
pub fn parse_args(argv: &[String]) -> Result<Cli, ParseEnd> {
    let mut cli = Cli::default();
    let mut extras: Vec<String> = Vec::new();
    let mut index = 1usize;
    let mut positional_only = false;
    while index < argv.len() {
        let token = argv[index].clone();
        index += 1;
        if positional_only {
            extras.push(token);
            continue;
        }
        if token == "--" {
            positional_only = true;
            continue;
        }
        if token == "-h" {
            return Err(ParseEnd::Help);
        }
        if !token.starts_with("--") {
            extras.push(token);
            continue;
        }
        parse_long(&mut cli, &token, argv, &mut index, &mut extras)?;
    }
    if !extras.is_empty() {
        return Err(ParseEnd::Error(format!(
            "unrecognized arguments: {}",
            extras.join(" ")
        )));
    }
    Ok(cli)
}

/// Un token `--opción[=valor]`; los desconocidos quedan para el error final.
fn parse_long(
    cli: &mut Cli,
    token: &str,
    argv: &[String],
    index: &mut usize,
    extras: &mut Vec<String>,
) -> Result<(), ParseEnd> {
    let (name, inline) = match token.split_once('=') {
        Some((name, value)) => (name, Some(value)),
        None => (token, None),
    };
    let option = match resolve_long(name) {
        Ok(Some(option)) => option,
        Ok(None) => {
            extras.push(token.to_string());
            return Ok(());
        }
        Err(error) => return Err(ParseEnd::Error(error)),
    };
    if option == "--help" {
        if let Some(value) = inline {
            return Err(ParseEnd::Error(format!(
                "argument --help: ignored explicit argument '{value}'"
            )));
        }
        return Err(ParseEnd::Help);
    }
    if !takes_value(option) {
        if let Some(value) = inline {
            return Err(ParseEnd::Error(format!(
                "argument {option}: ignored explicit argument '{value}'"
            )));
        }
        set_flag(cli, option);
        return Ok(());
    }
    let value = option_value(argv, index, option, inline)?;
    set_value(cli, option, &value).map_err(ParseEnd::Error)
}

/// Valor de una opción con argumento, separado o inline.
fn option_value(
    argv: &[String],
    index: &mut usize,
    option: &str,
    inline: Option<&str>,
) -> Result<String, ParseEnd> {
    if let Some(value) = inline {
        return Ok(value.to_string());
    }
    let next = argv.get(*index);
    let usable =
        next.is_some_and(|next| !next.starts_with('-') || next == "-" || looks_negative(next));
    if !usable {
        return Err(ParseEnd::Error(missing_value(option)));
    }
    *index += 1;
    Ok(next.cloned().unwrap_or_default())
}

/// `-1` se acepta como valor de `--mic`, igual que en `argparse`.
pub(crate) fn looks_negative(token: &str) -> bool {
    token
        .strip_prefix('-')
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

/// `os.path.basename(sys.argv[0])`.
pub(crate) fn program_name(argv0: &str) -> String {
    Path::new(argv0).file_name().map_or_else(
        || argv0.to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

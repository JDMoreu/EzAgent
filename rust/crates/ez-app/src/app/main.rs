//! Port of `main()` (`app.py` 3987-4085) (F6-07).
//!
//! La línea de comandos se analiza a mano con la semántica y los textos de
//! `argparse` (sin `clap`, que no está entre las dependencias del proyecto).

mod ask;
mod cli;
mod env;
mod lists;
mod output;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use ez_core::config::{config_path, VoiceConfig};
use ez_core::llm_providers::needs_api_key;
use ez_core::pyvalue::truthy;
use serde_json::{json, Value};

use super::core::{cfg_text, log, VoiceApp};
use super::demo::run_demo;
use super::live::LiveHooks;

pub use cli::{help_text, parse_args, Cli, ParseEnd};
#[cfg(test)]
pub(crate) use cli::{looks_negative, program_name, usage};
#[cfg(test)]
pub(crate) use env::parse_env_value;
pub use lists::{agent_row, mic_row, print_agents, print_mics};

use ask::{run_ask, run_say};
use env::load_env_candidates;
use output::{display_path, print_line, write_stderr, write_stdout};

/// `main()` como valor de retorno (el binario lo usa como código de salida).
pub fn run() -> i32 {
    let argv: Vec<String> = std::env::args_os()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    run_with(&argv)
}

/// `main()` con los argumentos ya separados (pruebas incluidas).
pub fn run_with(argv: &[String]) -> i32 {
    // `eval` no abre ventanas ni micrófono: se atiende antes que nada.
    if let Some(code) = super::eval::maybe_run(argv) {
        return code;
    }
    // DECISION: la app instalada no tiene Python; `ezagent-voice downloads`
    // es `python -m ezagent.core.downloads` (mismos argumentos y salida).
    if argv.get(1).map(String::as_str) == Some("downloads") {
        let mut stdout = std::io::stdout();
        return ez_core::downloads::cli_main(&argv[2..], &mut stdout);
    }
    // Antes de crear cualquier ventana Win32, como el Python.
    ez_hud::win::dpi::enable_dpi_awareness();
    let prog = argv.first().map_or_else(
        || "ezagent-voice".to_string(),
        |value| cli::program_name(value),
    );
    let cli = match parse_args(argv) {
        Ok(cli) => cli,
        Err(ParseEnd::Help) => {
            write_stdout(&help_text(&prog));
            return 0;
        }
        Err(ParseEnd::Error(message)) => {
            write_stderr(&format!(
                "{}\n{prog}: error: {message}\n",
                cli::usage(&prog)
            ));
            return 2;
        }
    };
    init_logging();
    if cli.demo_ui {
        run_demo();
        return 0;
    }
    if cli.list_mics {
        print_mics();
        return 0;
    }
    load_env_candidates();
    let mut cfg = match VoiceConfig::load(cli.config.as_deref()) {
        Ok(cfg) => cfg,
        Err(error) => {
            write_stderr(&format!("{error}\n"));
            return 1;
        }
    };
    if let Some(mic) = cli.mic {
        cfg.set("input_device_index", json!(mic));
        // `--mic` es explícito: el nombre guardado no debe ganarle.
        cfg.set("input_device_name", Value::Null);
    }
    if let Some(code) = run_batch(&cli, &cfg) {
        return code;
    }
    run_conversation(&cli, cfg)
}

/// Registro a stderr, como el `loguru` por omisión del Python.
fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
}

/// `--write-config`, `--list-agents` y `--say`, en el orden del Python.
fn run_batch(cli: &Cli, cfg: &VoiceConfig) -> Option<i32> {
    if cli.write_config {
        return Some(write_config(cli, cfg));
    }
    if cli.list_agents {
        print_agents(cfg);
        return Some(0);
    }
    if let Some(text) = cli.say.as_ref().filter(|text| !text.is_empty()) {
        return Some(run_say(cfg, text));
    }
    None
}

fn write_config(cli: &Cli, cfg: &VoiceConfig) -> i32 {
    let path = cli.config.clone().unwrap_or_else(config_path);
    match cfg.save(Some(&path)) {
        Ok(()) => {
            print_line(&display_path(&path));
            0
        }
        Err(error) => {
            write_stderr(&format!("{error}\n"));
            1
        }
    }
}

/// La app de voz: comprobación de la API key, `--ask` y el bucle real.
fn run_conversation(cli: &Cli, cfg: VoiceConfig) -> i32 {
    let api_key_missing = cfg.get("api_key").is_none_or(|value| !truthy(value));
    let provider = cfg_text(&cfg, "llm_provider", "");
    let base_url = cfg_text(&cfg, "llm_base_url", "");
    if api_key_missing && needs_api_key(Some(&provider), &base_url) {
        log(format!(
            "Falta la API key del proveedor {provider}. Configura LLM_API_KEY \
             o la variable del proveedor en .env o llm.json."
        ));
        return 1;
    }
    if let Some(text) = cli.ask.as_ref().filter(|text| !text.is_empty()) {
        return run_ask(&cfg, text);
    }
    let config_file = cli.config.clone().unwrap_or_else(config_path);
    log(format!("Configuración: {}", display_path(&config_file)));
    let tasks_path = cli
        .config
        .as_ref()
        .map(|path| path.with_file_name("tasks.json"));
    let app = match VoiceApp::new(cfg, tasks_path, Some(config_file)) {
        Ok(app) => app,
        Err(error) => {
            write_stderr(&format!("{error}\n"));
            return 1;
        }
    };
    app.set_hooks(Arc::new(LiveHooks::new(&app)));
    app.run();
    0
}

/// El binario llama a esta función y sale con su código.
pub fn main() -> i32 {
    run()
}

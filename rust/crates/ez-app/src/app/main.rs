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

/// Registro a stderr, como el `loguru` por omisión del Python, y a un
/// archivo con rotación diaria.
///
/// DECISION: la app se arranca sin consola, así que el registro solo en
/// stderr se perdía: no quedaba rastro de un congelamiento (DX-1, paso 7).
/// Ahora también va a `%APPDATA%\EzAgent\logs\ezagent.log.<fecha>`, con
/// rotación diaria y siete días de historia.
fn init_logging() {
    init_logging_in(&ez_core::dirs::logs_dir());
}

/// Registro a stderr y a un archivo con rotación diaria, en la carpeta dada.
///
/// DECISION: la carpeta es un parámetro (y no el valor fijo) para que la
/// prueba pueda usar una temporal y no tocar el registro de la persona
/// (DX-1, paso 7).
///
/// Devuelve `false` si la carpeta no se pudo crear (entonces el registro se
/// queda solo en stderr).
pub fn init_logging_in(dir: &std::path::Path) -> bool {
    // DX-1d: el hook va antes que nada. Un pánico que cruza un callback de
    // Windows aborta el proceso, y el registro del `tracing` es asíncrono: la
    // línea del pánico tiene que escribirla el hook, síncrona, en el archivo.
    ez_core::panics::install_hook_in(dir);
    if std::fs::create_dir_all(dir).is_err() {
        let _ = tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .try_init();
        return false;
    }
    let appender = tracing_appender::rolling::daily(dir, "ezagent.log");
    let (file, guard) = tracing_appender::non_blocking(appender);
    // DECISION: el guard tiene que vivir tanto como la app: al soltarlo
    // el archivo deja de recibir el registro.
    std::mem::forget(guard);
    let _ = tracing_subscriber::fmt()
        .with_writer(ConsoleAndFile { file })
        .with_ansi(false)
        .try_init();
    prune_logs(dir, LOG_DAYS);
    true
}

/// Cuántos días de registro se guardan.
const LOG_DAYS: u64 = 7;

/// Escribe el registro en la consola y en el archivo.
struct ConsoleAndFile {
    file: tracing_appender::non_blocking::NonBlocking,
}

/// Los dos destinos, en un solo escritor.
struct BothWriters {
    console: std::io::Stderr,
    file: tracing_appender::non_blocking::NonBlocking,
}

impl std::io::Write for BothWriters {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let console = self.console.write(buf);
        let file = self.file.write(buf);
        console.and(file)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.console.flush()?;
        self.file.flush()
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for ConsoleAndFile {
    type Writer = BothWriters;

    fn make_writer(&'a self) -> Self::Writer {
        BothWriters {
            console: std::io::stderr(),
            file: self.file.clone(),
        }
    }
}

/// Borra los registros con más de `days` días.
///
/// DECISION: solo se borran los archivos que el propio registro crea (los
/// que empiezan por `ezagent.log`); nada más de la carpeta.
pub fn prune_logs(dir: &std::path::Path, days: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(days * 24 * 60 * 60))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    for entry in entries.flatten() {
        let path = entry.path();
        let is_ours = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("ezagent.log"));
        if !is_ours {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        if modified < cutoff {
            let _ = std::fs::remove_file(&path);
        }
    }
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

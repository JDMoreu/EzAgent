//! `hc` y `herdr-app` falsos (F1-11).
//!
//! Imita los subcomandos que usa `voice/herdr.py` (`ls`, `get`, `read`, `send`,
//! `put`, `keys`, `kill`, `run`, `do tab.new`, `do agent.focus`), más `spaces`
//! y `tabs` para inspeccionar el estado. El estado vive en un JSON elegido con
//! `--state` (o `FAKE_HC_STATE`): las pruebas de punta a punta pueden editarlo
//! entre llamadas para provocar que un agente termine o se bloquee.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

mod commands;

pub use commands::execute;

/// Estado completo del Herdr falso.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Último número de terminal creado (`tN`).
    pub next: u32,
    /// Espacios conocidos (aunque no tengan pestañas).
    pub spaces: Vec<String>,
    pub terminals: Vec<Terminal>,
}

/// Un terminal con su pantalla y lo que se le escribió.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Terminal {
    pub address: String,
    pub tab: String,
    pub space: String,
    pub name: String,
    pub title: String,
    pub agent: String,
    pub state: String,
    pub cwd: String,
    #[serde(rename = "where")]
    pub where_: String,
    pub turn: i64,
    /// Pantalla completa (sin ANSI); `read` devuelve sus últimas líneas.
    pub screen: String,
    /// Textos enviados con `send`, en orden.
    pub sent: Vec<String>,
    /// Teclas pulsadas con `keys`, en orden.
    pub keys: Vec<String>,
    /// Texto escrito que espera Enter (lo usa `keys` con `enter`).
    pub pending_input: String,
    /// Selector abierto (`model` o `effort`) que consume la siguiente escritura.
    pub selector: String,
    /// Nivel al que pasa el próximo `ctrl+t` (ciclo de variantes).
    pub variant_index: usize,
    /// Lecturas necesarias para que un trabajo pase a `idle` y muestre su conclusión.
    pub done_after_reads: Option<u32>,
    /// Milisegundos desde el envío de la tarea para pasar a `idle` (lo ve el vigía).
    pub done_after_ms: Option<u64>,
    /// Momento (ms UNIX) en que se envió la tarea; lo fija `send`.
    pub started_at: Option<u64>,
    pub reads: u32,
    pub conclusion: String,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            address: String::new(),
            tab: String::new(),
            space: String::from("main"),
            name: String::new(),
            title: String::new(),
            agent: String::from("powershell.exe"),
            state: String::from("idle"),
            cwd: String::new(),
            where_: String::new(),
            turn: 0,
            screen: String::new(),
            sent: Vec::new(),
            keys: Vec::new(),
            pending_input: String::new(),
            selector: String::new(),
            variant_index: 0,
            done_after_reads: None,
            done_after_ms: None,
            started_at: None,
            reads: 0,
            conclusion: String::new(),
        }
    }
}

impl State {
    pub fn load(path: &Path, seed: Option<&Path>) -> Result<Self> {
        if path.is_file() {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("no pude leer el estado {}", path.display()))?;
            return serde_json::from_str(&text)
                .with_context(|| format!("el estado {} no es JSON válido", path.display()));
        }
        if let Some(seed) = seed {
            let text = std::fs::read_to_string(seed)
                .with_context(|| format!("no pude leer el seed {}", seed.display()))?;
            return serde_json::from_str(&text)
                .with_context(|| format!("el seed {} no es JSON válido", seed.display()));
        }
        Ok(State::default())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("no pude crear {}", parent.display()))?;
            }
        }
        let text = serde_json::to_string_pretty(self).context("no pude serializar el estado")?;
        std::fs::write(path, text)
            .with_context(|| format!("no pude guardar el estado {}", path.display()))
    }

    pub fn find(&self, address: &str) -> Option<&Terminal> {
        self.terminals
            .iter()
            .find(|terminal| terminal.address == address)
    }

    pub fn find_mut(&mut self, address: &str) -> Option<&mut Terminal> {
        self.terminals
            .iter_mut()
            .find(|terminal| terminal.address == address)
    }

    pub fn remove(&mut self, address: &str) -> Option<Terminal> {
        let index = self
            .terminals
            .iter()
            .position(|terminal| terminal.address == address)?;
        Some(self.terminals.remove(index))
    }

    /// Crea un terminal `tN` en un espacio; la pantalla empieza siendo el prompt de PowerShell.
    pub fn create(&mut self, name: &str, folder: &str, argv: &[String], space: &str) -> Terminal {
        self.next += 1;
        let address = format!("t{}", self.next);
        let tab = format!("tab{}", self.next);
        if !self.spaces.iter().any(|known| known == space) {
            self.spaces.push(space.to_string());
        }
        let agent = argv
            .first()
            .map(|arg| executable_name(arg))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "powershell.exe".to_string());
        let folder = if folder.trim().is_empty() {
            String::from("C:\\")
        } else {
            folder.to_string()
        };
        let terminal = Terminal {
            address: address.clone(),
            tab,
            space: space.to_string(),
            name: name.to_string(),
            title: name.to_string(),
            agent,
            state: String::from("idle"),
            cwd: folder.clone(),
            where_: format!("{space} / {name}"),
            screen: format!("PS {folder}> "),
            ..Terminal::default()
        };
        self.terminals.push(terminal.clone());
        terminal
    }
}

/// `basename` sin `.exe` en minúsculas, como lo compara `agent_session._is_shell`.
pub fn executable_name(arg: &str) -> String {
    Path::new(arg)
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Respuesta de una invocación: lo que imprime el binario y su código de salida.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reply {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Reply {
    pub fn ok(text: String) -> Self {
        Self {
            code: 0,
            stdout: text,
            stderr: String::new(),
        }
    }

    pub fn json(value: &Value) -> Self {
        Self::ok(format!("{value}\n"))
    }

    pub fn error(code: i32, message: &str) -> Self {
        Self {
            code,
            stdout: String::new(),
            stderr: format!("{message}\n"),
        }
    }

    pub fn json_error(code: i32, value: &Value) -> Self {
        Self {
            code,
            stdout: format!("{value}\n"),
            stderr: String::new(),
        }
    }
}

/// Opciones globales y comando que hay que ejecutar.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub state: PathBuf,
    pub seed: Option<PathBuf>,
    pub command: String,
    pub args: Vec<String>,
}

fn default_state_path() -> PathBuf {
    if let Some(value) = std::env::var_os("FAKE_HC_STATE") {
        return PathBuf::from(value);
    }
    std::env::temp_dir()
        .join("ezagent-fake-hc")
        .join("state.json")
}

impl Invocation {
    /// Acepta `--state`, `--seed` y `--json` antes del comando.
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut state = default_state_path();
        let mut seed = None;
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--state" => {
                    state = PathBuf::from(args.get(index + 1).context("--state necesita ruta")?);
                    index += 2;
                }
                "--seed" => {
                    seed = Some(PathBuf::from(
                        args.get(index + 1).context("--seed necesita ruta")?,
                    ));
                    index += 2;
                }
                "--json" => index += 1,
                _ => break,
            }
        }
        let command = args
            .get(index)
            .cloned()
            .context("falta el comando (prueba con --help)")?;
        Ok(Self {
            state,
            seed,
            command,
            args: args[index + 1..].to_vec(),
        })
    }
}

/// Cerrojo entre procesos: `hc ls` del vigía y un `send` pueden coincidir.
///
/// Se apoya en `create_new` del archivo `<estado>.lock`; está pensado para el
/// servidor falso, no para un Herdr real.
struct StateLock {
    path: PathBuf,
}

impl StateLock {
    fn acquire(target: &Path) -> Result<Self> {
        let mut name = target
            .file_name()
            .map_or_else(std::ffi::OsString::new, |name| name.to_os_string());
        name.push(".lock");
        let path = target.with_file_name(name);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("no pude crear {}", parent.display()))?;
            }
        }
        // 30 s: con el equipo cargado (compilaciones, antivirus) varios fake-hc
        // en fila pueden tardar más de los 2 s de antes y la prueba fallaba.
        for _ in 0..6000 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    sleep(Duration::from_millis(5));
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("no pude bloquear el estado {}", target.display())
                    });
                }
            }
        }
        anyhow::bail!(
            "el estado {} sigue bloqueado por otro fake-hc",
            target.display()
        )
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        // En Windows el borrado puede fallar un instante si otro proceso (el
        // antivirus, por ejemplo) tiene abierto el archivo; un cerrojo que se
        // queda dejaría bloqueados a todos los fake-hc siguientes.
        for _ in 0..200 {
            match std::fs::remove_file(&self.path) {
                Err(error) if error.kind() != ErrorKind::NotFound => {
                    sleep(Duration::from_millis(10));
                }
                _ => return,
            }
        }
    }
}

/// Ejecuta una invocación completa del binario (sin imprimir nada).
pub fn dispatch(args: &[String]) -> Reply {
    let invocation = match Invocation::parse(args) {
        Ok(invocation) => invocation,
        Err(error) => return Reply::error(2, &format!("fake-hc: {error}")),
    };
    if matches!(invocation.command.as_str(), "--help" | "-h" | "help") {
        return Reply::ok(usage().to_string());
    }
    if invocation.command == "--version" {
        return Reply::ok(format!("fake-hc {}\n", env!("CARGO_PKG_VERSION")));
    }
    let _lock = match StateLock::acquire(&invocation.state) {
        Ok(lock) => lock,
        Err(error) => return Reply::error(1, &format!("fake-hc: {error}")),
    };
    let mut state = match State::load(&invocation.state, invocation.seed.as_deref()) {
        Ok(state) => state,
        Err(error) => return Reply::error(1, &format!("fake-hc: {error}")),
    };
    let outcome = execute(&mut state, &invocation.command, &invocation.args);
    if outcome.changed {
        if let Err(error) = state.save(&invocation.state) {
            return Reply::error(1, &format!("fake-hc: {error}"));
        }
    }
    outcome.reply
}

pub fn usage() -> &'static str {
    "fake-hc: imita los subcomandos de hc que usa EzAgent.\n\
     \n\
     Uso: fake-hc [--state RUTA] [--seed RUTA] COMANDO [ARGUMENTOS]\n\
     \n\
     Comandos:\n\
       ls [--json] [--space NOMBRE]   Terminales y su estado\n\
       spaces [--json]                Espacios y sus pestañas\n\
       tabs [--json]                  Pestañas planas\n\
       get DIRECCION                  Datos de una terminal\n\
       read DIRECCION [--lines N]     Pantalla visible\n\
       send DIRECCION TEXTO [--json]  Escribe texto\n\
       put DIRECCION RUTA [--json]    Entrega un archivo\n\
       keys DIRECCION TECLA...        Pulsa teclas (enter, esc, ctrl+t...)\n\
       kill DIRECCION                 Cierra la terminal\n\
       run --json --cwd RUTA --name NOMBRE -- ETIQUETA ARGV...\n\
       do tab.new --name NOMBRE --folder RUTA --argv JSON [--space NOMBRE]\n\
       do agent.focus --terminal DIRECCION\n\
     \n\
     El estado se guarda en --state, FAKE_HC_STATE o %TEMP%/ezagent-fake-hc/state.json.\n"
}

/// Forma de un terminal que espera `herdr.py` en `get`/`ls`.
pub fn json_terminal(terminal: &Terminal) -> Value {
    json!({
        "address": terminal.address,
        "agent": terminal.agent,
        "app": terminal.agent,
        "state": terminal.state,
        "status": {"state": terminal.state, "app": terminal.agent},
        "cwd": terminal.cwd,
        "where": terminal.where_,
        "labels": {"where": terminal.where_},
        "name": terminal.name,
        "title": terminal.title,
        "turn": terminal.turn,
    })
}

#[cfg(test)]
mod tests;

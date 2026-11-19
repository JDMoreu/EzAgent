//! Terminales PTY (ConPTY por `portable-pty`) que alimentan a xterm.js.
//!
//! Cada vista es un proceso propio. Cuatro hilos por vista, todos con nombre:
//! lector, agrupador (junta hasta 16 ms), escritor y espera de salida.
// DECISION: separar lector/agrupador/escritor evita retener candados durante E/S;
// `clone_killer` permite matar el proceso sin compartir el `Child` en espera.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{unbounded, RecvTimeoutError, Sender};
use parking_lot::Mutex;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};

use crate::texts;

/// Tamaño de una terminal en celdas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

/// Lo que se lee de una terminal (8 KiB por bloque, según §10).
pub const CHUNK: usize = 8 * 1024;
/// Ventana de silencio para agrupar bloques antes de enviarlos a la vista.
pub const BATCH_WINDOW: Duration = Duration::from_millis(16);

/// Evento que la vista pinta: bytes crudos o el aviso de salida.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PtyEvent {
    Data(Vec<u8>),
    Exited(Option<i32>),
}

/// Destino de los eventos de una vista.
pub type EventSink = Arc<dyn Fn(PtyEvent) + Send + Sync>;

/// Error al abrir o usar una terminal.
#[derive(Debug)]
pub enum PtyError {
    /// `argv[0]` no existe en PATH.
    ProgramNotFound(String),
    /// Cualquier otro fallo de `portable-pty` o del sistema.
    Failed(String),
}

impl std::fmt::Display for PtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PtyError::ProgramNotFound(program) => f.write_str(&texts::program_not_found(program)),
            PtyError::Failed(detail) => f.write_str(&texts::pty_failed(detail)),
        }
    }
}

impl std::error::Error for PtyError {}

impl From<std::io::Error> for PtyError {
    fn from(error: std::io::Error) -> Self {
        PtyError::Failed(error.to_string())
    }
}

/// Una terminal abierta.
pub struct Session {
    id: String,
    writer: Sender<Vec<u8>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    killer: Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
}

impl Session {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Encola bytes para la terminal; no bloquea a quien llama.
    pub fn write(&self, data: &[u8]) -> Result<(), PtyError> {
        self.writer
            .send(data.to_vec())
            .map_err(|_| PtyError::Failed(texts::VIEW_CLOSED.to_string()))
    }

    /// Redimensiona la vista.
    pub fn resize(&self, size: Size) -> Result<(), PtyError> {
        self.master
            .lock()
            .resize(pty_size(size))
            .map_err(|error| PtyError::Failed(error.to_string()))
    }

    /// Mata el proceso de la vista.
    pub fn close(&self) {
        if let Err(error) = self.killer.lock().kill() {
            tracing::warn!(id = %self.id, %error, "no pude cerrar la terminal");
        }
    }
}

/// Abre una terminal con `argv` (resuelto por PATH), `cwd` y variables extra.
pub fn open(
    id: &str,
    argv: &[String],
    cwd: Option<&Path>,
    env: &[(String, String)],
    size: Size,
    sink: EventSink,
) -> Result<Session, PtyError> {
    let program = argv
        .first()
        .ok_or_else(|| PtyError::Failed("argv vacío".to_string()))?;
    // DECISION: resolver argv[0] antes de abrir el PTY da el mensaje «no encontré» y
    // evita que el lanzamiento dependa del PATH que hereda el hijo.
    let resolved =
        resolve_program(program).ok_or_else(|| PtyError::ProgramNotFound(program.clone()))?;

    let mut full: Vec<OsString> = Vec::with_capacity(argv.len());
    full.push(resolved.into_os_string());
    full.extend(argv[1..].iter().map(OsString::from));
    let mut command = CommandBuilder::from_argv(full);
    if let Some(dir) = cwd {
        command.cwd(dir);
    }
    for (key, value) in env {
        command.env(key, value);
    }

    let system = native_pty_system();
    let pair = system
        .openpty(pty_size(size))
        .map_err(|error| PtyError::Failed(error.to_string()))?;
    let master = pair.master;
    let slave = pair.slave;
    let child = slave
        .spawn_command(command)
        .map_err(|error| PtyError::Failed(error.to_string()))?;
    let killer = child.clone_killer();
    let reader = master
        .try_clone_reader()
        .map_err(|error| PtyError::Failed(error.to_string()))?;
    let writer = master
        .take_writer()
        .map_err(|error| PtyError::Failed(error.to_string()))?;
    // El lado esclavo debe cerrarse o la lectura nunca llega a EOF.
    drop(slave);

    spawn_reader(id, reader, Arc::clone(&sink))?;
    let writer = spawn_writer(id, writer)?;
    spawn_waiter(id, child, sink)?;

    Ok(Session {
        id: id.to_string(),
        writer,
        master: Arc::new(Mutex::new(master)),
        killer: Arc::new(Mutex::new(killer)),
    })
}

fn spawn_reader(
    id: &str,
    mut reader: Box<dyn Read + Send>,
    sink: EventSink,
) -> Result<(), PtyError> {
    let (tx, rx) = unbounded::<Vec<u8>>();
    thread::Builder::new()
        .name(format!("ez-hub-pty-{id}"))
        .spawn(move || {
            let mut buffer = [0u8; CHUNK];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if tx.send(buffer[..count].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        })
        .map_err(PtyError::from)?;
    thread::Builder::new()
        .name(format!("ez-hub-pty-{id}-batch"))
        .spawn(move || {
            while let Ok(first) = rx.recv() {
                let mut batch = first;
                loop {
                    match rx.recv_timeout(BATCH_WINDOW) {
                        Ok(more) => batch.extend_from_slice(&more),
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => {
                            sink(PtyEvent::Data(batch));
                            return;
                        }
                    }
                }
                sink(PtyEvent::Data(batch));
            }
        })
        .map_err(PtyError::from)?;
    Ok(())
}

fn spawn_writer(id: &str, mut writer: Box<dyn Write + Send>) -> Result<Sender<Vec<u8>>, PtyError> {
    let (tx, rx) = unbounded::<Vec<u8>>();
    thread::Builder::new()
        .name(format!("ez-hub-pty-{id}-write"))
        .spawn(move || {
            while let Ok(data) = rx.recv() {
                if writer
                    .write_all(&data)
                    .and_then(|()| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        })
        .map_err(PtyError::from)?;
    Ok(tx)
}

fn spawn_waiter(
    id: &str,
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
    sink: EventSink,
) -> Result<(), PtyError> {
    let id = id.to_string();
    thread::Builder::new()
        .name(format!("ez-hub-pty-{id}-wait"))
        .spawn(move || {
            let code = match child.wait() {
                Ok(status) => Some(status.exit_code() as i32),
                Err(error) => {
                    tracing::warn!(id = %id, %error, "no pude esperar a la terminal");
                    None
                }
            };
            sink(PtyEvent::Exited(code));
        })
        .map_err(PtyError::from)?;
    Ok(())
}

fn pty_size(size: Size) -> PtySize {
    PtySize {
        rows: size.rows.max(1),
        cols: size.cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Busca un programa en PATH (con PATHEXT en Windows) o comprueba una ruta.
pub fn resolve_program(program: &str) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file().then_some(path.to_path_buf());
    }
    let dirs = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&dirs) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        if path.extension().is_none() {
            for extension in windows_extensions() {
                let candidate = dir.join(format!("{program}{extension}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn windows_extensions() -> Vec<String> {
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(|extension| extension.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::Receiver;

    fn sink_channel() -> (EventSink, Receiver<PtyEvent>) {
        let (tx, rx) = unbounded();
        let sink: EventSink = Arc::new(move |event| {
            let _ = tx.send(event);
        });
        (sink, rx)
    }

    /// ConPTY consulta la posición del cursor (`ESC[6n`) y no suelta la salida
    /// hasta que el cliente contesta; xterm.js lo hace solo, aquí va a mano.
    fn answer_dsr(session: &Session, bytes: &[u8]) {
        if bytes.windows(4).any(|window| window == b"\x1b[6n") {
            let _ = session.write(b"\x1b[1;1R");
        }
    }

    #[test]
    fn resolves_a_program_from_path() {
        let found = resolve_program("hostname").or_else(|| resolve_program("hostname.exe"));
        assert!(found.is_some(), "hostname debería estar en PATH");
    }

    #[test]
    fn reports_a_missing_program() {
        assert!(resolve_program("ez-agent-no-existe-xyz").is_none());
        let (sink, _rx) = sink_channel();
        let error = open(
            "t-missing",
            &["ez-agent-no-existe-xyz".to_string()],
            None,
            &[],
            Size { cols: 80, rows: 24 },
            sink,
        )
        .err()
        .expect("debe fallar");
        assert!(matches!(error, PtyError::ProgramNotFound(_)));
        assert!(error.to_string().contains("No encontré"));
    }

    #[test]
    fn runs_hostname_and_reports_output_and_exit_code() {
        let (sink, rx) = sink_channel();
        let session = open(
            "t-hostname",
            &["hostname".to_string()],
            None,
            &[],
            Size { cols: 80, rows: 24 },
            sink,
        )
        .expect("abrir terminal");
        let mut output = Vec::new();
        let mut exit = None;
        while let Ok(event) = rx.recv_timeout(Duration::from_secs(20)) {
            match event {
                PtyEvent::Data(bytes) => {
                    answer_dsr(&session, &bytes);
                    output.extend_from_slice(&bytes);
                }
                PtyEvent::Exited(code) => {
                    exit = code;
                    break;
                }
            }
        }
        session.close();
        assert_eq!(
            exit,
            Some(0),
            "salida: {:?}",
            String::from_utf8_lossy(&output)
        );
        let expected = std::env::var("COMPUTERNAME").unwrap_or_default();
        if !expected.is_empty() {
            let text = String::from_utf8_lossy(&output).to_lowercase();
            assert!(text.contains(&expected.to_lowercase()), "salida: {text:?}");
        }
    }

    #[test]
    fn write_reaches_an_interactive_shell() {
        let (sink, rx) = sink_channel();
        let session = open(
            "t-write",
            &[
                "powershell.exe".to_string(),
                "-NoLogo".to_string(),
                "-NoProfile".to_string(),
            ],
            None,
            &[],
            Size {
                cols: 100,
                rows: 30,
            },
            sink,
        )
        .expect("abrir powershell");
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let mut text = String::new();
        let mut sent = false;
        while std::time::Instant::now() < deadline {
            if let Ok(PtyEvent::Data(bytes)) = rx.recv_timeout(Duration::from_millis(500)) {
                answer_dsr(&session, &bytes);
                if !sent {
                    session
                        .write(b"Write-Output ez-hub-ok\r\n")
                        .expect("escribir");
                    sent = true;
                }
                text.push_str(&String::from_utf8_lossy(&bytes));
                if text.contains("ez-hub-ok") {
                    break;
                }
            }
        }
        session.close();
        assert!(text.contains("ez-hub-ok"), "salida: {text:?}");
    }
}

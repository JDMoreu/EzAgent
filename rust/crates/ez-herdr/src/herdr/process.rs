//! Subprocesos de Herdr: `hc` y `herdr-app` con salida capturada y timeout.
//!
//! Equivale a `subprocess.run(..., capture_output=True, text=True,
//! encoding="utf-8", errors="replace", timeout=…)` sin shell: argumentos en
//! lista y entorno heredado (o heredado sin `HC_TERMINAL_ID`).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Resultado de un subproceso terminado.
#[derive(Debug, Clone)]
pub struct Completed {
    pub stdout: String,
    pub stderr: String,
    pub returncode: i64,
}

/// Por qué no se obtuvo un `Completed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// `FileNotFoundError`: el ejecutable no existe.
    NotFound,
    /// `subprocess.TimeoutExpired`.
    Timeout,
    /// Cualquier otro `OSError`.
    Os(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("ejecutable no encontrado"),
            Self::Timeout => formatter.write_str("tiempo de espera agotado"),
            Self::Os(message) => formatter.write_str(message),
        }
    }
}

fn drain<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        buffer
    })
}

/// Decodifica como `text=True, encoding="utf-8", errors="replace"`.
pub fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// `subprocess.run(argv, env=…, capture_output=True, timeout, CREATE_NO_WINDOW)`.
///
/// `anonymous` reproduce `_anonymous_env()`: se hereda el entorno sin
/// `HC_TERMINAL_ID` (la identidad de la terminal que ejecuta EzAgent).
pub fn run(argv: &[String], timeout_s: f64, anonymous: bool) -> Result<Completed, RunError> {
    let Some((program, rest)) = argv.split_first() else {
        return Err(RunError::Os("comando vacío".into()));
    };
    let mut command = Command::new(program);
    command
        .args(rest)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if anonymous {
        command.env_remove("HC_TERMINAL_ID");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => RunError::NotFound,
        _ => RunError::Os(error.to_string()),
    })?;
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let join = |handle: Option<std::thread::JoinHandle<Vec<u8>>>| {
        handle
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default()
    };
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_s);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(Completed {
                    stdout: decode(&join(stdout)),
                    stderr: decode(&join(stderr)),
                    returncode: i64::from(status.code().unwrap_or(-1)),
                });
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Timeout);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => return Err(RunError::Os(error.to_string())),
        }
    }
}

/// `shutil.which`: el ejecutable real de un nombre o ruta del PATH.
///
/// shortcut: no mira el directorio actual cuando falta en PATH ni acepta las
/// extensiones implícitas que Windows agrega al PATH vacío. Los valores de
/// `voice.json` son rutas o nombres en PATH, así que no se alcanza.
pub fn which(program: &str) -> Option<PathBuf> {
    if program.is_empty() {
        return None;
    }
    let path = Path::new(program);
    let explicit = path.is_absolute() || path.components().count() > 1;
    if explicit {
        return candidate(path);
    }
    let dirs = std::env::var_os("PATH")?;
    std::env::split_paths(&dirs).find_map(|folder| candidate(&folder.join(path)))
}

fn candidate(path: &Path) -> Option<PathBuf> {
    if is_executable(path) {
        return Some(path.to_path_buf());
    }
    #[cfg(windows)]
    {
        for extension in path_extensions() {
            let mut name = path.as_os_str().to_os_string();
            name.push(&extension);
            let variant = PathBuf::from(name);
            if is_executable(&variant) {
                return Some(variant);
            }
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    true
}

#[cfg(windows)]
fn path_extensions() -> Vec<std::ffi::OsString> {
    const DEFAULT_EXTENSIONS: &str = ".COM;.EXE;.BAT;.CMD";
    let raw = std::env::var("PATHEXT").unwrap_or_else(|_| DEFAULT_EXTENSIONS.to_string());
    let mut extensions = vec![std::ffi::OsString::new()];
    extensions.extend(
        raw.split(';')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(std::ffi::OsString::from),
    );
    extensions
}

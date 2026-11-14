//! Subprocesos de `tools.d` y de las skills: entorno limpio y salida capturada con timeout.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const ENV_NAMES: [&str; 3] = ["PATH", "SYSTEMROOT", "TEMP"];

/// Entorno mínimo para subprocess: solo `PATH`, `SYSTEMROOT` y `TEMP`.
/// Nunca se heredan claves de API, tokens ni variables del proyecto.
pub fn clean_environment() -> Vec<(String, String)> {
    ENV_NAMES
        .iter()
        .filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| ((*name).to_string(), value))
        })
        .collect()
}

/// Resultado de un subproceso terminado.
#[derive(Debug, Clone)]
pub struct Completed {
    pub stdout: String,
    pub stderr: String,
    pub returncode: i64,
}

/// Por qué no se obtuvo un `Completed`.
#[derive(Debug, Clone)]
pub enum RunError {
    /// `subprocess.TimeoutExpired`, con la salida que alcanzó a escribir.
    Timeout { stdout: String },
    /// `OSError` al lanzar el proceso.
    Os(String),
}

fn drain<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = reader.read_to_end(&mut buffer);
        buffer
    })
}

/// Decodifica como `text=True, encoding="utf-8", errors="replace"` (finales de línea universales).
fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

/// `subprocess.run(argv, shell=False, cwd, env, timeout, capture_output=True)`.
///
/// `argv` nunca pasa por un shell; el entorno se reemplaza por completo.
pub fn run_captured(
    argv: &[String],
    cwd: &Path,
    env: &[(String, String)],
    timeout_s: f64,
) -> Result<Completed, RunError> {
    let Some((program, rest)) = argv.split_first() else {
        return Err(RunError::Os("comando vacío".into()));
    };
    let mut child = Command::new(program)
        .args(rest)
        .current_dir(cwd)
        .env_clear()
        .envs(
            env.iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| RunError::Os(error.to_string()))?;
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_s.max(0.0));
    let join = |handle: Option<std::thread::JoinHandle<Vec<u8>>>| {
        handle
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default()
    };
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
                return Err(RunError::Timeout {
                    stdout: decode(&join(stdout)),
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => return Err(RunError::Os(error.to_string())),
        }
    }
}

//! R8: ejecuta los comandos de los criterios sin shell, con tope de tiempo.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::criteria::{Criteria, Criterion};
use crate::texts;

/// Caracteres de salida que se conservan.
pub const OUTPUT_TAIL: usize = 4000;
/// Intervalo de `try_wait` mientras el comando corre.
const POLL: Duration = Duration::from_millis(50);
/// Tope para abandonar los lectores de salida (§14.7).
const READER_GRACE: Duration = Duration::from_secs(2);

/// Resultado de un criterio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckRecord {
    pub id: String,
    pub descripcion: String,
    pub comando: String,
    pub ok: bool,
    pub codigo: Option<i32>,
    pub segundos: f64,
    /// Últimos `OUTPUT_TAIL` caracteres de salida y error.
    pub salida: String,
    pub motivo: Option<String>,
}

/// Ejecuta todos los criterios, uno por hilo, y devuelve los resultados en orden.
pub fn run_all(folder: &Path, criteria: &Criteria) -> Vec<CheckRecord> {
    let mut handles = Vec::with_capacity(criteria.criteria.len());
    for criterion in &criteria.criteria {
        let folder = folder.to_path_buf();
        let criterion = criterion.clone();
        let handle = std::thread::Builder::new()
            .name("ez-arbiter-check".into())
            .spawn(move || run_one(&folder, &criterion))
            .expect("crear el hilo de verificación");
        handles.push(handle);
    }
    handles
        .into_iter()
        .map(|handle| handle.join().unwrap_or_else(|_| failed_placeholder()))
        .collect()
}

fn failed_placeholder() -> CheckRecord {
    CheckRecord {
        id: String::new(),
        descripcion: String::new(),
        comando: String::new(),
        ok: false,
        codigo: None,
        segundos: 0.0,
        salida: String::new(),
        motivo: Some("el hilo de verificación falló".to_string()),
    }
}

/// Ejecuta un criterio con su carpeta, su código esperado y su tope.
pub fn run_one(folder: &Path, criterion: &Criterion) -> CheckRecord {
    let started = Instant::now();
    let mut record = CheckRecord {
        id: criterion.id.clone(),
        descripcion: criterion.descripcion.clone(),
        comando: criterion.command_text(),
        ok: false,
        codigo: None,
        segundos: 0.0,
        salida: String::new(),
        motivo: None,
    };
    let working_dir = match criterion.working_dir(folder) {
        Ok(path) => path,
        Err(reason) => {
            record.motivo = Some(reason);
            return record;
        }
    };
    let mut command = crate::exec::command(&criterion.comando[0]);
    command
        .args(&criterion.comando[1..])
        .current_dir(&working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            record.motivo = Some(texts::command_failed(&error.to_string()));
            return record;
        }
    };
    let (tx, rx) = mpsc::channel();
    spawn_reader(child.stdout.take(), "stdout", tx.clone());
    spawn_reader(child.stderr.take(), "stderr", tx);
    let deadline = started + Duration::from_secs(criterion.segundos);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() >= deadline => {
                kill_tree(&mut child);
                let status = child.wait().ok();
                timed_out = true;
                break status;
            }
            Ok(None) => std::thread::sleep(POLL),
            Err(error) => {
                record.motivo = Some(texts::command_failed(&error.to_string()));
                kill_tree(&mut child);
                let _ = child.wait();
                break None;
            }
        }
    };
    let (stdout, stderr) = collect_output(rx);
    record.segundos = started.elapsed().as_secs_f64();
    record.codigo = status.and_then(|status| status.code());
    let full = format!("{stdout}{stderr}");
    record.salida = tail(&full, OUTPUT_TAIL);
    record.motivo = judge(criterion, &record.codigo, &full, timed_out);
    record.ok = record.motivo.is_none();
    record
}

/// Mata el proceso y sus hijos al vencer el tope (diseño §14.7).
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let _ = crate::exec::command("taskkill")
            .args(["/T", "/F", "/PID"])
            .arg(child.id().to_string())
            .output();
    }
    let _ = child.kill();
}

fn spawn_reader<R: Read + Send + 'static>(
    reader: Option<R>,
    label: &'static str,
    tx: mpsc::Sender<(&'static str, Vec<u8>)>,
) {
    let Some(mut reader) = reader else {
        return;
    };
    std::thread::Builder::new()
        .name(format!("ez-arbiter-check-{label}"))
        .spawn(move || {
            let mut buffer = Vec::new();
            let _ = reader.read_to_end(&mut buffer);
            let _ = tx.send((label, buffer));
        })
        .expect("crear el hilo lector de verificación");
}

/// Lee las salidas hasta que responden los dos lectores o vence el margen:
/// nunca se espera sin límite a un lector colgado (§14.7).
fn collect_output(rx: mpsc::Receiver<(&'static str, Vec<u8>)>) -> (String, String) {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let deadline = Instant::now() + READER_GRACE;
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        let Ok((label, bytes)) = rx.recv_timeout(remaining) else {
            break;
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if label == "stdout" {
            stdout.push_str(&text);
        } else {
            stderr.push_str(&text);
        }
    }
    (stdout, stderr)
}

/// Veredicto del criterio: `None` si pasó, o el motivo del fallo.
pub fn judge(
    criterion: &Criterion,
    code: &Option<i32>,
    output: &str,
    timed_out: bool,
) -> Option<String> {
    if timed_out {
        return Some(texts::timed_out(criterion.segundos));
    }
    match code {
        Some(code) if *code == criterion.codigo => {}
        other => return Some(texts::wrong_code(*other, criterion.codigo)),
    }
    if let Some(needle) = &criterion.contiene {
        if !output.contains(needle) {
            return Some(texts::lacks_text(needle));
        }
    }
    None
}

/// Últimos `max` caracteres (no bytes) de un texto.
pub fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    text.chars().skip(count - max).collect()
}

#[cfg(test)]
mod tests {
    use super::tail;

    #[test]
    fn tail_counts_characters_not_bytes() {
        let text = "áéíóú";
        assert_eq!(tail(text, 3), "íóú");
        assert_eq!(tail(text, 10), text);
    }
}

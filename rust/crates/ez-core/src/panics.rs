//! Pánicos: mensaje, ubicación y rastro en disco (DX-1d).
//!
//! Un pánico que cruza un callback de Windows (`extern "system"`) no puede
//! desenrollarse: Rust aborta el proceso con `0xc0000409` y no queda rastro.
//! Este módulo deja el rastro:
//!
//! - `install_hook` deja un hook permanente que escribe la línea del pánico
//!   (mensaje y ubicación) en el archivo de registro **antes** de abortar.
//! - `catch_panic` atrapa un pánico en un borde de Windows y lo registra.
//!
//! DECISION: la línea se escribe con `std::fs` (síncrono) y no con `tracing`:
//! el destino del registro usa `tracing_appender::non_blocking`, así que al
//! abortar el proceso el búfer se pierde. Es justo el caso que nos duele.

use std::any::Any;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Prefijo de los archivos de registro que crea la app.
const LOG_PREFIX: &str = "ezagent.log";

/// Carpeta del registro a archivo (la fija `install_hook`).
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// El hook ya está instalado en este proceso.
static INSTALLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Ubicación del último pánico de este hilo: la captura el hook para que
    /// `catch_panic` pueda registrarla (el `catch_unwind` no la da).
    static LAST_LOCATION: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Extrae el mensaje de un pánico (`Box<dyn Any + Send>`).
pub fn message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else {
        "pánico con carga útil desconocida".to_string()
    }
}

/// Formatea la ubicación de un pánico como `archivo:línea:columna`.
pub fn location(info: &std::panic::PanicHookInfo<'_>) -> Option<String> {
    info.location()
        .map(|spot| format!("{}:{}:{}", spot.file(), spot.line(), spot.column()))
}

/// Instala el hook permanente de pánicos en la carpeta de registro de la app.
///
/// Es idempotente: la segunda llamada no cambia nada (así el binario y el
/// inicio del registro pueden llamarlo los dos).
pub fn install_hook() {
    install_hook_in(&crate::dirs::logs_dir());
}

/// Instala el hook permanente de pánicos en la carpeta dada.
///
/// DECISION: la carpeta es un parámetro para que la prueba use una temporal y
/// no toque el registro de la persona.
pub fn install_hook_in(dir: &Path) {
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = LOG_DIR.set(dir.to_path_buf());
    let previous = std::panic::take_hook();
    let dir = dir.to_path_buf();
    std::panic::set_hook(Box::new(move |info| {
        let message = message(info.payload());
        let location = location(info);
        remember(location.clone());
        let _ = write_record(&dir, &message, location.as_deref());
        previous(info);
    }));
}

/// Ejecuta `body` atrapando un pánico: ningún pánico cruza un callback de
/// Windows. Devuelve `None` si hubo pánico (queda registrado).
pub fn catch_panic<T>(body: impl FnOnce() -> T) -> Option<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(value) => Some(value),
        Err(payload) => {
            let message = message(payload.as_ref());
            let location = take_location();
            // El hook permanente ya escribió la línea en el archivo; aquí solo
            // falta el registro del `tracing` (con el candado del logger).
            tracing::error!(
                "pánico atrapado en un borde de Windows: {message} ({})",
                location.as_deref().unwrap_or("ubicación desconocida")
            );
            None
        }
    }
}

/// Escribe la línea de pánico en el archivo de registro de la app.
///
/// Devuelve la ruta del archivo escrito (o el error, si no hay carpeta de
/// registro: el hook todavía no se instaló).
pub fn record(message: &str, location: Option<&str>) -> std::io::Result<PathBuf> {
    let Some(dir) = LOG_DIR.get() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "la carpeta de registro no está fijada",
        ));
    };
    write_record(dir, message, location)
}

/// Escribe la línea de pánico en el archivo diario del registro (`<dir>`).
///
/// El archivo es el mismo que escribe la app (`ezagent.log.<fecha>`): si ya
/// existe, se añade al final; si no, se crea con la fecha de hoy.
pub fn write_record(dir: &Path, message: &str, location: Option<&str>) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = daily_file(dir);
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let spot = location.unwrap_or("ubicación desconocida");
    writeln!(file, "[{stamp}] PÁNICO: {message} ({spot})")?;
    file.flush()?;
    Ok(path)
}

/// El archivo diario del registro: el que ya existe, o el de hoy.
fn daily_file(dir: &Path) -> PathBuf {
    if let Some(path) = existing_daily(dir) {
        return path;
    }
    dir.join(format!(
        "{LOG_PREFIX}.{}",
        chrono::Utc::now().format("%Y-%m-%d")
    ))
}

/// El archivo diario más reciente que ya existe en la carpeta.
fn existing_daily(dir: &Path) -> Option<PathBuf> {
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !is_daily_name(name) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        if newest.as_ref().is_none_or(|(stamp, _)| modified > *stamp) {
            newest = Some((modified, path));
        }
    }
    newest.map(|(_, path)| path)
}

/// ¿El nombre es el de un archivo diario del registro (`ezagent.log.<fecha>`)?
fn is_daily_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(&format!("{LOG_PREFIX}.")) else {
        return false;
    };
    let bytes = suffix.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

/// Guarda la ubicación del pánico de este hilo.
fn remember(location: Option<String>) {
    LAST_LOCATION.with(|slot| {
        *slot.borrow_mut() = location;
    });
}

/// Toma (y limpia) la ubicación del último pánico de este hilo.
fn take_location() -> Option<String> {
    LAST_LOCATION.with(|slot| slot.borrow_mut().take())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_reads_strings_and_strs() {
        let owned: Box<dyn Any + Send> = Box::new("texto".to_string());
        assert_eq!(message(owned.as_ref()), "texto");
        let borrowed: Box<dyn Any + Send> = Box::new("préstamo");
        assert_eq!(message(borrowed.as_ref()), "préstamo");
        let other: Box<dyn Any + Send> = Box::new(7_u8);
        assert!(message(other.as_ref()).contains("desconocida"));
    }

    #[test]
    fn record_is_written_to_the_daily_file() {
        let dir = std::env::temp_dir().join(format!("ez-core-panics-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = write_record(&dir, "mensaje de prueba", Some("archivo.rs:1:2")).unwrap();
        assert!(path.exists());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("mensaje de prueba"), "{text}");
        assert!(text.contains("archivo.rs:1:2"), "{text}");
        // Una segunda línea va al mismo archivo (no se pisa).
        write_record(&dir, "otra línea", None).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("otra línea"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn daily_names_are_recognized() {
        assert!(is_daily_name("ezagent.log.2026-11-19"));
        assert!(!is_daily_name("ezagent.log.panic"));
        assert!(!is_daily_name("ezagent.log"));
        assert!(!is_daily_name("engine.log.2026-11-19"));
        assert!(!is_daily_name("ezagent.log.2026-11-1"));
    }

    #[test]
    fn the_existing_daily_file_is_reused() {
        let dir = std::env::temp_dir().join(format!("ez-core-panics-daily-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let older = dir.join("ezagent.log.2020-01-01");
        std::fs::write(&older, "viejo").unwrap();
        let newer = dir.join("ezagent.log.2021-01-01");
        std::fs::write(&newer, "nuevo").unwrap();
        let found = existing_daily(&dir).unwrap();
        assert!(found.ends_with("2021-01-01"), "{}", found.display());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_caught_panic_does_not_kill_the_process() {
        let result = catch_panic(|| -> i32 { panic!("pánico de prueba") });
        assert!(result.is_none());
        // El proceso sigue vivo: esto es justo lo que se pide de los callbacks.
        assert_eq!(catch_panic(|| 7), Some(7));
    }
}

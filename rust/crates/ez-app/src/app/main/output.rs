//! Salida de consola con los mismos fines de línea que Python.

use std::io::Write;
use std::path::Path;

/// Escribe como el `print` de Python: en Windows el texto traduce a CRLF.
pub(super) fn write_stdout(text: &str) {
    let text = if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    };
    let _ = std::io::stdout().lock().write_all(text.as_bytes());
}

/// Escribe un error con el mismo fin de línea que `sys.stderr` en Python.
pub(super) fn write_stderr(text: &str) {
    let text = if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    };
    let _ = std::io::stderr().lock().write_all(text.as_bytes());
}

/// `print(line)` de Python.
pub(super) fn print_line(text: &str) {
    write_stdout(&format!("{text}\n"));
}

/// `str(pathlib.Path)` en Windows: separadores con barra invertida.
pub(super) fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        text.replace('/', "\\")
    } else {
        text.into_owned()
    }
}

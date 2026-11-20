//! Program output of the test, written through `Write` (never `println!`).

use std::io::Write;

/// Writes one line to stdout.
pub fn line(text: &str) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "{text}");
}

/// Writes one line to stderr.
pub fn error(text: &str) {
    let stderr = std::io::stderr();
    let mut err = stderr.lock();
    let _ = writeln!(err, "{text}");
}

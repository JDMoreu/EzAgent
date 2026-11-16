//! Partes puras de la aprobación como transacción (§10.7): huella S1/S2 y
//! comprobación de S3. El envío de teclas y el contador de autorización viven
//! en el bucle (A3).

use std::sync::OnceLock;

use ez_core::pyvalue::{fold_accents, py_splitlines, py_strip};
use regex::Regex;

use super::{classify, DialogKind};

/// Espera sin cambios de pantalla antes de aprobar.
pub const APPROVE_SETTLE_S: f64 = 1.5;

/// Líneas de pantalla que forman la huella del antirrebote.
pub const FINGERPRINT_LINES: usize = 20;

fn grant_marker() -> &'static Regex {
    static GRANT: OnceLock<Regex> = OnceLock::new();
    GRANT.get_or_init(|| {
        Regex::new(
            r"(?i)\balways\b|\bsiempre\b|don'?t ask again|no (?:vuelvas a |volver a )?preguntar|for this session|para esta sesion|\ball future\b",
        )
        .expect("patrón fijo válido")
    })
}

/// Huella de las últimas líneas de pantalla para no aprobar dos veces.
///
/// DECISION: se ignoran las líneas vacías para que el relleno del TUI no
/// cambie la huella.
pub fn screen_fingerprint(screen: &str) -> String {
    let lines: Vec<String> = py_splitlines(screen)
        .into_iter()
        .map(py_strip)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    let start = lines.len().saturating_sub(FINGERPRINT_LINES);
    lines[start..].join("\n")
}

/// S2 es la misma pantalla que S1: el diálogo está quieto.
pub fn same_dialog(first: &str, second: &str) -> bool {
    screen_fingerprint(first) == screen_fingerprint(second)
}

fn shows_permanent_grant(screen: &str) -> bool {
    let marker = grant_marker();
    py_splitlines(screen).iter().any(|line| {
        let text = py_strip(line);
        !text.is_empty() && marker.is_match(&fold_accents(text))
    })
}

/// S3 confirma la aprobación: el diálogo desapareció y no quedó una concesión
/// permanente. Devuelve el motivo del fallo si no se cumple.
pub fn approval_granted(screen: &str) -> Result<(), &'static str> {
    if screen_fingerprint(screen).is_empty() {
        return Err("no pude leer la pantalla después de aprobar");
    }
    if classify(screen).kind != DialogKind::None {
        return Err("el diálogo de permiso sigue en pantalla");
    }
    if shows_permanent_grant(screen) {
        return Err("la pantalla muestra una concesión permanente");
    }
    Ok(())
}

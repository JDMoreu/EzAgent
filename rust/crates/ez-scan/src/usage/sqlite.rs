//! SQLite en solo lectura, por URI `mode=ro`, viendo el WAL.
//!
//! Nunca se usa `immutable=1`: los datos de hoy pueden estar todavía en el
//! `-wal` y una conexión inmutable no los vería.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

pub(crate) fn open_readonly(path: &Path) -> Result<Connection, String> {
    let uri = file_uri(path);
    let connection = Connection::open_with_flags(
        uri.as_str(),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|error| format!("{}: {error}", path.display()))?;
    connection
        .busy_timeout(Duration::from_millis(2_000))
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch("PRAGMA query_only = 1;")
        .map_err(|error| error.to_string())?;
    Ok(connection)
}

/// `file:///C:/ruta?mode=ro` (con `%20` y compañía para lo que no sea seguro).
pub(crate) fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let encoded = percent_encode(&text);
    if encoded.starts_with('/') {
        format!("file://{encoded}?mode=ro")
    } else {
        format!("file:///{encoded}?mode=ro")
    }
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        let safe =
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':');
        if safe {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// ¿Existe la tabla? (solo `sqlite_master`, nunca su contenido).
pub(crate) fn has_table(connection: &Connection, name: &str) -> bool {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |_| Ok(()),
        )
        .is_ok()
}

/// El mayor `time_created` de una tabla, si existe.
pub(crate) fn max_time(connection: &Connection, table: &str) -> Option<i64> {
    if !has_table(connection, table) {
        return None;
    }
    let sql = format!("SELECT COALESCE(MAX(time_created), 0) FROM {table}");
    connection
        .query_row(&sql, [], |row| row.get::<_, i64>(0))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_uri_es_de_solo_lectura_y_ve_el_wal() {
        let uri = file_uri(Path::new(r"C:\Users\una persona\base.db"));
        assert!(
            uri.starts_with("file:///C:/Users/una%20persona/base.db?mode=ro"),
            "{uri}"
        );
        assert!(!uri.contains("immutable"));
        let unix = file_uri(Path::new("/tmp/base.db"));
        assert_eq!(unix, "file:///tmp/base.db?mode=ro");
    }
}

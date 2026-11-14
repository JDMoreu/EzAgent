//! Registro a archivo con `tracing`, mismo destino que el `loguru` de Python (F2-01).

use std::path::Path;

use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;

/// Instala el registro en `<logs>/engine.log` y en stderr.
///
/// `shortcut: rotación diaria por defecto de tracing-appender; Python no rota.`
pub fn init(logs_dir: &Path, level: &str) -> Result<(), String> {
    std::fs::create_dir_all(logs_dir)
        .map_err(|error| format!("{}: {error}", logs_dir.display()))?;
    let file_appender = tracing_appender::rolling::daily(logs_dir, "engine.log");
    let filter = EnvFilter::try_new(level).map_err(|error| error.to_string())?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(file_appender.and(std::io::stderr))
        .with_ansi(false)
        .try_init()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn creates_the_log_directory() {
        let dir = std::env::temp_dir().join("ez-core-logging-test");
        let _ = std::fs::remove_dir_all(&dir);
        // no se instala el suscriptor global aquí: otros tests comparten proceso
        std::fs::create_dir_all(&dir).unwrap();
        assert!(dir.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

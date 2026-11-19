//! DX-1b: el registro de la app va a un archivo que rota (paso 7).
//!
//! Antes el registro solo salía por stderr y la app se arranca sin consola:
//! un congelamiento no dejaba ningún rastro (DX-1, paso 7). Ahora el registro
//! también va a `%APPDATA%\EzAgent\logs\ezagent.log.<fecha>`, con rotación
//! diaria y siete días de historia.
//!
//! Las pruebas usan una carpeta temporal propia: no tocan el registro real de
//! la persona ni el de la app instalada.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use ez_app::app::main::{init_logging_in, prune_logs};

/// Carpeta temporal propia de la prueba.
fn temp_dir(name: &str) -> PathBuf {
    let unique = format!(
        "ezagent-log-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).expect("the temporary folder was created");
    dir
}

/// Borra la carpeta temporal de la prueba.
fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

/// El registro se escribe en un archivo de la carpeta pedida.
#[test]
fn the_log_is_written_to_a_file_in_the_given_folder() {
    let dir = temp_dir("write");
    assert!(init_logging_in(&dir), "the folder could not be prepared");
    let marker = "dx-1b-marca-del-registro";
    tracing::info!("{marker}");

    // The writer is asynchronous: the file appears a moment later.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut found = None;
    while std::time::Instant::now() < deadline {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("ezagent.log") {
                    continue;
                }
                let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
                if text.contains(marker) {
                    found = Some(name);
                    break;
                }
            }
        }
        if found.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    cleanup(&dir);
    assert!(
        found.is_some(),
        "the log file was not written in {}",
        dir.display()
    );
}

/// La rotación daily escribe un archivo por día y la poda borra los viejos.
#[test]
fn the_old_logs_are_pruned_and_the_others_are_kept() {
    let dir = temp_dir("prune");
    let old = dir.join("ezagent.log.2020-01-01");
    let fresh = dir.join("ezagent.log.2026-01-01");
    let other = dir.join("latency.jsonl");
    for path in [&old, &fresh, &other] {
        std::fs::write(path, "x").expect("the file was written");
    }
    // The old file is a week and a half old.
    let old_time = SystemTime::now() - Duration::from_secs(10 * 24 * 60 * 60);
    std::fs::File::options()
        .write(true)
        .open(&old)
        .expect("the file was opened")
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(old_time)
                .set_accessed(old_time),
        )
        .expect("the time was set");

    prune_logs(&dir, 7);
    let kept: Vec<String> = std::fs::read_dir(&dir)
        .expect("the folder was read")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    cleanup(&dir);
    assert!(
        !kept.contains(&"ezagent.log.2020-01-01".to_string()),
        "{kept:?}"
    );
    assert!(
        kept.contains(&"ezagent.log.2026-01-01".to_string()),
        "{kept:?}"
    );
    // Only the log files of the app are pruned.
    assert!(kept.contains(&"latency.jsonl".to_string()), "{kept:?}");
}

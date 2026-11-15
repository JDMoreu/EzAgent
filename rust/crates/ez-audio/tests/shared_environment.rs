//! `ort` solo admite un entorno por proceso: Smart Turn y el TTS lo comparten.

use std::path::PathBuf;

use ez_audio::turn::shared_environment;

#[test]
fn the_environment_is_created_once_and_reused() {
    let Some(library) = std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from) else {
        eprintln!("shared_environment: omitida, falta ORT_DYLIB_PATH");
        return;
    };
    if !library.is_file() {
        eprintln!(
            "shared_environment: omitida, {} no existe",
            library.display()
        );
        return;
    }
    let first = shared_environment(&library).expect("primer entorno");
    let second = shared_environment(&library).expect("segundo entorno");
    assert!(std::ptr::eq(first, second));
}

//! `ezagent-hub`: la ventana del espacio de trabajo. `--demo` abre el modo demostración.
//! `--demo --selfcheck <ruta.json>` guarda el informe de la vista y sale con su veredicto.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

fn main() {
    ez_hub::init_tracing();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let demo = args.iter().any(|arg| arg == "--demo");
    let selfcheck = args
        .iter()
        .position(|arg| arg == "--selfcheck")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from);
    tracing::info!(demo, selfcheck = selfcheck.is_some(), "abriendo el hub");
    if let Err(error) = ez_hub::run(demo, selfcheck) {
        tracing::error!(%error, "la ventana no pudo abrirse");
        std::process::exit(1);
    }
}

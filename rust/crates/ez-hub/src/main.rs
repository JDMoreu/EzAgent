//! `ezagent-hub`: la ventana del espacio de trabajo. `--demo` abre el modo demostración.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ez_hub::init_tracing();
    let demo = std::env::args().skip(1).any(|arg| arg == "--demo");
    tracing::info!(demo, "abriendo el hub");
    if let Err(error) = ez_hub::run(demo) {
        tracing::error!(%error, "la ventana no pudo abrirse");
        std::process::exit(1);
    }
}

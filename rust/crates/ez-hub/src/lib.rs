//! Ventana del espacio de trabajo (Tauri). Diseño: `docs/designs/espacio-elefante.md` §10.
//!
//! En esta ola la ventana no habla con el árbitro ni con herdr-core: abre espacios,
//! terminales PTY propias y un modo demostración en memoria.

pub mod commands;
pub mod hub;
pub mod input;
pub mod pty;
pub mod space;
pub mod texts;

#[cfg(test)]
mod testutil;

pub use commands::AppState;
pub use ez_core::dirs;

/// Inicializa `tracing` (respeta `RUST_LOG`; por omisión `info`).
pub fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

/// Arranca la ventana; `demo` usa un espacio en memoria.
pub fn run(demo: bool) -> tauri::Result<()> {
    tauri::Builder::default()
        .manage(commands::AppState::new(demo))
        .invoke_handler(tauri::generate_handler![
            commands::spaces_list,
            commands::space_create,
            commands::view_open,
            commands::view_write,
            commands::view_resize,
            commands::view_close,
            commands::elephant_input,
            commands::hub_state,
            commands::card_action,
        ])
        .run(tauri::generate_context!())
}

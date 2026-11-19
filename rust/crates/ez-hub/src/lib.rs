//! Ventana del espacio de trabajo (Tauri). Diseño: `docs/designs/espacio-elefante.md` §10.
//!
//! En esta ola la ventana no habla con el árbitro ni con herdr-core: abre espacios,
//! terminales PTY propias y un modo demostración en memoria.

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

pub mod commands;
pub mod demo;
pub mod hub;
pub mod input;
pub mod pty;
pub mod runtime;
pub mod runtime_commands;
pub mod selfcheck;
pub mod space;
pub mod texts;
pub mod voicenote;

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

/// Arranca la ventana; `demo` usa un espacio en memoria y `selfcheck` activa la
/// autocomprobación de la vista, que escribe su informe en esa ruta.
pub fn run(demo: bool, selfcheck: Option<PathBuf>) -> tauri::Result<()> {
    let state = commands::AppState::with_selfcheck(demo, selfcheck.clone());
    let changed = state.runtime().map(|runtime| runtime.subscribe_changed());
    // El informe se escribe antes de cerrar; si la ruta sigue sin existir en 20 s,
    // la vista no avisó. El hilo no retiene ningún candado durante la espera.
    if let Some(path) = selfcheck {
        thread::Builder::new()
            .name("ez-hub-selfcheck".to_string())
            .spawn(move || watchdog(path))
            .expect("hilo de vigilancia de la autocomprobación");
    }
    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::spaces_list,
            commands::space_create,
            commands::view_open,
            commands::view_write,
            commands::view_send,
            commands::view_resize,
            commands::view_close,
            commands::elephant_input,
            runtime_commands::hub_state,
            runtime_commands::card_action,
            commands::runtime_status,
            commands::space_elephant,
            commands::space_fish,
            commands::elephant_send,
            runtime_commands::main_agent_create,
            runtime_commands::main_agent_remove,
            runtime_commands::space_tree,
            runtime_commands::node_address,
            runtime_commands::agent_send,
            runtime_commands::roles_list,
            runtime_commands::inventory,
            commands::voice_note_start,
            commands::voice_note_stop,
            commands::voice_insert,
            commands::selfcheck_enabled,
            commands::selfcheck_report,
        ])
        .setup(move |app| {
            use tauri::Emitter;
            if let Some(receiver) = changed {
                let handle = app.handle().clone();
                thread::Builder::new()
                    .name("ez-hub-changed".to_string())
                    .spawn(move || {
                        while receiver.recv().is_ok() {
                            if let Err(error) = handle.emit("hub://changed", ()) {
                                tracing::debug!(%error, "la vista ya no escucha cambios");
                            }
                        }
                    })
                    .expect("hilo de avisos a la vista");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
}

fn watchdog(path: PathBuf) {
    thread::sleep(Duration::from_secs(20));
    if path.exists() {
        return;
    }
    if let Err(error) = selfcheck::write_timeout(&path) {
        tracing::error!(%error, "no pude escribir el informe de autocomprobación");
    }
    std::process::exit(1);
}

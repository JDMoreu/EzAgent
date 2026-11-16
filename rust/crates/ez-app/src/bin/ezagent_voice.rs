//! Binary `ezagent-voice`: mismo arranque que `python -m ezagent.voice.app`.
// Sin ventana de consola en `release`, como el ejecutable de Python
// (`console=False`); las órdenes de terminal se enganchan a la consola padre.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ez_hud::win::console::attach_parent_console();
    std::process::exit(ez_app::app::main::main());
}

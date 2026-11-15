//! `_print_agents` y `_print_mics`: formatos exactos de la consola.

use ez_audio::capture::{list_input_devices, InputDevice};
use ez_core::config::VoiceConfig;
use ez_core::pyvalue::text_or_empty;
use ez_herdr::herdr::Herdr;
use serde_json::{Map, Value};

use super::super::core::log;
use super::output::print_line;

/// `_print_agents(cfg)`: una línea por agente de Herdr.
pub fn print_agents(cfg: &VoiceConfig) {
    let herdr = Herdr::new(cfg.clone());
    match herdr.list_agents(10.0) {
        Ok(agents) => {
            for agent in agents {
                print_line(&agent_row(&agent));
            }
        }
        Err(error) => log(format!("No pude listar agentes: {error}")),
    }
}

/// Formato exacto de `_print_agents`.
pub fn agent_row(agent: &Map<String, Value>) -> String {
    let address = text_or_empty(agent.get("address"));
    let name = text_or_empty(agent.get("agent"));
    let name = if name.is_empty() { "?" } else { &name };
    let state = text_or_empty(agent.get("state"));
    let state = if state.is_empty() { "?" } else { &state };
    let cwd = text_or_empty(agent.get("cwd"));
    let place = text_or_empty(agent.get("where"));
    format!("{address:>4}  {name:<14} {state:<8} {cwd}  {place}")
}

/// `_print_mics()`: una línea por dispositivo de entrada.
pub fn print_mics() {
    for device in list_input_devices() {
        print_line(&mic_row(&device));
    }
}

/// Formato exacto de `_print_mics`.
pub fn mic_row(device: &InputDevice) -> String {
    format!(
        "{:>3}  {:<45} {:>6} Hz  {} canales",
        device.index, device.name, device.rate, device.channels
    )
}

//! Filtros, pines y topología KS (`FilterNew`/`PinNew` de `pa_win_wdmks.c`).

use windows::core::GUID;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::KernelStreaming::{
    KSINTERFACESETID_Standard, KSMEDIUMSETID_Standard, KSPROPSETID_Topology,
    KSINTERFACE_STANDARD_LOOPED_STREAMING, KSINTERFACE_STANDARD_STREAMING, KSMEDIUM_STANDARD_DEVIO,
    KSNODETYPE_MUX, KSPIN_COMMUNICATION_BOTH, KSPIN_COMMUNICATION_SINK, KSPIN_DATAFLOW_IN,
    KSPIN_DATAFLOW_OUT, KSPROPERTY_PIN_COMMUNICATION, KSPROPERTY_PIN_CTYPES,
    KSPROPERTY_PIN_DATAFLOW, KSPROPERTY_PIN_INTERFACES, KSPROPERTY_PIN_MEDIUMS,
    KSPROPERTY_TOPOLOGY_CONNECTIONS, KSPROPERTY_TOPOLOGY_NODES, KSTOPOLOGY_CONNECTION,
};

use super::open_ks;
use super::{ks, Filter, Pin};

/// `KSFILTER_NODE`: el nodo que representa un pin del propio filtro.
const KSFILTER_NODE: u32 = u32::MAX;

/// `FilterNew`: los datos del filtro y solo los pines que `PinNew` acepta.
pub(super) fn build(handle: HANDLE, friendly: String, wave_rt: bool) -> Option<Filter> {
    let pin_count = ks::pin_u32(handle, 0, KSPROPERTY_PIN_CTYPES)?;
    let connections = ks::property_multi(
        handle,
        None,
        KSPROPSETID_Topology,
        KSPROPERTY_TOPOLOGY_CONNECTIONS.0 as u32,
    )?;
    let nodes = ks::property_multi(
        handle,
        None,
        KSPROPSETID_Topology,
        KSPROPERTY_TOPOLOGY_NODES.0 as u32,
    )?;
    let mut pins = Vec::new();
    for pin_id in 0..pin_count {
        if let Some(pin) = new_pin(handle, pin_id, &connections, &nodes, wave_rt) {
            pins.push(pin);
        }
    }
    if pins.is_empty() {
        return None;
    }
    Some(Filter { friendly, pins })
}

/// `PinNew`: solo los pines de streaming con audio y una tasa conocida.
fn new_pin(
    handle: HANDLE,
    pin_id: u32,
    connections: &[KSTOPOLOGY_CONNECTION],
    nodes: &[GUID],
    wave_rt: bool,
) -> Option<Pin> {
    let communication = ks::pin_u32(handle, pin_id, KSPROPERTY_PIN_COMMUNICATION)? as i32;
    if communication != KSPIN_COMMUNICATION_SINK.0 && communication != KSPIN_COMMUNICATION_BOTH.0 {
        return None;
    }
    let data_flow = ks::pin_u32(handle, pin_id, KSPROPERTY_PIN_DATAFLOW)?;
    let streaming = if wave_rt {
        KSINTERFACE_STANDARD_LOOPED_STREAMING.0 as u32
    } else {
        KSINTERFACE_STANDARD_STREAMING.0 as u32
    };
    if !ks::pin_has_identifier(
        handle,
        pin_id,
        KSPROPERTY_PIN_INTERFACES,
        KSINTERFACESETID_Standard,
        streaming,
    ) || !ks::pin_has_identifier(
        handle,
        pin_id,
        KSPROPERTY_PIN_MEDIUMS,
        KSMEDIUMSETID_Standard,
        KSMEDIUM_STANDARD_DEVIO,
    ) {
        return None;
    }
    let (channels, rate) = ks::data_ranges(handle, pin_id)?;
    Some(Pin {
        data_flow,
        channels,
        rate,
        names: pin_names(handle, pin_id, data_flow, connections, nodes),
    })
}

/// Nombres del pin: el de su pin de topología, o el de la categoría, o
/// `Input`/`Output`.
fn pin_names(
    handle: HANDLE,
    pin_id: u32,
    data_flow: u32,
    connections: &[KSTOPOLOGY_CONNECTION],
    nodes: &[GUID],
) -> Vec<String> {
    let is_input = data_flow == KSPIN_DATAFLOW_OUT.0 as u32;
    let default = if data_flow == KSPIN_DATAFLOW_IN.0 as u32 {
        "Output"
    } else {
        "Input"
    };
    let Some(topo_pin) = connected_pin(connections, nodes, pin_id, !is_input, None) else {
        return vec![default.to_string()];
    };
    let Some(physical) = ks::physical_connection(handle, topo_pin.pin) else {
        let name = ks::pin_name(handle, topo_pin.pin)
            .or_else(|| ks::category_name(handle, topo_pin.pin, is_input))
            .unwrap_or_else(|| default.to_string());
        return vec![name];
    };
    let Some(topology) = Topology::open(&physical.path) else {
        return vec![default.to_string()];
    };
    let names = if is_input {
        capture_names(&topology, physical.pin)
    } else {
        render_names(&topology, physical.pin)
    };
    if names.is_empty() {
        vec![default.to_string()]
    } else {
        names
    }
}

/// El filtro de topología, abierto solo para leer los nombres de sus pines.
struct Topology {
    handle: HANDLE,
    connections: Vec<KSTOPOLOGY_CONNECTION>,
    nodes: Vec<GUID>,
}

impl Topology {
    fn open(path: &str) -> Option<Self> {
        let handle = open_ks(path)?;
        let connections = ks::property_multi(
            handle,
            None,
            KSPROPSETID_Topology,
            KSPROPERTY_TOPOLOGY_CONNECTIONS.0 as u32,
        )?;
        let nodes = ks::property_multi(
            handle,
            None,
            KSPROPSETID_Topology,
            KSPROPERTY_TOPOLOGY_NODES.0 as u32,
        )?;
        Some(Self {
            handle,
            connections,
            nodes,
        })
    }
}

impl Drop for Topology {
    fn drop(&mut self) {
        // SAFETY: `handle` viene de `open_ks` y solo se cierra aquí.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

/// Un pin de salida: un solo endpoint, siguiendo la topología hacia adelante.
fn render_names(topology: &Topology, start: u32) -> Vec<String> {
    let Some(connected) = connected_pin(&topology.connections, &topology.nodes, start, true, None)
    else {
        return Vec::new();
    };
    vec![topology_name(topology, connected.pin, false)]
}

/// Un pin de captura: un endpoint, o uno por posición del multiplexor.
fn capture_names(topology: &Topology, start: u32) -> Vec<String> {
    let Some(first) = connected_pin(
        &topology.connections,
        &topology.nodes,
        start,
        false,
        Some(0),
    ) else {
        return Vec::new();
    };
    if first.mux_node.is_none() {
        return vec![topology_name(topology, first.pin, true)];
    }
    let mut names = Vec::new();
    for position in 0..64 {
        let Some(connected) = connected_pin(
            &topology.connections,
            &topology.nodes,
            start,
            false,
            Some(position),
        ) else {
            break;
        };
        names.push(topology_name(topology, connected.pin, true));
    }
    names
}

fn topology_name(topology: &Topology, pin: u32, input: bool) -> String {
    // DECISION: `PA_WDMKS_USE_CATEGORY_FOR_PIN_NAMES` vale 1 en PortAudio: en
    // las salidas el nombre sale de la categoría (en inglés), no de
    // `KSPROPERTY_PIN_NAME`; en las capturas, al revés.
    let name = if input {
        ks::pin_name(topology.handle, pin)
            .or_else(|| ks::category_name(topology.handle, pin, input))
    } else {
        ks::category_name(topology.handle, pin, input)
    };
    name.unwrap_or_else(|| {
        if input {
            "Input".to_string()
        } else {
            "Output".to_string()
        }
    })
}

/// El pin de topología de un pin de captura (hacia atrás) o salida (hacia
/// adelante).
fn connected_pin(
    connections: &[KSTOPOLOGY_CONNECTION],
    nodes: &[GUID],
    start: u32,
    forward: bool,
    mux: Option<u32>,
) -> Option<Connected> {
    let mut current = find_start(connections, start, forward)?;
    let mut mux_node = None;
    loop {
        let connection = connections.get(current)?;
        let terminal = if forward {
            connection.ToNode == KSFILTER_NODE
        } else {
            connection.FromNode == KSFILTER_NODE
        };
        if terminal {
            let pin = if forward {
                connection.ToNodePin
            } else {
                connection.FromNodePin
            };
            return Some(Connected { pin, mux_node });
        }
        if let Some(position) = mux {
            if !forward && nodes.get(connection.FromNode as usize) == Some(&KSNODETYPE_MUX) {
                current = next_connection(connections, current, forward, Some(position))?;
                mux_node = Some(connections[current].ToNode);
                continue;
            }
        }
        current = next_connection(connections, current, forward, None)?;
    }
}

struct Connected {
    pin: u32,
    mux_node: Option<u32>,
}

fn find_start(connections: &[KSTOPOLOGY_CONNECTION], pin: u32, forward: bool) -> Option<usize> {
    connections.iter().position(|connection| {
        if forward {
            connection.FromNode == KSFILTER_NODE && connection.FromNodePin == pin
        } else {
            connection.ToNode == KSFILTER_NODE && connection.ToNodePin == pin
        }
    })
}

/// `GetConnectionTo`/`GetConnectionFrom`, con la posición del mux si la hay.
fn next_connection(
    connections: &[KSTOPOLOGY_CONNECTION],
    current: usize,
    forward: bool,
    mux: Option<u32>,
) -> Option<usize> {
    let connection = connections.get(current)?;
    let target = if forward {
        connection.ToNode
    } else {
        connection.FromNode
    };
    let mut skipped = 0;
    for (index, candidate) in connections.iter().enumerate() {
        if index == current {
            continue;
        }
        let matches = if forward {
            candidate.FromNode == target
        } else {
            candidate.ToNode == target
        };
        if matches {
            if let Some(position) = mux {
                if skipped < position {
                    skipped += 1;
                    continue;
                }
            }
            return Some(index);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(
        from_node: u32,
        from_pin: u32,
        to_node: u32,
        to_pin: u32,
    ) -> KSTOPOLOGY_CONNECTION {
        KSTOPOLOGY_CONNECTION {
            FromNode: from_node,
            FromNodePin: from_pin,
            ToNode: to_node,
            ToNodePin: to_pin,
        }
    }

    #[test]
    fn a_capture_pin_walks_back_to_its_topology_pin() {
        // pin 0 ← nodo 1 ← nodo 0 ← pin 9 de la topología.
        let connections = [
            connection(1, 0, KSFILTER_NODE, 0),
            connection(0, 0, 1, 0),
            connection(KSFILTER_NODE, 9, 0, 0),
        ];
        let found = connected_pin(&connections, &[], 0, false, None);
        assert_eq!(found.map(|value| value.pin), Some(9));
    }

    #[test]
    fn a_render_pin_walks_forward_to_its_topology_pin() {
        // pin 0 → nodo 0 → nodo 1 → pin 4 de la topología.
        let connections = [
            connection(KSFILTER_NODE, 0, 0, 0),
            connection(0, 0, 1, 0),
            connection(1, 0, KSFILTER_NODE, 4),
        ];
        let found = connected_pin(&connections, &[], 0, true, None);
        assert_eq!(found.map(|value| value.pin), Some(4));
    }

    #[test]
    fn a_mux_pin_reports_the_chosen_input() {
        // pin 0 ← mux 0 con dos entradas ← pines 9 y 10 de la topología.
        let connections = [
            connection(0, 0, KSFILTER_NODE, 0),
            connection(KSFILTER_NODE, 9, 0, 0),
            connection(KSFILTER_NODE, 10, 0, 1),
        ];
        let nodes = [KSNODETYPE_MUX];
        let found = connected_pin(&connections, &nodes, 0, false, Some(1));
        assert_eq!(found.as_ref().map(|value| value.pin), Some(10));
        assert_eq!(found.and_then(|value| value.mux_node), Some(0));
    }
}

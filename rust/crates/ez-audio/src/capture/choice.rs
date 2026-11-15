//! Elección del micrófono: `pick_input_device` del Python y la traducción de un
//! índice de PortAudio al dispositivo de cpal que se abre.

use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::portaudio::{names_match, PortAudioDevice};
use super::{device_at, device_name, host_devices, input_devices, HostDevice, InputDevice};
use super::{MIN_TIMING_SECS, SKIP_NAMES};
use crate::CHUNK_SECS;

/// Lo que la app necesita del micrófono elegido.
pub struct ChosenMic {
    /// Índice de PortAudio: el que se guarda en `voice.json`.
    pub portaudio: Option<usize>,
    /// Índice en la enumeración de cpal: el que abre el stream.
    pub cpal: Option<usize>,
    /// Nombre en PortAudio: el que registra y guarda la app.
    pub name: String,
}

/// `pick_input_device` con el nombre guardado del micrófono.
///
/// El nombre manda sobre el índice: los índices de PortAudio, de cpal y los de
/// una enumeración anterior apuntan a dispositivos distintos, así que «el mismo
/// `voice.json`» sonaba por micrófonos distintos en cada app. Si el nombre ya no
/// existe, el índice sigue siendo la pista y, si nada vale, se elige como
/// siempre.
// DECISION: paridad con Python añadiendo el nombre; sin nombre, la elección es
// exactamente `pick_input_device`.
pub fn choose_mic(preferred: Option<usize>, name: Option<&str>) -> ChosenMic {
    let host = host_devices();
    let all = super::portaudio_devices_with(&host);
    let inputs = input_devices(&all);
    let default_cpal = default_cpal_input(&host);
    let hint = name
        .and_then(|name| find_by_name(&inputs, name))
        .or(preferred);
    let chosen = choose_input_device(&inputs, hint, default_input_index(&all), |index, rate| {
        read_timing(&all, &host, default_cpal, index, rate)
    });
    let name = chosen.and_then(|index| {
        all.iter()
            .find(|device| device.index == index)
            .map(|device| device.name.clone())
    });
    ChosenMic {
        portaudio: chosen,
        cpal: chosen.and_then(|index| resolve_in(&all, &host, default_cpal, index)),
        name: name.unwrap_or_else(|| "ninguno".to_string()),
    }
}

pub fn pick_input_device(preferred: Option<usize>) -> Option<usize> {
    choose_mic(preferred, None).portaudio
}

/// [`pick_input_device`] con el nombre guardado del micrófono.
pub fn pick_input_device_named(preferred: Option<usize>, name: Option<&str>) -> Option<usize> {
    choose_mic(preferred, name).portaudio
}

/// Elige el índice de entrada como `pick_input_device`, sin tocar el hardware:
/// `timing` devuelve la latencia de lectura medida para `(índice, tasa)`.
pub fn choose_input_device(
    infos: &[InputDevice],
    preferred: Option<usize>,
    default_index: Option<usize>,
    mut timing: impl FnMut(usize, u32) -> Option<f64>,
) -> Option<usize> {
    if let Some(index) = preferred {
        if let Some(device) = infos.iter().find(|device| device.index == index) {
            if is_usable(device, &mut timing) {
                return Some(index);
            }
        }
    }
    let mut candidates: Vec<&InputDevice> = infos
        .iter()
        .filter(|device| is_usable(device, &mut timing))
        .collect();
    candidates.sort_by_key(|device| {
        let name = device.name.to_lowercase();
        (
            if name.contains("micrófono") { 0 } else { 1 },
            if name.contains("webcam") { 0 } else { 1 },
            device.index,
        )
    });
    if let Some(first) = candidates.first() {
        return Some(first.index);
    }
    default_index.filter(|index| infos.iter().any(|device| device.index == *index))
}

fn is_usable(device: &InputDevice, timing: &mut impl FnMut(usize, u32) -> Option<f64>) -> bool {
    let name = device.name.to_lowercase();
    if SKIP_NAMES.iter().any(|skip| name.contains(skip)) {
        return false;
    }
    let rate = if device.rate == 0 {
        16_000
    } else {
        device.rate
    };
    timing(device.index, rate).is_some_and(|value| value >= MIN_TIMING_SECS)
}

/// Índice del dispositivo de entrada con ese nombre, entre los de `infos`.
///
/// El nombre se compara exacto, luego sin distinguir mayúsculas y, al final, por
/// prefijo: los nombres de MME vienen cortados a 31 caracteres.
fn find_by_name(infos: &[InputDevice], name: &str) -> Option<usize> {
    if name.is_empty() {
        return None;
    }
    if let Some(device) = infos.iter().find(|device| device.name == name) {
        return Some(device.index);
    }
    let wanted = name.to_lowercase();
    if let Some(device) = infos
        .iter()
        .find(|device| device.name.to_lowercase() == wanted)
    {
        return Some(device.index);
    }
    infos
        .iter()
        .find(|device| names_match(&device.name, name))
        .map(|device| device.index)
}

/// El dispositivo de cpal que corresponde a un índice de PortAudio.
///
/// Sin coincidencia de nombre (una salida, otra API de host o un índice fuera de
/// la lista) devuelve `None`: ese índice cae a la elección automática, como en
/// Python con un índice que no es un micrófono. El mapeador de sonido es el
/// dispositivo por omisión del sistema, que es lo que abriría Python.
fn resolve_in(
    devices: &[PortAudioDevice],
    host: &[HostDevice],
    default_cpal: Option<usize>,
    index: usize,
) -> Option<usize> {
    let target = devices.iter().find(|device| device.index == index)?;
    if !target.input {
        return None;
    }
    if target.mapper {
        return default_cpal;
    }
    host.iter()
        .find(|device| names_match(&device.name, &target.name))
        .map(|device| device.index)
}

/// El dispositivo de entrada por omisión del sistema, entre los de `host`.
fn default_cpal_input(host: &[HostDevice]) -> Option<usize> {
    let name = cpal_default_input_name()?;
    host.iter()
        .find(|device| names_match(&device.name, &name))
        .map(|device| device.index)
}

/// El nombre del dispositivo de entrada por omisión, como lo ve cpal.
fn cpal_default_input_name() -> Option<String> {
    let device = cpal::default_host().default_input_device()?;
    Some(device_name(&device.to_string()))
}

/// El índice de PortAudio del micrófono por omisión del sistema, como
/// `pa.get_default_input_device_info()["index"]`.
// DECISION: PortAudio pregunta a `winmm` por el dispositivo preferido y cae al
// mapeador; aquí se usa el nombre del que elige cpal y, si no aparece, el
// mapeador, que sigue al mismo dispositivo por omisión.
fn default_input_index(devices: &[PortAudioDevice]) -> Option<usize> {
    let by_name = cpal_default_input_name().and_then(|name| {
        devices
            .iter()
            .find(|device| device.input && names_match(&device.name, &name))
    });
    by_name
        .or_else(|| devices.iter().find(|device| device.input && device.mapper))
        .or_else(|| devices.iter().find(|device| device.input))
        .map(|device| device.index)
}

/// `_read_timing`: media de tres lecturas de un chunk, en segundos.
///
/// La tasa pedida es la de PortAudio; si el dispositivo de cpal no la acepta se
/// usa la suya (WASAPI no remuestrea como MME), que es lo que hace
/// `open_stream`.
fn read_timing(
    devices: &[PortAudioDevice],
    host: &[HostDevice],
    default_cpal: Option<usize>,
    index: usize,
    rate: u32,
) -> Option<f64> {
    let cpal_index = resolve_in(devices, host, default_cpal, index)?;
    let device = device_at(cpal_index)?;
    let (config, format) = super::input_config(&device, rate)?;
    let stream_rate = config.sample_rate;
    let channels = usize::from(config.channels);
    let frames = ((f64::from(stream_rate) * CHUNK_SECS) as usize).max(160);
    let (sender, receiver) = crossbeam_channel::bounded::<usize>(32);
    let stream = device
        .build_input_stream_raw(
            config,
            format,
            move |data, _| {
                let _ = sender.try_send(data.len());
            },
            |error| tracing::warn!("Audio: {error}"),
            Some(Duration::from_secs(2)),
        )
        .ok()?;
    stream.play().ok()?;
    let start = std::time::Instant::now();
    let target = frames * channels * 3;
    let mut received = 0_usize;
    while received < target {
        received += receiver.recv_timeout(Duration::from_secs(2)).ok()?;
    }
    Some(start.elapsed().as_secs_f64() / 3.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(index: usize, name: &str, rate: u32) -> InputDevice {
        InputDevice {
            index,
            name: name.to_string(),
            rate,
            channels: 2,
        }
    }

    fn portaudio(index: usize, name: &str, input: bool) -> PortAudioDevice {
        portaudio_mapper(index, name, input, false)
    }

    fn portaudio_mapper(index: usize, name: &str, input: bool, mapper: bool) -> PortAudioDevice {
        PortAudioDevice {
            index,
            id: index,
            mapper,
            name: name.to_string(),
            rate: 44_100,
            channels: 1,
            input,
        }
    }

    /// La enumeración de MME de esta máquina: 0 es el mapeador, 1 y 2 son los
    /// micrófonos y el 4 es una salida (lo que el `voice.json` del usuario tiene
    /// guardado).
    fn machine() -> Vec<PortAudioDevice> {
        vec![
            portaudio_mapper(0, "Asignador de sonido Microsoft - Input", true, true),
            portaudio(1, "Micrófono (TA82)", true),
            portaudio(2, "Micrófono (Webcam AC410)", true),
            portaudio_mapper(3, "Asignador de sonido Microsoft - Output", false, true),
            portaudio(4, "Altavoces (TA82)", false),
        ]
    }

    fn host() -> Vec<HostDevice> {
        vec![
            HostDevice {
                index: 0,
                name: "Altavoces (TA82)".to_string(),
                channels: 0,
            },
            HostDevice {
                index: 4,
                name: "Micrófono (TA82)".to_string(),
                channels: 1,
            },
            HostDevice {
                index: 5,
                name: "Micrófono (Webcam AC410)".to_string(),
                channels: 2,
            },
        ]
    }

    #[test]
    fn preferred_device_wins_when_usable() {
        let infos = [
            device(0, "Micrófono (USB)", 48_000),
            device(1, "Webcam", 16_000),
        ];
        let picked = choose_input_device(&infos, Some(0), None, |_, _| Some(0.01));
        assert_eq!(picked, Some(0));
    }

    #[test]
    fn microphone_beats_webcam_and_skipped_names() {
        let infos = [
            device(0, "Mezcla estéreo", 48_000),
            device(1, "Webcam", 16_000),
            device(2, "Micrófono interno", 48_000),
        ];
        let picked = choose_input_device(&infos, None, None, |_, _| Some(0.01));
        assert_eq!(picked, Some(2));
    }

    #[test]
    fn devices_that_do_not_answer_are_skipped() {
        let infos = [
            device(0, "Micrófono roto", 48_000),
            device(1, "Webcam", 16_000),
        ];
        let picked = choose_input_device(&infos, Some(0), None, |index, _| {
            if index == 0 {
                None
            } else {
                Some(0.01)
            }
        });
        assert_eq!(picked, Some(1));
    }

    #[test]
    fn slow_reads_fall_back_to_the_default_device() {
        let infos = [device(0, "Micrófono lento", 48_000)];
        let picked = choose_input_device(&infos, None, Some(0), |_, _| Some(0.001));
        assert_eq!(picked, Some(0));
        let none = choose_input_device(&infos, None, None, |_, _| Some(0.001));
        assert_eq!(none, None);
    }

    #[test]
    fn zero_rate_uses_16k_for_the_timing_probe() {
        let infos = [device(7, "Micrófono", 0)];
        let mut seen = None;
        let picked = choose_input_device(&infos, None, None, |index, rate| {
            seen = Some((index, rate));
            Some(0.01)
        });
        assert_eq!(picked, Some(7));
        assert_eq!(seen, Some((7, 16_000)));
    }

    #[test]
    fn an_output_index_falls_back_to_the_automatic_choice() {
        // Con el índice 4 de PortAudio (una salida) Python descarta el índice y
        // elige el micrófono: aquí el Webcam (2), que es el que ordena primero.
        let all = machine();
        let infos = input_devices(&all);
        let picked = choose_input_device(&infos, Some(4), Some(0), |_, _| Some(0.01));
        assert_eq!(picked, Some(2));
        // Un índice de otra API de host (no está en el bloque de MME) cae igual.
        let picked = choose_input_device(&infos, Some(21), Some(0), |_, _| Some(0.01));
        assert_eq!(picked, Some(2));
    }

    #[test]
    fn a_truncated_name_matches_the_same_microphone() {
        let all = machine();
        let infos = input_devices(&all);
        assert_eq!(find_by_name(&infos, "micrófono (webcam ac410)"), Some(2));
        // El nombre guardado por una versión anterior salía de cpal, más largo.
        assert_eq!(
            find_by_name(&infos, "Micrófono (Webcam AC410) (2- USB Audio)"),
            Some(2)
        );
        assert_eq!(find_by_name(&infos, ""), None);
        assert_eq!(find_by_name(&infos, "Micrófono (otro)"), None);
    }

    #[test]
    fn a_portaudio_index_resolves_to_the_device_with_the_same_name() {
        let all = machine();
        assert_eq!(resolve_in(&all, &host(), None, 2), Some(5));
        assert_eq!(resolve_in(&all, &host(), None, 1), Some(4));
        // Una salida, un índice que no existe o el de otra API: no son micrófono.
        assert_eq!(resolve_in(&all, &host(), None, 4), None);
        assert_eq!(resolve_in(&all, &host(), None, 21), None);
    }

    #[test]
    fn the_sound_mapper_follows_the_system_default_device() {
        let all = machine();
        assert_eq!(resolve_in(&all, &host(), Some(4), 0), Some(4));
        assert_eq!(resolve_in(&all, &host(), None, 0), None);
    }
}

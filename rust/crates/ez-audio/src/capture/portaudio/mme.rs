//! Bloque de MME de PortAudio, que es el que ocupa los índices bajos.
//!
//! Reproduce `pa_win_wmme.c`: el mapeador de entrada, las entradas de `winmm`,
//! el mapeador de salida y las salidas.

use std::mem::size_of;

use windows::Win32::Media::Audio::{
    waveInGetDevCapsW, waveInGetNumDevs, waveInOpen, waveOutGetDevCapsW, waveOutGetNumDevs,
    waveOutOpen, WAVEFORMATEX, WAVEINCAPSW, WAVEOUTCAPSW, WAVE_FORMAT_1M16, WAVE_FORMAT_1S16,
    WAVE_FORMAT_2M16, WAVE_FORMAT_2S16, WAVE_FORMAT_4M16, WAVE_FORMAT_4S16, WAVE_FORMAT_PCM,
    WAVE_FORMAT_QUERY,
};

use super::{push_device, PortAudioDevice};

/// El mapeador de sonido de `winmm` (`WAVE_MAPPER`), que es el dispositivo por
/// omisión del sistema.
const WAVE_MAPPER: usize = u32::MAX as usize;

/// `defaultSampleRateSearchOrder_` de PortAudio, en su orden.
const RATES: [u32; 13] = [
    44_100, 48_000, 32_000, 24_000, 22_050, 88_200, 96_000, 192_000, 16_000, 12_000, 11_025, 9_600,
    8_000,
];

/// Los sufijos que PortAudio les pone a los dos mapeadores.
const INPUT_SUFFIX: &str = " - Input";
const OUTPUT_SUFFIX: &str = " - Output";

/// Lo que `wave*GetDevCaps` cuenta de un dispositivo.
struct Caps {
    name: String,
    channels: u16,
    formats: u32,
}

/// Añade el bloque de MME de PortAudio.
///
/// `channels_of` devuelve el máximo de canales del endpoint con ese nombre (lo
/// que PortAudio le pregunta al filtro KS); si no contesta, se usan los de
/// `wave*GetDevCaps`. Las salidas solo sirven para reconocer un índice que no es
/// un micrófono, así que se quedan con los de `winmm`.
pub(super) fn append(
    devices: &mut Vec<PortAudioDevice>,
    channels_of: impl Fn(&str) -> Option<u16>,
) {
    // SAFETY: cuenta los dispositivos del sistema; no usa punteros.
    let inputs = unsafe { waveInGetNumDevs() } as usize;
    for id in mapper_ids(inputs) {
        let Some(caps) = input_caps(id) else { continue };
        let mapper = id == WAVE_MAPPER;
        let channels = if mapper {
            caps.channels
        } else {
            channels_of(&caps.name)
                .filter(|count| *count > 0)
                .unwrap_or(caps.channels)
        };
        let rate = detect_rate(channels, caps.formats, |rate| {
            input_accepts(id, channels, rate)
        });
        push_device(
            devices,
            mapper_name(&caps.name, mapper, INPUT_SUFFIX),
            rate,
            channels,
            true,
            id,
            mapper,
        );
    }
    // SAFETY: cuenta los dispositivos del sistema; no usa punteros.
    let outputs = unsafe { waveOutGetNumDevs() } as usize;
    for id in mapper_ids(outputs) {
        let Some(caps) = output_caps(id) else {
            continue;
        };
        let mapper = id == WAVE_MAPPER;
        let rate = detect_rate(caps.channels, caps.formats, |rate| {
            output_accepts(id, caps.channels, rate)
        });
        push_device(
            devices,
            mapper_name(&caps.name, mapper, OUTPUT_SUFFIX),
            rate,
            caps.channels,
            false,
            id,
            mapper,
        );
    }
}

/// PortAudio añade el mapeador delante de cada sentido si hay dispositivos.
fn mapper_ids(count: usize) -> Vec<usize> {
    if count == 0 {
        return Vec::new();
    }
    std::iter::once(WAVE_MAPPER).chain(0..count).collect()
}

fn mapper_name(name: &str, mapper: bool, suffix: &str) -> String {
    if mapper {
        format!("{name}{suffix}")
    } else {
        name.to_string()
    }
}

fn input_caps(id: usize) -> Option<Caps> {
    let mut caps = WAVEINCAPSW::default();
    // SAFETY: `caps` es un WAVEINCAPSW vivo y `cbwic` es su tamaño exacto;
    // `waveInGetDevCapsW` solo escribe en ese bloque.
    let code = unsafe { waveInGetDevCapsW(id, &mut caps, size_of::<WAVEINCAPSW>() as u32) };
    if code != 0 {
        return None;
    }
    // `WAVEINCAPSW` va empaquetada: el nombre se copia antes de leerlo.
    let name = caps.szPname;
    Some(Caps {
        name: super::wide_name(&name),
        channels: caps.wChannels,
        formats: caps.dwFormats,
    })
}

fn output_caps(id: usize) -> Option<Caps> {
    let mut caps = WAVEOUTCAPSW::default();
    // SAFETY: igual que `input_caps`, con la estructura de salida.
    let code = unsafe { waveOutGetDevCapsW(id, &mut caps, size_of::<WAVEOUTCAPSW>() as u32) };
    if code != 0 {
        return None;
    }
    // `WAVEOUTCAPSW` va empaquetada: el nombre se copia antes de leerlo.
    let name = caps.szPname;
    Some(Caps {
        name: super::wide_name(&name),
        channels: caps.wChannels,
        formats: caps.dwFormats,
    })
}

/// `DetectDefaultSampleRate`: la primera tasa de la lista que el dispositivo
/// acepta con su máximo de canales, o 0 si ninguna.
///
/// `dwFormats` ya anuncia las tasas estándar (16 bits), que se dan por buenas
/// sin abrir el dispositivo, como hace PortAudio.
fn detect_rate(channels: u16, formats: u32, mut supports: impl FnMut(u32) -> bool) -> u32 {
    RATES
        .into_iter()
        .find(|rate| announced(formats, channels, *rate) || supports(*rate))
        .unwrap_or(0)
}

/// Bits de `dwFormats` que PortAudio consulta sin abrir el dispositivo.
fn announced(formats: u32, channels: u16, rate: u32) -> bool {
    let bit = match (rate, channels) {
        (11_025, 1) => WAVE_FORMAT_1M16,
        (11_025, 2) => WAVE_FORMAT_1S16,
        (22_050, 1) => WAVE_FORMAT_2M16,
        (22_050, 2) => WAVE_FORMAT_2S16,
        (44_100, 1) => WAVE_FORMAT_4M16,
        (44_100, 2) => WAVE_FORMAT_4S16,
        _ => return false,
    };
    formats & bit != 0
}

/// `QueryInputWaveFormatEx`: pregunta si acepta el formato, sin abrir nada.
fn input_accepts(id: usize, channels: u16, rate: u32) -> bool {
    let format = wave_format(channels, rate);
    // SAFETY: en modo `WAVE_FORMAT_QUERY` no se abre el dispositivo, `format`
    // vive durante la llamada y `phwi` no se usa.
    unsafe { waveInOpen(None, id as u32, &format, None, None, WAVE_FORMAT_QUERY) == 0 }
}

/// `QueryOutputWaveFormatEx`: igual que [`input_accepts`], para una salida.
fn output_accepts(id: usize, channels: u16, rate: u32) -> bool {
    let format = wave_format(channels, rate);
    // SAFETY: igual que `input_accepts`, con `waveOutOpen`.
    unsafe { waveOutOpen(None, id as u32, &format, None, None, WAVE_FORMAT_QUERY) == 0 }
}

/// PCM de 16 bits con los canales y la tasa pedidos.
fn wave_format(channels: u16, rate: u32) -> WAVEFORMATEX {
    let block = channels.saturating_mul(2);
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: channels,
        nSamplesPerSec: rate,
        nAvgBytesPerSec: rate.saturating_mul(u32::from(block)),
        nBlockAlign: block,
        wBitsPerSample: 16,
        cbSize: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mapper_is_listed_before_the_devices_of_its_direction() {
        assert_eq!(mapper_ids(2), vec![WAVE_MAPPER, 0, 1]);
        assert!(mapper_ids(0).is_empty());
    }

    #[test]
    fn the_name_of_a_mapper_gets_the_direction_suffix() {
        assert_eq!(
            mapper_name("Asignador de sonido Microsoft", true, INPUT_SUFFIX),
            "Asignador de sonido Microsoft - Input"
        );
        assert_eq!(
            mapper_name("Micrófono (TA82)", false, OUTPUT_SUFFIX),
            "Micrófono (TA82)"
        );
    }

    #[test]
    fn the_announced_44k_bit_skips_the_probe() {
        let formats = WAVE_FORMAT_4S16;
        let mut probes = 0;
        let rate = detect_rate(2, formats, |_| {
            probes += 1;
            false
        });
        assert_eq!(rate, 44_100);
        assert_eq!(probes, 0); // Sin el bit de 44 100 se sondea en el orden de PortAudio.
        let rate = detect_rate(2, 0, |candidate| candidate == 48_000);
        assert_eq!(rate, 48_000);
        assert_eq!(detect_rate(2, 0, |_| false), 0);
    }

    #[test]
    fn formats_count_the_channels_they_announce() {
        assert!(announced(WAVE_FORMAT_4M16, 1, 44_100));
        assert!(!announced(WAVE_FORMAT_4M16, 2, 44_100));
        assert!(announced(WAVE_FORMAT_2S16, 2, 22_050));
        assert!(!announced(WAVE_FORMAT_4S16, 2, 48_000));
    }
}

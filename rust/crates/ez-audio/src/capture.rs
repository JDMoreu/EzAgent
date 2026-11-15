//! Port of `src/ezagent/voice/audio.py` (captura) (F4-01).
//!
//! PyAudio se sustituye por `cpal`: el micrófono no se cierra entre turnos y los
//! chunks de 30 ms se recortan del flujo continuo del dispositivo. Los índices
//! de dispositivo siguen la numeración de PortAudio (ver `portaudio`), que es la
//! que guarda `voice.json`.

mod choice;
mod mic;
mod portaudio;

use std::fmt;

use cpal::traits::{DeviceTrait, HostTrait};

use crate::CHUNK_SECS;

pub use choice::{
    choose_input_device, choose_mic, pick_input_device, pick_input_device_named, ChosenMic,
};
pub use mic::{CaptureEvent, CapturePipeline, MicCapture, StreamHooks, VadSettings};
pub use portaudio::PortAudioDevice;

/// Nombres que nunca son un micrófono útil (monitores de mezcla, etc.).
pub(crate) const SKIP_NAMES: [&str; 5] = [
    "controlador primario",
    "mezcla estéreo",
    "mezcla estereo",
    "steam",
    "wave",
];

/// Por debajo de este pico (≈ -46 dBFS) un clip es silencio o ruido de sala.
pub const WEAK_PEAK_RMS: f64 = 0.005;
pub const WEAK_VOICED_SECS: f64 = 0.3;
/// Tasa mínima de lectura para considerar que un dispositivo responde.
pub(crate) const MIN_TIMING_SECS: f64 = 0.005;

/// Fallo al abrir el micrófono. `NoMicrophone` reproduce el `RuntimeError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    NoMicrophone,
    Device(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoMicrophone => formatter.write_str("No hay ningún micrófono disponible."),
            Self::Device(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CaptureError {}

/// Un dispositivo de entrada, como los dicts de `list_input_devices`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDevice {
    /// Índice global de PortAudio: el mismo número que imprime la app Python.
    pub index: usize,
    pub name: String,
    pub rate: u32,
    pub channels: u16,
}

/// Un dispositivo de entrada del host por omisión de cpal (WASAPI en Windows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HostDevice {
    /// Índice en la enumeración completa del host (las salidas incluidas).
    pub index: usize,
    pub name: String,
    pub channels: u16,
}

/// Descarta un byte suelto final: una muestra int16 ocupa dos bytes.
fn even(pcm: &[u8]) -> &[u8] {
    &pcm[..pcm.len() - pcm.len() % 2]
}

fn samples(pcm: &[u8]) -> impl Iterator<Item = f32> + '_ {
    even(pcm)
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| i16::from_le_bytes(*pair) as f32 / 32768.0)
}

pub fn pcm_rms(pcm: &[u8]) -> f64 {
    let mut sum = 0.0_f32;
    let mut count = 0_usize;
    for sample in samples(pcm) {
        sum += sample * sample;
        count += 1;
    }
    if count == 0 {
        return 0.0;
    }
    f64::from((sum / count as f32).sqrt())
}

/// True si el clip no tiene voz sostenida (silencio, ruido o un clic).
pub fn pcm_is_weak(pcm: &[u8], rate: u32) -> bool {
    let frames = ((f64::from(rate) * CHUNK_SECS) as usize).max(1);
    let all: Vec<f32> = samples(pcm).collect();
    let usable = all.len() / frames * frames;
    if usable == 0 {
        return true;
    }
    let mut levels = Vec::with_capacity(usable / frames);
    let mut peak = 0.0_f32;
    for frame in all[..usable].chunks_exact(frames) {
        let sum: f32 = frame.iter().map(|value| value * value).sum();
        let level = (sum / frames as f32).sqrt();
        peak = peak.max(level);
        levels.push(level);
    }
    if f64::from(peak) < WEAK_PEAK_RMS {
        return true;
    }
    // `max(0.002, 0.25 * peak)` en float64 y el escalar recortado a float32,
    // que es lo que hace numpy al comparar un array float32.
    let threshold = ((0.25 * f64::from(peak)).max(0.002)) as f32;
    let voiced = levels.iter().filter(|level| **level >= threshold).count();
    voiced as f64 * CHUNK_SECS < WEAK_VOICED_SECS
}

/// WAV mono 16 bits PCM, igual que `wave.open(buffer, "wb")`.
pub fn pcm_to_wav(pcm: &[u8], rate: u32) -> Vec<u8> {
    wav_bytes(pcm, rate)
}

/// Como [`pcm_to_wav`] pero reproduce el error de `wave.setframerate` con una
/// tasa no positiva («sampling rate not specified»).
pub fn pcm_to_wav_checked(pcm: &[u8], rate: i64) -> Result<Vec<u8>, String> {
    if rate <= 0 || rate > i64::from(u32::MAX) {
        return Err("sampling rate not specified".to_string());
    }
    Ok(wav_bytes(pcm, rate as u32))
}

fn wav_bytes(pcm: &[u8], rate: u32) -> Vec<u8> {
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&((36 + pcm.len()) as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    wav.extend_from_slice(pcm);
    wav
}

/// PortAudio entrega los nombres en UTF-8 y Tk los interpreta como Latin-1.
fn device_name(name: &str) -> String {
    if name.contains('Ã') || name.contains('Â') {
        if let Some(fixed) = latin1_to_utf8(name) {
            return fixed;
        }
    }
    name.to_string()
}

fn latin1_to_utf8(name: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(name.len());
    for character in name.chars() {
        let code = u32::from(character);
        if code > 0xFF {
            return None;
        }
        bytes.push(code as u8);
    }
    String::from_utf8(bytes).ok()
}

fn device_at(index: usize) -> Option<cpal::Device> {
    cpal::default_host().devices().ok()?.nth(index)
}

/// Los dispositivos de entrada del host por omisión de cpal.
pub(crate) fn host_devices() -> Vec<HostDevice> {
    let host = cpal::default_host();
    let Ok(devices) = host.devices() else {
        return Vec::new();
    };
    let mut inputs = Vec::new();
    for (index, device) in devices.enumerate() {
        if !device.supports_input() {
            continue;
        }
        let config = device.default_input_config().ok();
        inputs.push(HostDevice {
            index,
            name: device_name(&device.to_string()),
            channels: config.as_ref().map_or(0, |value| value.channels()),
        });
    }
    inputs
}

/// Enumera el bloque de MME de PortAudio, que es el que numera `voice.json`.
pub fn list_portaudio_devices() -> Vec<PortAudioDevice> {
    let host = host_devices();
    portaudio::list_devices(|name| {
        host.iter()
            .find(|device| portaudio::names_match(&device.name, name))
            .map(|device| device.channels)
    })
}

/// La vista de `list_input_devices` del Python: solo las entradas.
pub fn input_devices(devices: &[PortAudioDevice]) -> Vec<InputDevice> {
    devices
        .iter()
        .filter(|device| device.input)
        .map(|device| InputDevice {
            index: device.index,
            name: device.name.clone(),
            rate: device.rate,
            channels: device.channels,
        })
        .collect()
}

pub fn list_input_devices() -> Vec<InputDevice> {
    input_devices(&list_portaudio_devices())
}

/// Configuración de entrada: prefiere int16 a la tasa pedida.
fn input_config(
    device: &cpal::Device,
    rate: u32,
) -> Option<(cpal::StreamConfig, cpal::SampleFormat)> {
    let mut fallback = None;
    if let Ok(ranges) = device.supported_input_configs() {
        for range in ranges {
            if !range.contains_rate(rate) {
                continue;
            }
            let format = range.sample_format();
            let config = range.with_sample_rate(rate).config();
            match format {
                cpal::SampleFormat::I16 => return Some((config, format)),
                cpal::SampleFormat::F32 if fallback.is_none() => fallback = Some((config, format)),
                _ => {}
            }
        }
    }
    fallback.or_else(|| {
        device
            .default_input_config()
            .ok()
            .map(|config| (config.config(), config.sample_format()))
    })
}

fn downmix_i16(samples: &[i16], channels: usize) -> Vec<i16> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks(channels)
        .map(|frame| {
            let sum: i32 = frame.iter().map(|value| i32::from(*value)).sum();
            (sum / frame.len() as i32) as i16
        })
        .collect()
}

fn downmix_f32(samples: &[f32], channels: usize) -> Vec<i16> {
    if channels <= 1 {
        return samples.iter().map(|value| to_i16(*value)).collect();
    }
    samples
        .chunks(channels)
        .map(|frame| {
            let sum: f32 = frame.iter().sum();
            to_i16(sum / frame.len() as f32)
        })
        .collect()
}

fn to_i16(value: f32) -> i16 {
    (value * 32768.0).clamp(-32768.0, 32767.0) as i16
}

/// Convierte el buffer del dispositivo a mono int16 (pcm del segmentador).
fn to_mono_i16(data: &cpal::Data, channels: usize) -> Vec<i16> {
    match data.sample_format() {
        cpal::SampleFormat::I16 => {
            downmix_i16(data.as_slice::<i16>().unwrap_or_default(), channels)
        }
        cpal::SampleFormat::F32 => {
            downmix_f32(data.as_slice::<f32>().unwrap_or_default(), channels)
        }
        cpal::SampleFormat::I32 => {
            let converted: Vec<i16> = data
                .as_slice::<i32>()
                .unwrap_or_default()
                .iter()
                .map(|value| (value >> 16) as i16)
                .collect();
            downmix_i16(&converted, channels)
        }
        cpal::SampleFormat::U16 => {
            let converted: Vec<i16> = data
                .as_slice::<u16>()
                .unwrap_or_default()
                .iter()
                .map(|value| (i32::from(*value) - 32_768) as i16)
                .collect();
            downmix_i16(&converted, channels)
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcm_of(values: &[i16]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    #[test]
    fn rms_of_silence_and_full_scale() {
        assert_eq!(pcm_rms(&[]), 0.0);
        assert_eq!(pcm_rms(&[7]), 0.0);
        let full = pcm_of(&[i16::MAX; 480]);
        assert!((pcm_rms(&full) - 1.0).abs() < 1e-4);
        let half = pcm_of(&[16384; 480]);
        assert!((pcm_rms(&half) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn odd_byte_is_ignored() {
        let mut pcm = pcm_of(&[16384, 16384]);
        pcm.push(0x7F);
        assert!((pcm_rms(&pcm) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn weak_clips_are_detected() {
        assert!(pcm_is_weak(&[], 16_000));
        // Menos de un chunk: no hay ni una ventana que medir.
        assert!(pcm_is_weak(&pcm_of(&[20_000; 100]), 16_000));
        assert!(pcm_is_weak(&pcm_of(&[0; 16_000]), 16_000));
        assert!(pcm_is_weak(&pcm_of(&[100; 16_000]), 16_000));
        // Un clic de un chunk y el resto en silencio no es voz sostenida.
        let mut click = vec![20_000_i16; 480];
        click.resize(16_000, 0);
        assert!(pcm_is_weak(&pcm_of(&click), 16_000));
        // Tono constante a media escala sí lo es.
        assert!(!pcm_is_weak(&pcm_of(&[16384; 16_000]), 16_000));
    }

    #[test]
    fn wav_header_matches_the_wave_module() {
        let wav = pcm_to_wav(&[1, 2, 3, 4], 16_000);
        assert_eq!(wav.len(), 48);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(
            u32::from_le_bytes(wav[4..8].try_into().expect("4 bytes")),
            40
        );
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(
            u32::from_le_bytes(wav[16..20].try_into().expect("4 bytes")),
            16
        );
        assert_eq!(
            u16::from_le_bytes(wav[20..22].try_into().expect("2 bytes")),
            1
        );
        assert_eq!(
            u16::from_le_bytes(wav[22..24].try_into().expect("2 bytes")),
            1
        );
        assert_eq!(
            u32::from_le_bytes(wav[24..28].try_into().expect("4 bytes")),
            16_000
        );
        assert_eq!(
            u32::from_le_bytes(wav[28..32].try_into().expect("4 bytes")),
            32_000
        );
        assert_eq!(
            u16::from_le_bytes(wav[32..34].try_into().expect("2 bytes")),
            2
        );
        assert_eq!(
            u16::from_le_bytes(wav[34..36].try_into().expect("2 bytes")),
            16
        );
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(
            u32::from_le_bytes(wav[40..44].try_into().expect("4 bytes")),
            4
        );
        assert_eq!(&wav[44..], &[1, 2, 3, 4]);
    }

    #[test]
    fn mojibake_device_names_are_repaired() {
        // «Micrófono» leído como Latin-1 deja «MicrÃ³fono».
        assert_eq!(device_name("MicrÃ³fono"), "Micrófono");
        assert_eq!(device_name("Altavoces (Â®)"), "Altavoces (®)");
        assert_eq!(device_name("Micrófono"), "Micrófono");
        // Con un carácter fuera de Latin-1 no se puede reparar.
        assert_eq!(device_name("Ã😀"), "Ã😀");
    }
}

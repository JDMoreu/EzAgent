//! Numeración global de PortAudio en Windows y emparejamiento de nombres con
//! los dispositivos de cpal.
//!
//! PyAudio numera los dispositivos con el índice global de PortAudio, que
//! reparte entre sus APIs de host en este orden: MME (la primera, la de por
//! omisión y la que ocupa los índices bajos), DirectSound, WASAPI y WDM-KS. El
//! `input_device_index` de `voice.json`, el de `--mic` y el del menú son ese
//! número, así que aquí se reproduce la enumeración de cada API tal como la
//! hace PortAudio, con el mismo orden, nombres, tasas y canales.

mod dsound;
mod mme;
mod wasapi;
mod wdmks;

/// Un dispositivo tal como lo numera PortAudio.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortAudioDevice {
    /// Índice global de PortAudio: el de `voice.json` y el de `--mic`.
    pub index: usize,
    /// Id de `winmm` (solo en MME; 0 en las demás APIs).
    pub id: usize,
    /// Es el dispositivo por omisión de su API: el mapeador de MME o el
    /// primario de DirectSound.
    pub mapper: bool,
    pub name: String,
    pub rate: u32,
    pub channels: u16,
    pub input: bool,
}

/// Añade un dispositivo con el índice que le toca en la lista global.
pub(super) fn push_device(
    devices: &mut Vec<PortAudioDevice>,
    name: String,
    rate: u32,
    channels: u16,
    input: bool,
    id: usize,
    mapper: bool,
) {
    let index = devices.len();
    devices.push(PortAudioDevice {
        index,
        id,
        mapper,
        name,
        rate,
        channels,
        input,
    });
}

/// `szPname` es una cadena UTF-16 terminada en nulo (y cortada a 31 caracteres).
pub(super) fn wide_name(text: &[u16]) -> String {
    let end = text
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}

/// Enumera los dispositivos de las cuatro APIs de host, en el orden de
/// PortAudio.
///
/// `channels_of` devuelve el máximo de canales del endpoint con ese nombre (lo
/// que PortAudio le pregunta al filtro KS en MME y DirectSound); si no
/// contesta, se usan los de `wave*GetDevCaps` y `DSCCAPS`.
pub fn list_devices(channels_of: impl Fn(&str) -> Option<u16>) -> Vec<PortAudioDevice> {
    let mut devices = Vec::new();
    mme::append(&mut devices, &channels_of);
    dsound::append(&mut devices, &channels_of);
    wasapi::append(&mut devices);
    wdmks::append(&mut devices);
    devices
}

/// Dos nombres del mismo dispositivo: los de MME vienen cortados a 31
/// caracteres, así que el corto es el principio del que da WASAPI.
pub fn names_match(left: &str, right: &str) -> bool {
    let left = left.trim().to_lowercase();
    let right = right.trim().to_lowercase();
    !left.is_empty()
        && !right.is_empty()
        && (left == right || left.starts_with(&right) || right.starts_with(&left))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_truncated_mme_name_matches_the_full_endpoint_name() {
        assert!(names_match(
            "3 - Q27B16 (AMD High Definition",
            "3 - Q27B16 (AMD High Definition Audio Device)"
        ));
        assert!(names_match(
            "MICRÓFONO (WEBCAM AC410)",
            "Micrófono (Webcam AC410)"
        ));
        assert!(!names_match("Micrófono (TA82)", "Micrófono (Webcam AC410)"));
        assert!(!names_match("", "Micrófono (TA82)"));
    }

    #[test]
    fn every_device_gets_the_next_global_index() {
        let mut devices = Vec::new();
        push_device(&mut devices, "uno".to_string(), 44_100, 1, true, 0, false);
        push_device(&mut devices, "dos".to_string(), 48_000, 2, false, 0, false);
        assert_eq!(devices[0].index, 0);
        assert_eq!(devices[1].index, 1);
    }
}

//! Bloque de DirectSound de PortAudio (`pa_win_ds.c`).
//!
//! PortAudio enumera primero los dispositivos de captura y después los de
//! reproducción, y descarta los controladores emulados (`DSCAPS_EMULDRIVER`)
//! porque MME ya los cubre. El nombre viene de la enumeración y los canales de
//! captura de `DSCCAPS` (refinados con el endpoint, como PortAudio); la tasa,
//! de los bits de `dwFormats` (16 bits).

use std::mem::size_of;

use windows::core::{BOOL, GUID, PCWSTR};
use windows::Win32::Media::Audio::DirectSound::{
    DirectSoundCaptureCreate, DirectSoundCaptureEnumerateW, DirectSoundCreate,
    DirectSoundEnumerateW, IDirectSound, IDirectSoundCapture, DSCAPS, DSCAPS_CONTINUOUSRATE,
    DSCAPS_EMULDRIVER, DSCAPS_PRIMARYSTEREO, DSCCAPS,
};
use windows::Win32::Media::Audio::{
    WAVE_FORMAT_1M16, WAVE_FORMAT_1S16, WAVE_FORMAT_2M16, WAVE_FORMAT_2S16, WAVE_FORMAT_48M16,
    WAVE_FORMAT_48S16, WAVE_FORMAT_4M16, WAVE_FORMAT_4S16, WAVE_FORMAT_96M16, WAVE_FORMAT_96S16,
};

use super::{push_device, PortAudioDevice};

/// Tasas de `defaultSampleRateSearchOrder_` de `pa_win_ds.c`.
const RATES: [u32; 13] = [
    44_100, 48_000, 32_000, 24_000, 22_050, 88_200, 96_000, 192_000, 16_000, 12_000, 11_025, 9_600,
    8_000,
];

type Names = Vec<(Option<GUID>, String)>;

/// Añade los dispositivos de DirectSound: capturas primero, salidas después.
///
/// `channels_of` refina los canales de las capturas como PortAudio, que le
/// pregunta al filtro KS: `DSCCAPS` informa 2 para un micrófono mono.
pub(super) fn append(
    devices: &mut Vec<PortAudioDevice>,
    channels_of: impl Fn(&str) -> Option<u16>,
) {
    let mut captures = Names::new();
    // SAFETY: el callback solo escribe en `captures`, que vive hasta que
    // termina la llamada.
    let _ = unsafe {
        DirectSoundCaptureEnumerateW(
            Some(collect),
            Some(&mut captures as *mut Names as *const core::ffi::c_void),
        )
    };
    for (guid, name) in captures {
        let Some(caps) = capture_caps(guid.as_ref()) else {
            continue;
        };
        if caps.dwFlags & DSCAPS_EMULDRIVER != 0 || caps.dwChannels < 1 {
            continue;
        }
        let channels = channels_of(&name)
            .filter(|count| *count > 0)
            .unwrap_or(caps.dwChannels as u16);
        // El GUID nulo es el dispositivo primario: el micrófono por omisión.
        push_device(
            devices,
            name,
            capture_rate(caps.dwChannels, caps.dwFormats),
            channels,
            true,
            0,
            guid.is_none(),
        );
    }

    let mut outputs = Names::new();
    // SAFETY: igual que la captura, con la enumeración de reproducción.
    let _ = unsafe {
        DirectSoundEnumerateW(
            Some(collect),
            Some(&mut outputs as *mut Names as *const core::ffi::c_void),
        )
    };
    for (guid, name) in outputs {
        let Some(caps) = output_caps(guid.as_ref()) else {
            continue;
        };
        if caps.dwFlags & DSCAPS_EMULDRIVER != 0 {
            continue;
        }
        let channels = if caps.dwFlags & DSCAPS_PRIMARYSTEREO != 0 {
            2
        } else {
            1
        };
        // DECISION: no se consulta `GetSpeakerConfig`: los canales de una salida
        // no los usa nadie (no es un micrófono ni cambia el índice global).
        push_device(devices, name, output_rate(&caps), channels, false, 0, false);
    }
}

/// `CollectGUIDsProcW`: guarda el GUID (nulo en el dispositivo primario) y el
/// nombre de cada dispositivo enumerado.
unsafe extern "system" fn collect(
    guid: *mut GUID,
    description: PCWSTR,
    _module: PCWSTR,
    context: *mut core::ffi::c_void,
) -> BOOL {
    // DX-1d: un pánico no puede cruzar la frontera FFI (Rust aborta con
    // 0xc0000409). Se atrapa y la enumeración sigue.
    ez_core::panics::catch_panic(|| {
        // SAFETY: `context` es el `Names` vivo que pasa `append`.
        let names = unsafe { &mut *(context as *mut Names) };
        // SAFETY: `description` es la cadena que entrega DirectSound en la llamada.
        let name = unsafe { description.to_string() }.unwrap_or_default();
        // SAFETY: `guid` es nulo o apunta a un GUID válido durante la llamada.
        let guid = if guid.is_null() {
            None
        } else {
            Some(unsafe { *guid })
        };
        names.push((guid, name));
        BOOL::from(true)
    })
    .unwrap_or(BOOL::from(true))
}

fn capture_caps(guid: Option<&GUID>) -> Option<DSCCAPS> {
    let mut device: Option<IDirectSoundCapture> = None;
    // SAFETY: el GUID vive durante la llamada y `device` recibe un puntero nulo
    // si la creación falla.
    unsafe { DirectSoundCaptureCreate(guid.map(|value| value as *const GUID), &mut device, None) }
        .ok()?;
    let device = device?;
    // `DSCCAPS` debe anunciar su tamaño; la envoltura de `windows` no lo hace.
    let mut caps = DSCCAPS {
        dwSize: size_of::<DSCCAPS>() as u32,
        ..Default::default()
    };
    // SAFETY: `caps` anuncia su tamaño y la vtable es la de este objeto vivo.
    let result = unsafe {
        (windows::core::Interface::vtable(&device).GetCaps)(
            windows::core::Interface::as_raw(&device),
            &mut caps,
        )
    };
    result.ok().ok()?;
    Some(caps)
}

fn output_caps(guid: Option<&GUID>) -> Option<DSCAPS> {
    let mut device: Option<IDirectSound> = None;
    // SAFETY: igual que en la captura, con el objeto de reproducción.
    unsafe { DirectSoundCreate(guid.map(|value| value as *const GUID), &mut device, None) }.ok()?;
    let mut caps = DSCAPS {
        dwSize: size_of::<DSCAPS>() as u32,
        ..Default::default()
    };
    // SAFETY: `caps` anuncia su tamaño y `GetCaps` solo escribe en él.
    unsafe { device?.GetCaps(&mut caps) }.ok()?;
    Some(caps)
}

/// `AddInputDeviceInfoFromDirectSoundCapture`: los bits de 16 bits de
/// `dwFormats` dan la tasa por omisión, con 48 000 de reserva.
fn capture_rate(channels: u32, formats: u32) -> u32 {
    let wanted: &[(u32, u32)] = match channels {
        2 => &[
            (WAVE_FORMAT_4S16, 44_100),
            (WAVE_FORMAT_48S16, 48_000),
            (WAVE_FORMAT_2S16, 22_050),
            (WAVE_FORMAT_1S16, 11_025),
            (WAVE_FORMAT_96S16, 96_000),
        ],
        1 => &[
            (WAVE_FORMAT_4M16, 44_100),
            (WAVE_FORMAT_48M16, 48_000),
            (WAVE_FORMAT_2M16, 22_050),
            (WAVE_FORMAT_1M16, 11_025),
            (WAVE_FORMAT_96M16, 96_000),
        ],
        _ => return 48_000,
    };
    wanted
        .iter()
        .find(|(bit, _)| formats & bit != 0)
        .map_or(48_000, |(_, rate)| *rate)
}

/// `AddOutputDeviceInfoFromDirectSound`: la primera tasa de la lista que cae
/// dentro del rango que anuncia el controlador.
fn output_rate(caps: &DSCAPS) -> u32 {
    if caps.dwFlags & DSCAPS_CONTINUOUSRATE != 0 {
        let inside = RATES.into_iter().find(|rate| {
            *rate >= caps.dwMinSecondarySampleRate && *rate <= caps.dwMaxSecondarySampleRate
        });
        return inside.unwrap_or(caps.dwMaxSecondarySampleRate);
    }
    let min = caps.dwMinSecondarySampleRate;
    let max = caps.dwMaxSecondarySampleRate;
    if min == max {
        if min == 0 {
            48_000
        } else {
            max
        }
    } else if min < 1_000 && max > 50_000 {
        48_000
    } else {
        max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_rates_come_from_the_announced_bits() {
        assert_eq!(capture_rate(2, WAVE_FORMAT_4S16), 44_100);
        assert_eq!(capture_rate(2, WAVE_FORMAT_48S16), 48_000);
        assert_eq!(capture_rate(1, WAVE_FORMAT_4M16), 44_100);
        assert_eq!(capture_rate(1, WAVE_FORMAT_96M16), 96_000);
        assert_eq!(capture_rate(2, 0), 48_000);
        assert_eq!(capture_rate(4, WAVE_FORMAT_4S16), 48_000);
    }

    #[test]
    fn output_rates_follow_the_reported_range() {
        let mut caps = DSCAPS {
            dwMinSecondarySampleRate: 44_100,
            dwMaxSecondarySampleRate: 48_000,
            ..Default::default()
        };
        assert_eq!(output_rate(&caps), 48_000);
        caps.dwFlags = DSCAPS_CONTINUOUSRATE;
        caps.dwMinSecondarySampleRate = 8_000;
        caps.dwMaxSecondarySampleRate = 96_000;
        assert_eq!(output_rate(&caps), 44_100);
        // Un rango ridículo (100..100000) se trata como rango y da 48 000.
        caps.dwFlags = 0;
        caps.dwMinSecondarySampleRate = 100;
        caps.dwMaxSecondarySampleRate = 100_000;
        assert_eq!(output_rate(&caps), 48_000);
        caps.dwMinSecondarySampleRate = 0;
        caps.dwMaxSecondarySampleRate = 0;
        assert_eq!(output_rate(&caps), 48_000);
    }
}

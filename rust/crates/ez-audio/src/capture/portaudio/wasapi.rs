//! Bloque de WASAPI de PortAudio (`pa_win_wasapi.c`).
//!
//! PortAudio enumera los endpoints activos con `EnumAudioEndpoints(eAll)`, que
//! devuelve primero los de reproducción y después los de captura. El nombre es
//! el `PKEY_Device_FriendlyName` del endpoint; la tasa y los canales, el
//! formato de mezcla del motor de audio (`GetMixFormat`).

use std::ffi::c_void;

use windows::core::{Interface, GUID};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    eAll, eCapture, IAudioClient, IMMDevice, IMMDeviceEnumerator, IMMEndpoint, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED, STGM_READ,
};
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;

use super::{push_device, PortAudioDevice};

/// `PKEY_Device_FriendlyName`, el nombre con el que Windows presenta el
/// endpoint (`<pin> (<dispositivo>)`).
const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};

/// `PA_WASAPI_DEVICE_NAME_LEN - 1`: PortAudio corta el nombre a 127 bytes.
const NAME_LIMIT: usize = 127;

/// Añade los endpoints activos de WASAPI, primero las salidas y luego las
/// entradas.
pub(super) fn append(devices: &mut Vec<PortAudioDevice>) {
    // SAFETY: COM por hilo; si ya estaba inicializado con otro modelo,
    // `CoInitializeEx` falla y no hay que deshacer nada.
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    enumerate(devices);
    if initialized {
        // SAFETY: equilibra el `CoInitializeEx` de esta misma función.
        unsafe { CoUninitialize() };
    }
}

fn enumerate(devices: &mut Vec<PortAudioDevice>) {
    // SAFETY: COM ya está inicializado en este hilo y el CLSID es válido.
    let Ok(enumerator) = (unsafe {
        CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
    }) else {
        return;
    };
    // SAFETY: solo lee la colección de endpoints.
    let Ok(collection) = (unsafe { enumerator.EnumAudioEndpoints(eAll, DEVICE_STATE_ACTIVE) })
    else {
        return;
    };
    // SAFETY: solo lee el número de endpoints de la colección.
    let count = unsafe { collection.GetCount() }.unwrap_or(0);
    for position in 0..count {
        // SAFETY: `position` está dentro de la colección.
        let Ok(device) = (unsafe { collection.Item(position) }) else {
            continue;
        };
        match endpoint_info(&device, position) {
            Some((name, rate, channels, input)) => {
                push_device(devices, name, rate, channels, input, 0, false);
            }
            // PortAudio deja el hueco del índice con un dispositivo inactivo.
            None => push_device(devices, String::new(), 0, 0, false, 0, false),
        }
    }
}

/// Nombre, formato de mezcla y sentido de un endpoint.
fn endpoint_info(device: &IMMDevice, position: u32) -> Option<(String, u32, u16, bool)> {
    let name = friendly_name(device).unwrap_or_else(|| format!("baddev{position}"));
    let endpoint: IMMEndpoint = device.cast().ok()?;
    // SAFETY: el endpoint está vivo y `GetDataFlow` solo escribe su salida.
    let flow = unsafe { endpoint.GetDataFlow() }.ok()?;
    // SAFETY: el dispositivo está vivo; la activación devuelve su cliente.
    let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }.ok()?;
    // SAFETY: `GetMixFormat` reserva el bloque con el asignador COM; se copia y
    // se libera con `CoTaskMemFree`.
    let format = unsafe { client.GetMixFormat() }.ok()?;
    let (rate, channels) = unsafe { ((*format).nSamplesPerSec, (*format).nChannels) };
    unsafe { CoTaskMemFree(Some(format as *const c_void)) };
    Some((truncate(name), rate, channels, flow == eCapture))
}

/// `PKEY_Device_FriendlyName` como texto.
fn friendly_name(device: &IMMDevice) -> Option<String> {
    // SAFETY: el almacén de propiedades solo se lee.
    let store: IPropertyStore = unsafe { device.OpenPropertyStore(STGM_READ) }.ok()?;
    // SAFETY: el PROPVARIANT devuelto se libera con `PropVariantClear`.
    let mut value: PROPVARIANT = unsafe { store.GetValue(&PKEY_DEVICE_FRIENDLY_NAME) }.ok()?;
    // SAFETY: solo se lee `pwszVal` cuando `vt` dice que es una cadena.
    let text = unsafe {
        if value.Anonymous.Anonymous.vt == VT_LPWSTR {
            value.Anonymous.Anonymous.Anonymous.pwszVal.to_string().ok()
        } else {
            None
        }
    };
    // SAFETY: libera las cadenas que reservó `GetValue`.
    let _ = unsafe { PropVariantClear(&mut value) };
    text
}

/// Recorte a 127 bytes en una frontera de carácter, como el `memcpy` de
/// PortAudio (que puede partir un carácter multibyte).
fn truncate(name: String) -> String {
    if name.len() <= NAME_LIMIT {
        return name;
    }
    let mut end = NAME_LIMIT;
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_names_are_cut_like_portaudio() {
        assert_eq!(truncate("corto".to_string()), "corto");
        let long = "á".repeat(100);
        let cut = truncate(long);
        assert!(cut.len() <= NAME_LIMIT);
        assert!(cut.chars().all(|character| character == 'á'));
    }

    #[test]
    fn the_friendly_name_key_is_the_documented_one() {
        assert_eq!(PKEY_DEVICE_FRIENDLY_NAME.pid, 14);
    }
}

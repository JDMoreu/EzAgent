//! Bloque de WDM-KS de PortAudio (`pa_win_wdmks.c`).
//!
//! PortAudio enumera los filtros KS de `KSCATEGORY_AUDIO` que tienen alias de
//! reproducción o captura (con SetupAPI, en el orden del registro) y, dentro de
//! cada filtro, sus pines de streaming en orden. El nombre del dispositivo es
//! `<nombre del pin> (<nombre del filtro>)`; la tasa y los canales salen de
//! `KSPROPERTY_PIN_DATARANGES`. Los pines con multiplexor dan un dispositivo por
//! entrada del mux.

mod ks;
mod topology;

use std::collections::HashMap;
use std::mem::size_of;

use windows::core::{w, GUID, PCWSTR};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
    SetupDiGetDeviceInterfaceAlias, SetupDiGetDeviceInterfaceDetailW,
    SetupDiOpenDeviceInterfaceRegKey, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO,
    SPINT_REMOVED, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Media::KernelStreaming::{
    KSCATEGORY_AUDIO, KSCATEGORY_CAPTURE, KSCATEGORY_REALTIME, KSCATEGORY_RENDER,
    KSPIN_DATAFLOW_OUT,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_MODE, OPEN_EXISTING,
};
use windows::Win32::System::Registry::{RegCloseKey, RegQueryValueExW, KEY_QUERY_VALUE};

use super::{push_device, wide_name, PortAudioDevice};

/// Un filtro KS con sus pines de streaming válidos.
pub(super) struct Filter {
    pub friendly: String,
    pub pins: Vec<Pin>,
}

pub(super) struct Pin {
    pub data_flow: u32,
    pub channels: u16,
    pub rate: u32,
    /// Un nombre por dispositivo: uno solo, o uno por entrada del mux.
    pub names: Vec<String>,
}

impl Pin {
    fn is_input(&self) -> bool {
        self.data_flow == KSPIN_DATAFLOW_OUT.0 as u32
    }
}

/// Añade los dispositivos de WDM-KS, en el orden de los filtros y sus pines.
pub(super) fn append(devices: &mut Vec<PortAudioDevice>) {
    for (name, rate, channels, input) in devices_from_filters(&enumerate_filters()) {
        push_device(devices, name, rate, channels, input, 0, false);
    }
}

/// Compone los nombres `pin (filtro)` y resuelve los nombres repetidos de un
/// mismo filtro con el sufijo ` n` de `ScanDeviceInfos`.
fn devices_from_filters(filters: &[Filter]) -> Vec<(String, u32, u16, bool)> {
    let mut devices = Vec::new();
    for filter in filters {
        let mut total: HashMap<(String, bool), usize> = HashMap::new();
        for pin in &filter.pins {
            for name in &pin.names {
                *total.entry((name.clone(), pin.is_input())).or_default() += 1;
            }
        }
        let mut seen: HashMap<(String, bool), usize> = HashMap::new();
        for pin in &filter.pins {
            for name in &pin.names {
                let key = (name.clone(), pin.is_input());
                let index = seen.entry(key.clone()).or_default();
                *index += 1;
                let local = if total.get(&key).copied().unwrap_or(0) > 1 {
                    format!("{name} {index}")
                } else {
                    name.clone()
                };
                devices.push((
                    format!("{local} ({})", filter.friendly),
                    pin.rate,
                    pin.channels,
                    pin.is_input(),
                ));
            }
        }
    }
    devices
}

/// Enumera los filtros KS con alias de reproducción o captura.
fn enumerate_filters() -> Vec<Filter> {
    let mut filters = Vec::new();
    // SAFETY: SetupAPI devuelve un HDEVINFO que se destruye al final.
    let Ok(handle) = (unsafe {
        SetupDiGetClassDevsW(
            Some(&KSCATEGORY_AUDIO),
            PCWSTR::null(),
            None,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    }) else {
        return filters;
    };
    let mut member = 0;
    loop {
        let mut data = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        // SAFETY: `data` anuncia su tamaño y `member` recorre la lista.
        if unsafe {
            SetupDiEnumDeviceInterfaces(handle, None, &KSCATEGORY_AUDIO, member, &mut data)
        }
        .is_err()
        {
            break;
        }
        member += 1;
        let render = has_alias(handle, &data, &KSCATEGORY_RENDER);
        let capture = has_alias(handle, &data, &KSCATEGORY_CAPTURE);
        if !render && !capture {
            continue;
        }
        let wave_rt = has_alias(handle, &data, &KSCATEGORY_REALTIME);
        let Some(path) = device_path(handle, &data) else {
            continue;
        };
        let friendly = interface_friendly_name(handle, &data);
        if let Some(filter) = open_filter(&path, friendly, wave_rt) {
            filters.push(filter);
        }
    }
    // SAFETY: `handle` viene de `SetupDiGetClassDevsW` y ya no se usa.
    let _ = unsafe { SetupDiDestroyDeviceInfoList(handle) };
    filters
}

/// `SetupDiGetDeviceInterfaceAlias`: el alias existe y no está retirado.
fn has_alias(handle: HDEVINFO, data: &SP_DEVICE_INTERFACE_DATA, class: &GUID) -> bool {
    let mut alias = SP_DEVICE_INTERFACE_DATA {
        cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
        ..Default::default()
    };
    // SAFETY: `alias` anuncia su tamaño y solo se lee `Flags`.
    if unsafe { SetupDiGetDeviceInterfaceAlias(handle, data, class, &mut alias) }.is_err() {
        return false;
    }
    alias.Flags != 0 && alias.Flags & SPINT_REMOVED == 0
}

/// `SP_DEVICE_INTERFACE_DETAIL_DATA_W.DevicePath` del miembro enumerado.
fn device_path(handle: HDEVINFO, data: &SP_DEVICE_INTERFACE_DATA) -> Option<String> {
    let mut buffer = [0_u8; 1024];
    let detail = buffer.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
    // SAFETY: `detail` apunta al buffer y `cbSize` es el de la cabecera.
    unsafe { (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32 };
    // SAFETY: el buffer es mayor que la ruta de cualquier interfaz.
    let result = unsafe {
        SetupDiGetDeviceInterfaceDetailW(
            handle,
            data,
            Some(detail),
            buffer.len() as u32,
            None,
            None,
        )
    };
    result.ok()?;
    // SAFETY: `DevicePath` es la cadena UTF-16 que escribió SetupAPI.
    unsafe { PCWSTR((*detail).DevicePath.as_ptr()).to_string() }.ok()
}

/// `FriendlyName` del registro de la interfaz, o vacío (como PortAudio).
fn interface_friendly_name(handle: HDEVINFO, data: &SP_DEVICE_INTERFACE_DATA) -> String {
    // SAFETY: la clave pertenece al miembro enumerado y se cierra abajo.
    let Ok(key) =
        (unsafe { SetupDiOpenDeviceInterfaceRegKey(handle, data, None, KEY_QUERY_VALUE.0) })
    else {
        return String::new();
    };
    let mut buffer = [0_u16; 512];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: `buffer` es mayor que el nombre; `size` recibe los bytes escritos.
    let result = unsafe {
        RegQueryValueExW(
            key,
            w!("FriendlyName"),
            None,
            None,
            Some(buffer.as_mut_ptr() as *mut u8),
            Some(&mut size),
        )
    };
    // SAFETY: `key` viene de `SetupDiOpenDeviceInterfaceRegKey`.
    let _ = unsafe { RegCloseKey(key) };
    if !result.is_ok() {
        return String::new();
    }
    let units = (size as usize / 2).min(buffer.len());
    wide_name(&buffer[..units]).trim().to_string()
}

/// `FilterNew`: abre el filtro y crea sus pines válidos.
fn open_filter(path: &str, friendly: String, wave_rt: bool) -> Option<Filter> {
    let handle = open_ks(path)?;
    let filter = topology::build(handle, friendly, wave_rt);
    // SAFETY: PortAudio cierra el filtro al terminar la enumeración.
    let _ = unsafe { CloseHandle(handle) };
    filter
}

fn open_ks(path: &str) -> Option<HANDLE> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: la ruta vive durante la llamada; se abre para IOCTL de lectura.
    // DECISION: sin `FILE_FLAG_OVERLAPPED`; PortAudio lo usa, pero aquí los
    // IOCTL son síncronos y Windows exige `OVERLAPPED` en handles asíncronos.
    unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            GENERIC_READ.0 | GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Media::KernelStreaming::KSPIN_DATAFLOW_IN;

    fn pin(data_flow: u32, names: &[&str]) -> Pin {
        Pin {
            data_flow,
            channels: 2,
            rate: 48_000,
            names: names.iter().map(|name| name.to_string()).collect(),
        }
    }

    fn filter(friendly: &str, pins: Vec<Pin>) -> Filter {
        Filter {
            friendly: friendly.to_string(),
            pins,
        }
    }

    #[test]
    fn composite_names_follow_the_portaudio_rule() {
        let filters = [filter(
            "TA82",
            vec![
                pin(KSPIN_DATAFLOW_IN.0 as u32, &["Altavoces"]),
                pin(KSPIN_DATAFLOW_OUT.0 as u32, &["Micrófono", "Micrófono"]),
            ],
        )];
        let devices = devices_from_filters(&filters);
        assert_eq!(
            devices[0],
            ("Altavoces (TA82)".to_string(), 48_000, 2, false)
        );
        assert_eq!(
            devices[1],
            ("Micrófono 1 (TA82)".to_string(), 48_000, 2, true)
        );
        assert_eq!(
            devices[2],
            ("Micrófono 2 (TA82)".to_string(), 48_000, 2, true)
        );
    }

    #[test]
    fn an_empty_filter_name_still_gets_the_parentheses() {
        let filters = [filter(
            "",
            vec![pin(KSPIN_DATAFLOW_IN.0 as u32, &["Auriculares"])],
        )];
        assert_eq!(
            devices_from_filters(&filters)[0].0,
            "Auriculares ()".to_string()
        );
    }
}

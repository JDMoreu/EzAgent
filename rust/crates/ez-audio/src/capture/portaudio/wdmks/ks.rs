//! Propiedades KS de un filtro o un pin (`DeviceIoControl`/`IOCTL_KS_PROPERTY`).

use std::ffi::c_void;
use std::mem::size_of;

use windows::core::GUID;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Media::KernelStreaming::{
    KSPROPSETID_Pin, IOCTL_KS_PROPERTY, KSIDENTIFIER, KSIDENTIFIER_0, KSIDENTIFIER_0_0,
    KSPROPERTY_PIN, KSPROPERTY_PIN_CATEGORY, KSPROPERTY_PIN_DATARANGES, KSPROPERTY_PIN_NAME,
    KSPROPERTY_PIN_PHYSICALCONNECTION, KSPROPERTY_TYPE_GET, KSP_PIN, KSP_PIN_0,
};
use windows::Win32::System::IO::DeviceIoControl;

use super::super::wide_name;

/// `defaultSampleRateSearchOrder[]` de `pa_win_wdmks.c`, en su orden.
const RATES: [u32; 13] = [
    44_100, 48_000, 88_200, 96_000, 192_000, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 9_600,
    8_000,
];

/// Los terminales USB que `GetNameFromCategory` sabe nombrar.
const TERMINAL_NAMES: [(u16, &str); 18] = [
    (0x0201, "Microphone"),
    (0x0202, "Desktop Microphone"),
    (0x0203, "Personal Microphone"),
    (0x0204, "Omni Directional Microphone"),
    (0x0205, "Microphone Array"),
    (0x0206, "Processing Microphone Array"),
    (0x0301, "Speakers"),
    (0x0302, "Headphones"),
    (0x0303, "Head Mounted Display Audio"),
    (0x0304, "Desktop Speaker"),
    (0x0305, "Room Speaker"),
    (0x0306, "Communication Speaker"),
    (0x0307, "LFE Speakers"),
    (0x0601, "Analog"),
    (0x0602, "Digital"),
    (0x0603, "Line"),
    (0x0604, "Audio"),
    (0x0605, "SPDIF"),
];

/// `KSPROPERTY_PIN_PHYSICALCONNECTION`: el filtro de topología y su pin.
pub(super) struct Physical {
    pub path: String,
    pub pin: u32,
}

/// Una propiedad KS de filtro o de pin, con un buffer de tamaño holgado.
pub(super) fn property_bytes(
    handle: HANDLE,
    pin: Option<u32>,
    set: GUID,
    id: u32,
) -> Option<Vec<u8>> {
    let mut output = vec![0_u8; 8192];
    let mut returned = 0_u32;
    match pin {
        Some(pin) => {
            let input = KSP_PIN {
                Property: ks_identifier(set, id),
                PinId: pin,
                Anonymous: KSP_PIN_0 { Reserved: 0 },
            };
            ioctl(
                handle,
                &input as *const _ as *const c_void,
                size_of::<KSP_PIN>() as u32,
                &mut output,
                &mut returned,
            )?;
        }
        None => {
            let input = ks_identifier(set, id);
            ioctl(
                handle,
                &input as *const _ as *const c_void,
                size_of::<KSIDENTIFIER>() as u32,
                &mut output,
                &mut returned,
            )?;
        }
    }
    output.truncate(returned as usize);
    Some(output)
}

fn ks_identifier(set: GUID, id: u32) -> KSIDENTIFIER {
    KSIDENTIFIER {
        Anonymous: KSIDENTIFIER_0 {
            Anonymous: KSIDENTIFIER_0_0 {
                Set: set,
                Id: id,
                Flags: KSPROPERTY_TYPE_GET,
            },
        },
    }
}

fn ioctl(
    handle: HANDLE,
    input: *const c_void,
    input_len: u32,
    output: &mut [u8],
    returned: &mut u32,
) -> Option<()> {
    // SAFETY: el IOCTL solo escribe en `output`, que vive durante la llamada.
    unsafe {
        DeviceIoControl(
            handle,
            IOCTL_KS_PROPERTY,
            Some(input),
            input_len,
            Some(output.as_mut_ptr() as *mut c_void),
            output.len() as u32,
            Some(returned),
            None,
        )
    }
    .ok()
}

/// Una propiedad de pin que devuelve un `u32` (comunicación, dataflow, …).
pub(super) fn pin_u32(handle: HANDLE, pin: u32, property: KSPROPERTY_PIN) -> Option<u32> {
    let bytes = property_bytes(handle, Some(pin), KSPROPSETID_Pin, property.0 as u32)?;
    read_u32(&bytes, 0)
}

/// Una lista `KSMULTIPLE_ITEM` con entradas de tipo `T`.
pub(super) fn property_multi<T: Copy>(
    handle: HANDLE,
    pin: Option<u32>,
    set: GUID,
    id: u32,
) -> Option<Vec<T>> {
    let bytes = property_bytes(handle, pin, set, id)?;
    let count = read_u32(&bytes, 4)? as usize;
    let size = size_of::<T>();
    let mut items = Vec::with_capacity(count);
    for index in 0..count {
        let offset = 8 + index * size;
        let slice = bytes.get(offset..offset + size)?;
        // SAFETY: `slice` tiene el tamaño de `T` y `read_unaligned` admite
        // cualquier alineación.
        items.push(unsafe { std::ptr::read_unaligned(slice.as_ptr() as *const T) });
    }
    Some(items)
}

/// `KSPROPERTY_PIN_INTERFACES`/`MEDIUMS`: busca el conjunto y el id pedidos.
pub(super) fn pin_has_identifier(
    handle: HANDLE,
    pin: u32,
    property: KSPROPERTY_PIN,
    set: GUID,
    id: u32,
) -> bool {
    let Some(bytes) = property_bytes(handle, Some(pin), KSPROPSETID_Pin, property.0 as u32) else {
        return false;
    };
    identifiers(&bytes)
        .iter()
        .any(|(entry_set, entry_id)| *entry_set == set && *entry_id == id)
}

/// Las entradas de `KSPROPERTY_PIN_INTERFACES`/`MEDIUMS` (conjunto e id).
fn identifiers(bytes: &[u8]) -> Vec<(GUID, u32)> {
    let Some(count) = read_u32(bytes, 4) else {
        return Vec::new();
    };
    (0..count as usize)
        .filter_map(|index| {
            let offset = 8 + index * 24;
            Some((read_guid(bytes, offset)?, read_u32(bytes, offset + 16)?))
        })
        .collect()
}

/// `KSPROPERTY_PIN_DATARANGES`: máximo de canales y primera tasa de PortAudio.
pub(super) fn data_ranges(handle: HANDLE, pin: u32) -> Option<(u16, u32)> {
    let bytes = property_bytes(
        handle,
        Some(pin),
        KSPROPSETID_Pin,
        KSPROPERTY_PIN_DATARANGES.0 as u32,
    )?;
    let size = read_u32(&bytes, 0)? as usize;
    let count = read_u32(&bytes, 4)? as usize;
    let end = size.min(bytes.len());
    let mut channels = 0_u32;
    let mut rate_index: Option<usize> = None;
    let mut offset = 8;
    for _ in 0..count {
        if offset + 64 > end {
            break;
        }
        let format_size = read_u32(&bytes, offset)? as usize;
        if format_size < 64 || offset + format_size > end {
            break;
        }
        let maximum = read_u32(&bytes, offset + 64)?;
        channels = channels.max(if maximum == u32::MAX { 256 } else { maximum });
        if let (Some(min), Some(max)) =
            (read_u32(&bytes, offset + 76), read_u32(&bytes, offset + 80))
        {
            if let Some(index) = RATES.iter().position(|rate| *rate >= min && *rate <= max) {
                rate_index = Some(rate_index.map_or(index, |current| current.min(index)));
            }
        }
        offset += format_size;
    }
    Some((channels.min(u32::from(u16::MAX)) as u16, RATES[rate_index?]))
}

/// `KSPROPERTY_PIN_NAME`, que es el nombre localizado que pone el controlador.
pub(super) fn pin_name(handle: HANDLE, pin: u32) -> Option<String> {
    let bytes = property_bytes(
        handle,
        Some(pin),
        KSPROPSETID_Pin,
        KSPROPERTY_PIN_NAME.0 as u32,
    )?;
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_ne_bytes(*pair))
        .collect();
    let name = wide_name(&units);
    (!name.is_empty()).then_some(name)
}

/// `GetNameFromCategory`: los nombres en inglés de los terminales USB.
pub(super) fn category_name(handle: HANDLE, pin: u32, input: bool) -> Option<String> {
    let bytes = property_bytes(
        handle,
        Some(pin),
        KSPROPSETID_Pin,
        KSPROPERTY_PIN_CATEGORY.0 as u32,
    )?;
    let guid = read_guid(&bytes, 0)?;
    let mut terminal = (guid.data1.wrapping_sub(0xDFF2_19E0) & 0xFFFF) as u16;
    if input && (0x301..0x400).contains(&terminal) {
        terminal = 0x603;
    }
    if !input && (0x201..0x300).contains(&terminal) {
        terminal = 0x603;
    }
    let name = TERMINAL_NAMES
        .iter()
        .find(|(value, _)| *value == terminal)
        .map(|(_, name)| *name)?;
    let mut name = name.to_string();
    if (0x601..0x700).contains(&terminal) {
        name.push_str(if input { " In" } else { " Out" });
    }
    Some(name)
}

/// `KSPROPERTY_PIN_PHYSICALCONNECTION` del filtro de streaming.
pub(super) fn physical_connection(handle: HANDLE, pin: u32) -> Option<Physical> {
    let bytes = property_bytes(
        handle,
        Some(pin),
        KSPROPSETID_Pin,
        KSPROPERTY_PIN_PHYSICALCONNECTION.0 as u32,
    )?;
    let pin = read_u32(&bytes, 4)?;
    let units: Vec<u16> = bytes
        .get(8..)
        .unwrap_or(&[])
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_ne_bytes(*pair))
        .collect();
    let mut path = wide_name(&units);
    if path.as_bytes().get(1) == Some(&b'?') {
        path.replace_range(1..2, "\\");
    }
    Some(Physical { path, pin })
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_ne_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_guid(bytes: &[u8], offset: usize) -> Option<GUID> {
    let data1 = read_u32(bytes, offset)?;
    let data2 = read_u32(bytes, offset + 4)? as u16;
    let data3 = read_u32(bytes, offset + 6)? as u16;
    let data4: [u8; 8] = bytes.get(offset + 8..offset + 16)?.try_into().ok()?;
    Some(GUID::from_values(data1, data2, data3, data4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_read_from_the_multiple_item() {
        let set = GUID::from_u128(0x1a8766a0_62ce_11cf_a5d6_28db04c10000);
        let mut bytes = vec![0_u8; 8 + 24];
        bytes[4..8].copy_from_slice(&1_u32.to_ne_bytes());
        bytes[8..24].copy_from_slice(&[
            0xa0, 0x66, 0x87, 0x1a, 0xce, 0x62, 0xcf, 0x11, 0xa5, 0xd6, 0x28, 0xdb, 0x04, 0xc1,
            0x00, 0x00,
        ]);
        bytes[24..28].copy_from_slice(&0_u32.to_ne_bytes());
        assert_eq!(identifiers(&bytes), vec![(set, 0)]);
    }

    #[test]
    fn a_guid_is_read_with_its_windows_layout() {
        let bytes = [
            0x04, 0xad, 0x94, 0x69, 0xef, 0x93, 0xd0, 0x11, 0xa3, 0xcc, 0x00, 0xa0, 0xc9, 0x22,
            0x31, 0x96,
        ];
        assert_eq!(
            read_guid(&bytes, 0),
            Some(GUID::from_u128(0x6994ad04_93ef_11d0_a3cc_00a0c9223196))
        );
    }

    #[test]
    fn u32_reads_are_bounds_checked() {
        assert_eq!(read_u32(&[1, 0, 0, 0], 0), Some(1));
        assert_eq!(read_u32(&[1, 0], 0), None);
    }
}

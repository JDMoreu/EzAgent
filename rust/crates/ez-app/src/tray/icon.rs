//! Conversión de la imagen RGBA a `HICON` para `Shell_NotifyIconW`.

use std::ffi::c_void;
use std::mem::size_of;

use windows::core::BOOL;
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPV5HEADER, BI_BITFIELDS,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

use super::IconImage;

const LCS_WINDOWS_COLOR_SPACE: u32 = 0x7352_4742;

pub(super) fn icon_from_rgba(image: &IconImage) -> Option<HICON> {
    let width = image.width as i32;
    let height = image.height as i32;
    if width <= 0 || height <= 0 {
        return None;
    }
    let header = BITMAPV5HEADER {
        bV5Size: size_of::<BITMAPV5HEADER>() as u32,
        bV5Width: width,
        bV5Height: -height,
        bV5Planes: 1,
        bV5BitCount: 32,
        bV5Compression: BI_BITFIELDS,
        bV5RedMask: 0x00FF_0000,
        bV5GreenMask: 0x0000_FF00,
        bV5BlueMask: 0x0000_00FF,
        bV5AlphaMask: 0xFF00_0000,
        bV5CSType: LCS_WINDOWS_COLOR_SPACE,
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    // SAFETY: `header` es válido; el DIB creado es nuestro hasta borrarlo.
    let color = unsafe {
        CreateDIBSection(
            None,
            (&header as *const BITMAPV5HEADER).cast::<BITMAPINFO>(),
            DIB_RGB_COLORS,
            &mut bits,
            None,
            0,
        )
        .ok()?
    };
    if bits.is_null() {
        // SAFETY: `color` es un HBITMAP nuestro.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(color.0));
        }
        return None;
    }
    // SAFETY: CreateDIBSection reservó ancho×alto×4 bytes.
    let pixels = unsafe {
        std::slice::from_raw_parts_mut(bits.cast::<u8>(), (width as usize) * (height as usize) * 4)
    };
    for (source, target) in image
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(pixels.as_chunks_mut::<4>().0)
    {
        target[0] = source[2];
        target[1] = source[1];
        target[2] = source[0];
        target[3] = source[3];
    }
    let stride = (width as usize).div_ceil(32) * 4;
    let mask_bits = vec![0u8; stride * height as usize];
    // SAFETY: `mask_bits` es válido y CreateBitmap copia los bits.
    let mask = unsafe { CreateBitmap(width, height, 1, 1, Some(mask_bits.as_ptr().cast())) };
    if mask.is_invalid() {
        // SAFETY: `color` es un HBITMAP nuestro.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(color.0));
        }
        return None;
    }
    let info = ICONINFO {
        fIcon: BOOL::from(true),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    // SAFETY: ambos bitmaps son válidos y CreateIconIndirect copia sus bits.
    let icon = unsafe { CreateIconIndirect(&info).ok() };
    // SAFETY: el icono ya copió los bitmaps; se pueden liberar.
    unsafe {
        let _ = DeleteObject(HGDIOBJ(mask.0));
        let _ = DeleteObject(HGDIOBJ(color.0));
    }
    icon
}

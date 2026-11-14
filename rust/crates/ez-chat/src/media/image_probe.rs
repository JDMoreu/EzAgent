//! Lectura de las dimensiones de una imagen desde su cabecera.

pub(super) fn probe(bytes: &[u8]) -> Option<(u32, u32, &'static str)> {
    if bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR" {
        return Some((u32_be(bytes, 16)?, u32_be(bytes, 20)?, "png"));
    }
    if bytes.len() >= 10 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return Some((
            u32::from(u16_le(bytes, 6)?),
            u32::from(u16_le(bytes, 8)?),
            "gif",
        ));
    }
    if bytes.len() >= 26 && bytes.starts_with(b"BM") {
        let header = u32_le(bytes, 14)?;
        let (width, height) = if header == 12 {
            (u32::from(u16_le(bytes, 18)?), u32::from(u16_le(bytes, 20)?))
        } else {
            (
                i32_le(bytes, 18)?.unsigned_abs(),
                i32_le(bytes, 22)?.unsigned_abs(),
            )
        };
        if width > 0 && height > 0 {
            return Some((width, height, "bmp"));
        }
        return None;
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        return jpeg_size(bytes).map(|(width, height)| (width, height, "jpeg"));
    }
    if bytes.len() >= 30 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return webp_size(bytes).map(|(width, height)| (width, height, "webp"));
    }
    None
}

fn u16_le(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn i32_le(bytes: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn u32_be(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn u16_be(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut index = 2usize;
    while index + 3 < bytes.len() {
        if bytes[index] != 0xff {
            index += 1;
            continue;
        }
        let mut marker = index + 1;
        while bytes.get(marker) == Some(&0xff) {
            marker += 1;
        }
        let kind = *bytes.get(marker)?;
        index = marker + 1;
        if matches!(kind, 0xd8 | 0xd9 | 0x01 | 0xd0..=0xd7) {
            continue;
        }
        let length = usize::from(u16_be(bytes, index)?);
        if length < 2 || index + length > bytes.len() {
            return None;
        }
        if matches!(
            kind,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) {
            let height = u32::from(u16_be(bytes, index + 3)?);
            let width = u32::from(u16_be(bytes, index + 5)?);
            return Some((width, height));
        }
        index += length;
    }
    None
}

fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    match &bytes[12..16] {
        b"VP8 " => {
            if bytes.get(23..26) != Some(&[0x9d, 0x01, 0x2a]) {
                return None;
            }
            Some((
                u32::from(u16_le(bytes, 26)? & 0x3fff),
                u32::from(u16_le(bytes, 28)? & 0x3fff),
            ))
        }
        b"VP8L" => {
            if bytes.get(20) != Some(&0x2f) {
                return None;
            }
            let bits = u32_le(bytes, 21)?;
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        b"VP8X" => {
            let width =
                u32::from(bytes[24]) | (u32::from(bytes[25]) << 8) | (u32::from(bytes[26]) << 16);
            let height =
                u32::from(bytes[27]) | (u32::from(bytes[28]) << 8) | (u32::from(bytes[29]) << 16);
            Some((width + 1, height + 1))
        }
        _ => None,
    }
}

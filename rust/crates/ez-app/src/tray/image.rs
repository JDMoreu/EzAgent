//! Icono de la bandeja: decodificación PNG, respaldo dibujado y escalado.
//!
//! Parte de `tray.rs` (el original Python usa Pillow); aquí solo entra `png`
//! para leer `assets/icon.png` y un bilineal propio para dejarlo en 64 px.

use std::path::Path;

use super::{ICON_SIZE, INK, INK_TOP, MINT};

/// Imagen RGBA del icono, ya en 64×64 y con el canal alfa listo para Win32.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl IconImage {
    /// Píxel en coordenadas de la imagen; útil para inspeccionar el icono.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * self.width + x) * 4) as usize;
        [
            self.rgba[index],
            self.rgba[index + 1],
            self.rgba[index + 2],
            self.rgba[index + 3],
        ]
    }
}

/// Carga el icono del proyecto o dibuja el respaldo, como `load_icon_image`.
pub fn load_icon_image(path: Option<&Path>) -> IconImage {
    if let Some(path) = path {
        if path.exists() {
            if let Ok(bytes) = std::fs::read(path) {
                if let Some(image) = decode_png(&bytes) {
                    return resize_rgba(&image, ICON_SIZE);
                }
            }
        }
    }
    fallback_image()
}

fn decode_png(bytes: &[u8]) -> Option<IconImage> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let pixels = (info.width as usize) * (info.height as usize);
    let data = &buffer[..info.buffer_size()];
    let mut rgba = vec![0u8; pixels * 4];
    match info.color_type {
        png::ColorType::Rgba => {
            if data.len() != rgba.len() {
                return None;
            }
            rgba.copy_from_slice(data);
        }
        png::ColorType::Rgb => {
            for (source, target) in data
                .as_chunks::<3>()
                .0
                .iter()
                .zip(rgba.as_chunks_mut::<4>().0)
            {
                target[..3].copy_from_slice(source);
                target[3] = 255;
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for (source, target) in data
                .as_chunks::<2>()
                .0
                .iter()
                .zip(rgba.as_chunks_mut::<4>().0)
            {
                target[0] = source[0];
                target[1] = source[0];
                target[2] = source[0];
                target[3] = source[1];
            }
        }
        png::ColorType::Grayscale => {
            for (source, target) in data
                .as_chunks::<1>()
                .0
                .iter()
                .zip(rgba.as_chunks_mut::<4>().0)
            {
                target[0] = source[0];
                target[1] = source[0];
                target[2] = source[0];
                target[3] = 255;
            }
        }
        png::ColorType::Indexed => return None,
    }
    Some(IconImage {
        width: info.width,
        height: info.height,
        rgba,
    })
}

fn resize_rgba(image: &IconImage, size: u32) -> IconImage {
    if image.width == size && image.height == size {
        return image.clone();
    }
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let scale_x = image.width as f32 / size as f32;
    let scale_y = image.height as f32 / size as f32;
    for y in 0..size {
        let sy = ((y as f32 + 0.5) * scale_y - 0.5).clamp(0.0, image.height as f32 - 1.0);
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(image.height - 1);
        let fy = sy - y0 as f32;
        for x in 0..size {
            let sx = ((x as f32 + 0.5) * scale_x - 0.5).clamp(0.0, image.width as f32 - 1.0);
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(image.width - 1);
            let fx = sx - x0 as f32;
            let mut pixel = [0u8; 4];
            for (channel, value) in pixel.iter_mut().enumerate() {
                let at = |px: u32, py: u32| -> f32 {
                    image.rgba[((py * image.width + px) * 4) as usize + channel] as f32
                };
                let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
                let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
                let mixed = top * (1.0 - fy) + bottom * fy;
                // DECISION: Pillow usa LANCZOS; un bilineal basta para un icono
                // de 64 px y evita depender de `image` en este crate.
                *value = mixed.round().clamp(0.0, 255.0) as u8;
            }
            let index = ((y * size + x) * 4) as usize;
            rgba[index..index + 4].copy_from_slice(&pixel);
        }
    }
    IconImage {
        width: size,
        height: size,
        rgba,
    }
}

// DECISION: `brand.mark_image` aún no está portado (F2-11, esqueleto vacío);
// el respaldo dibuja el mismo cuadrado de grafito con dos gotas de menta.
fn fallback_image() -> IconImage {
    let side = ICON_SIZE as f32;
    let radius = 0.22 * side;
    let head_radius = 0.125 * side;
    let head_y = 0.60 * side;
    let head_offset = (0.28 / 2.0 + 0.005) * side;
    let heads = [
        (side / 2.0 - head_offset, head_y),
        (side / 2.0 + head_offset, head_y),
    ];
    let mut rgba = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let tile = rounded_coverage(px, py, side, radius);
            if tile <= 0.0 {
                continue;
            }
            let base = mix(INK_TOP, INK, (py / side).clamp(0.0, 1.0));
            let drop = heads.iter().fold(0.0f32, |coverage, (hx, hy)| {
                coverage.max(circle_coverage(px, py, *hx, *hy, head_radius))
            });
            let color = mix(base, MINT, drop);
            let index = ((y * ICON_SIZE + x) * 4) as usize;
            rgba[index..index + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
    IconImage {
        width: ICON_SIZE,
        height: ICON_SIZE,
        rgba,
    }
}

fn rounded_coverage(x: f32, y: f32, side: f32, radius: f32) -> f32 {
    let half = side / 2.0;
    let dx = ((x - half).abs() - (half - radius)).max(0.0);
    let dy = ((y - half).abs() - (half - radius)).max(0.0);
    let distance = (dx * dx + dy * dy).sqrt() - radius;
    (0.5 - distance).clamp(0.0, 1.0)
}

fn circle_coverage(x: f32, y: f32, cx: f32, cy: f32, radius: f32) -> f32 {
    let distance = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
    (radius + 0.5 - distance).clamp(0.0, 1.0)
}

fn mix(from: [u8; 3], to: [u8; 3], amount: f32) -> [u8; 3] {
    [0, 1, 2].map(|channel| {
        let start = f32::from(from[channel]);
        let end = f32::from(to[channel]);
        (start * (1.0 - amount) + end * amount).round() as u8
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_has_python_icon_size_and_transparent_corners() {
        let image = fallback_image();
        assert_eq!((image.width, image.height), (ICON_SIZE, ICON_SIZE));
        assert_eq!(image.pixel(0, 0)[3], 0);
        assert_eq!(image.pixel(ICON_SIZE - 1, ICON_SIZE - 1)[3], 0);
        assert_eq!(image.pixel(32, 12)[3], 255);
        let head = image.pixel(23, 38);
        assert_eq!(head[3], 255);
        assert!(
            head[1] > head[0],
            "la gota debe ser verde azulada: {head:?}"
        );
    }

    #[test]
    fn missing_or_unreadable_path_uses_fallback() {
        let missing = load_icon_image(Some(Path::new("no-existe-icono-ezagent.png")));
        assert_eq!((missing.width, missing.height), (ICON_SIZE, ICON_SIZE));
        let broken = std::env::temp_dir().join("ezagent-broken-icon.png");
        std::fs::write(&broken, b"no es un png").expect("escribir temporal");
        let image = load_icon_image(Some(&broken));
        let _ = std::fs::remove_file(&broken);
        assert_eq!((image.width, image.height), (ICON_SIZE, ICON_SIZE));
    }

    #[test]
    fn decodes_and_resizes_a_png_to_icon_size() {
        let path = std::env::temp_dir().join("ezagent-tray-test-icon.png");
        {
            let file = std::fs::File::create(&path).expect("crear png");
            let mut encoder = png::Encoder::new(file, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("cabecera png");
            writer
                .write_image_data(&[
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128,
                ])
                .expect("datos png");
        }
        let image = load_icon_image(Some(&path));
        let _ = std::fs::remove_file(&path);
        assert_eq!((image.width, image.height), (ICON_SIZE, ICON_SIZE));
        assert_eq!(image.pixel(0, 0)[3], 255);
        assert_eq!(image.pixel(ICON_SIZE - 1, ICON_SIZE - 1)[3], 128);
    }
}

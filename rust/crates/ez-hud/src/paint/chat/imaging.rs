//! Decoding of the images the chat shows, like `PIL.Image.convert("RGBA")`.
//!
//! `media.load_image` (ported in `ez-chat`) validates the file: local, inside
//! the allowed folders, a known image extension and a bounded size. Here the
//! bytes become the straight-RGBA pixels the panel paints. Pillow opens PNG,
//! JPEG, GIF, BMP and WebP and returns the **first** frame as RGBA, so an
//! animated GIF or WebP shows its first frame, like the Python.

use ez_gfx::Image;

/// Decodes an image of any accepted format to straight RGBA.
pub fn decode(bytes: &[u8]) -> Result<Image, String> {
    let decoded = image::load_from_memory(bytes).map_err(|error| error.to_string())?;
    let width = decoded.width();
    let height = decoded.height();
    let mut rgba = decoded.to_rgba8().into_raw();
    if bytes.starts_with(b"BM") {
        // Pillow reads every BMP as RGB (`BmpImagePlugin`): the fourth byte of
        // a 32-bit BMP is padding, not alpha.
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
    }
    Image::from_rgba(width, height, rgba).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::decode;

    const GIF: &[u8] = include_bytes!("../../../tests/data/chat_images/transparente.gif");

    #[test]
    fn a_corrupt_file_is_not_an_image() {
        assert!(decode(b"no soy una imagen").is_err());
        assert!(decode(&[]).is_err());
    }

    #[test]
    fn a_gif_keeps_its_transparency() {
        let image = decode(GIF).expect("GIF transparente");
        assert_eq!((image.width(), image.height()), (8, 6));
        assert_eq!(image.pixel(0, 0).to_array()[3], 0);
        assert_eq!(image.pixel(1, 0).to_array()[3], 255);
    }
}

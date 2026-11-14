//! FreeType font loading, metrics and glyph layout.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use freetype::face::{KerningMode, LoadFlag};
use freetype::{Face, Library};

use super::FontStyle;

/// Logical style value in FreeType's 26.6 pixel format.
pub(crate) fn pixel(value: i64) -> i64 {
    ((value + 32) & -64) >> 6
}

/// One laid-out glyph: face index and advance in 26.6 pixels.
pub(crate) struct GlyphInfo {
    pub(crate) index: u32,
    pub(crate) advance: i64,
}

/// Glyph bitmap copied out of the FreeType slot.
pub(crate) struct RasterGlyph {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) pixels: Vec<u8>,
}

/// Loaded face plus the metrics Pillow exposes through `getmetrics()`.
pub(crate) struct Font {
    face: Option<Face>,
    ascent: i32,
    descent: i32,
}

impl Font {
    /// `getmetrics()`: `(ascent, descent)` in whole pixels.
    pub(crate) fn metrics(&self) -> (i32, i32) {
        (self.ascent, self.descent)
    }

    /// `font.getlength(text)` in pixels.
    pub(crate) fn length(&self, text: &str) -> f64 {
        self.layout(text)
            .iter()
            .map(|glyph| glyph.advance)
            .sum::<i64>() as f64
            / 64.0
    }

    /// Pillow's `text_layout_fallback`: advances in 26.6 with integer kerning.
    pub(crate) fn layout(&self, text: &str) -> Vec<GlyphInfo> {
        let Some(face) = &self.face else {
            return Vec::new();
        };
        let mut glyphs = Vec::with_capacity(text.chars().count());
        let mut last_index = 0u32;
        for ch in text.chars() {
            let index = face.get_char_index(ch as usize).unwrap_or(0);
            let advance = match face.load_glyph(index, LoadFlag::DEFAULT) {
                Ok(()) => i64::from(face.glyph().metrics().horiAdvance),
                Err(_) => 0,
            };
            if face.has_kerning() && last_index != 0 && index != 0 {
                if let Ok(delta) = face.get_kerning(last_index, index, KerningMode::KerningDefault)
                {
                    if let Some(previous) = glyphs.last_mut() {
                        let previous: &mut GlyphInfo = previous;
                        previous.advance += pixel(i64::from(delta.x)) * 64;
                    }
                }
            }
            glyphs.push(GlyphInfo { index, advance });
            last_index = index;
        }
        glyphs
    }

    /// `font.getbbox(text)[1]` with the default `la` anchor.
    pub(crate) fn ink_top(&self, text: &str) -> Option<i32> {
        self.ink_span(text).map(|(top, _)| top)
    }

    /// `(font.getbbox(text)[1], font.getbbox(text)[3])` with anchor `la`.
    pub(crate) fn ink_span(&self, text: &str) -> Option<(i32, i32)> {
        let face = self.face.as_ref()?;
        let mut y_min = 0i64;
        let mut y_max = 0i64;
        for glyph in self.layout(text) {
            face.load_glyph(glyph.index, LoadFlag::DEFAULT).ok()?;
            let outline = face.glyph().get_glyph().ok()?;
            let bbox = outline.get_cbox(freetype::ffi::FT_GLYPH_BBOX_PIXELS);
            y_max = y_max.max(i64::from(bbox.yMax));
            y_min = y_min.min(i64::from(bbox.yMin));
        }
        let top = i64::from(self.ascent) - y_max;
        Some((top as i32, (top + y_max - y_min) as i32))
    }

    /// Loads a glyph and renders it into an 8-bit coverage bitmap.
    pub(crate) fn raster(&self, index: u32) -> Option<RasterGlyph> {
        let face = self.face.as_ref()?;
        face.load_glyph(index, LoadFlag::DEFAULT | LoadFlag::RENDER)
            .ok()?;
        let slot = face.glyph();
        let bitmap = slot.bitmap();
        if bitmap.pixel_mode().ok()? != freetype::bitmap::PixelMode::Gray {
            return None;
        }
        let width = u32::try_from(bitmap.width()).ok()?;
        let height = u32::try_from(bitmap.rows()).ok()?;
        let pitch = bitmap.pitch();
        let buffer = bitmap.buffer();
        let mut pixels = Vec::with_capacity((width * height) as usize);
        let stride = pitch.unsigned_abs() as usize;
        for row in 0..height as usize {
            let start = row * stride;
            pixels.extend_from_slice(&buffer[start..start + width as usize]);
        }
        Some(RasterGlyph {
            width,
            height,
            left: slot.bitmap_left(),
            top: slot.bitmap_top(),
            pixels,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FontKey {
    family: &'static str,
    weight: i32,
    size: u64,
}

thread_local! {
    static LIBRARY: RefCell<Option<Library>> = const { RefCell::new(None) };
    static FONTS: RefCell<HashMap<FontKey, Rc<Font>>> = RefCell::new(HashMap::new());
    static PATHS: RefCell<HashMap<(&'static str, i32), Option<PathBuf>>> =
        RefCell::new(HashMap::new());
}

fn with_library<T>(callback: impl FnOnce(&Library) -> T) -> Option<T> {
    LIBRARY.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Library::init().ok();
        }
        cell.as_ref().map(callback)
    })
}

fn font_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../src/ezagent/voice/assets/fonts")
}

fn project_font(family: &str, weight: i32) -> Option<PathBuf> {
    let name = match (family, weight) {
        ("Geist", 400) => "Geist-Regular.ttf",
        ("Geist", 500) => "Geist-Medium.ttf",
        ("Geist", 600) => "Geist-SemiBold.ttf",
        ("GeistMono", 400) => "GeistMono-Regular.ttf",
        ("GeistMono", 500 | 600) => "GeistMono-Medium.ttf",
        _ => return None,
    };
    Some(font_dir().join(name))
}

fn fallback_fonts(family: &str, weight: i32) -> Vec<PathBuf> {
    match (family, weight) {
        ("Geist", 400 | 500) => vec![PathBuf::from("C:/Windows/Fonts/segoeui.ttf")],
        ("Geist", 600) => vec![
            PathBuf::from("C:/Windows/Fonts/seguisb.ttf"),
            PathBuf::from("C:/Windows/Fonts/segoeui.ttf"),
        ],
        ("GeistMono", _) => vec![PathBuf::from("C:/Windows/Fonts/consola.ttf")],
        _ => Vec::new(),
    }
}

/// `_font_path`: first candidate that exists, remembered even when absent.
fn font_path(style: FontStyle) -> Option<PathBuf> {
    let key = (style.family, style.weight);
    if let Some(path) = PATHS.with(|paths| paths.borrow().get(&key).cloned()) {
        return path;
    }
    let mut candidates = Vec::new();
    if let Some(project) = project_font(style.family, style.weight) {
        candidates.push(project);
    }
    candidates.extend(fallback_fonts(style.family, style.weight));
    let found = candidates.into_iter().find(|candidate| candidate.exists());
    PATHS.with(|paths| {
        paths.borrow_mut().insert(key, found.clone());
    });
    found
}

/// `_load_font`: a cached face, or a metrics-only fallback when none exists.
pub(crate) fn load_font(style: FontStyle, scale: f64) -> Rc<Font> {
    let size_px = (style.size * scale).max(1.0);
    let key = FontKey {
        family: style.family,
        weight: style.weight,
        size: super::float_key(size_px),
    };
    if let Some(font) = FONTS.with(|fonts| fonts.borrow().get(&key).cloned()) {
        return font;
    }
    let font = match font_path(style) {
        Some(path) => with_library(|library| load_face(library, &path, size_px))
            .flatten()
            .unwrap_or_else(|| missing_font(size_px)),
        None => missing_font(size_px),
    };
    let font = Rc::new(font);
    FONTS.with(|fonts| {
        fonts.borrow_mut().insert(key, font.clone());
    });
    font
}

fn load_face(library: &Library, path: &Path, size_px: f64) -> Option<Font> {
    let face = library.new_face(path, 0).ok()?;
    let width = (size_px * 64.0) as isize;
    face.set_char_size(width, width, 0, 0).ok()?;
    let metrics = face.size_metrics()?;
    let ascent = pixel(i64::from(metrics.ascender)) as i32;
    let descent = -pixel(i64::from(metrics.descender)) as i32;
    Some(Font {
        face: Some(face),
        ascent,
        descent,
    })
}

fn missing_font(size_px: f64) -> Font {
    // DECISION: without Geist and the system fallbacks Pillow draws its
    // internal bitmap font. The port keeps the same metrics shape (one line
    // per em) but no glyphs, so a broken installation still lays out text.
    let line_h = (size_px.round_ties_even() as i32).max(1);
    Font {
        face: None,
        ascent: line_h,
        descent: 0,
    }
}

/// `cap_height`/`baseline_top` bbox without going through the whole bitmap.
pub(crate) fn ink_span(style: FontStyle, scale: f64, text: &str) -> Option<(i32, i32)> {
    load_font(style, scale).ink_span(text)
}

/// Empties only the font and path caches; `clear_cache` does both caches.
pub(crate) fn clear_font_cache() {
    FONTS.with(|fonts| fonts.borrow_mut().clear());
    PATHS.with(|paths| paths.borrow_mut().clear());
}

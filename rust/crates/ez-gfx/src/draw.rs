//! Port of `Pillow ImageDraw` (F2-08).
//!
//! Reproduces the subset of `PIL/ImageDraw.py` and `src/libImaging/Draw.c` that
//! the HUD uses (`ellipse`, `line`, `rounded_rectangle`, `polygon`,
//! `rectangle`) byte for byte on straight-alpha `RGBA` and `L` images: the
//! Bresenham line, the scan-line polygon filler with Pillow's corner joining,
//! the newer quarter-ellipse rasterizer and the clipped arc/pie machinery that
//! `rounded_rectangle` draws its corners with.
//!
//! `ImageDraw.Draw(image, "RGBA")` over an `RGBA` image blends with `op = 0`,
//! so a fill stores the whole `RGBA` word. `Draw` does the same: [`Image`] is
//! written channel for channel and [`Mask`] with its single byte.
//!
//! // DECISION: only `op = 0` is ported. Pillow's `RGBA`-on-`RGB` blending and
//! // its clipped-alpha polygon path are not used by the HUD.

mod clip;
mod ellipse;
mod geom;

#[cfg(test)]
mod oracle;
#[cfg(test)]
mod tests;

use crate::raster::{Image, Mask, Rgba};

/// A color an [`ImageDraw`] surface accepts.
pub trait Ink: Copy {
    /// The 32-bit word the C drawing routines carry around.
    fn packed(self) -> u32;
    /// `ImageDraw.ink` when a call passes neither `fill` nor `outline`
    /// (`draw_ink(-1)` in Pillow).
    fn default_ink() -> Self;
}

impl Ink for Rgba {
    fn packed(self) -> u32 {
        u32::from_le_bytes(self.to_array())
    }

    fn default_ink() -> Self {
        Rgba::WHITE
    }
}

impl Ink for u8 {
    fn packed(self) -> u32 {
        u32::from(self)
    }

    fn default_ink() -> Self {
        0
    }
}

/// One drawing target: an `RGBA` image or an `L` mask.
pub trait Surface {
    /// Color type of the surface's mode.
    type Ink: Ink;

    /// Width in pixels.
    fn width(&self) -> u32;
    /// Height in pixels.
    fn height(&self) -> u32;
    /// `point8`/`point32` of `Draw.c`.
    fn point(&mut self, x: i32, y: i32, ink: u32);
    /// `hline8`/`hline32` of `Draw.c`, with an optional clip mask.
    fn hline(&mut self, x0: i32, y: i32, x1: i32, ink: u32, mask: Option<&Mask>);
}

impl Surface for Mask {
    type Ink = u8;

    fn width(&self) -> u32 {
        Self::width(self)
    }

    fn height(&self) -> u32 {
        Self::height(self)
    }

    fn point(&mut self, x: i32, y: i32, ink: u32) {
        if x >= 0 && y >= 0 && x < self.width() as i32 && y < self.height() as i32 {
            self.set(x as u32, y as u32, ink as u8);
        }
    }

    fn hline(&mut self, x0: i32, y: i32, x1: i32, ink: u32, mask: Option<&Mask>) {
        if y < 0 || y >= self.height() as i32 {
            return;
        }
        let start = x0.max(0);
        let end = x1.min(self.width() as i32 - 1);
        if start > end {
            return;
        }
        let row = self.row_mut(y as u32);
        for x in start..=end {
            if mask.is_none_or(|clip| clip.get(x as u32, y as u32) != 0) {
                row[x as usize] = ink as u8;
            }
        }
    }
}

impl Surface for Image {
    type Ink = Rgba;

    fn width(&self) -> u32 {
        Self::width(self)
    }

    fn height(&self) -> u32 {
        Self::height(self)
    }

    fn point(&mut self, x: i32, y: i32, ink: u32) {
        if x >= 0 && y >= 0 && x < self.width() as i32 && y < self.height() as i32 {
            self.set_pixel(x as u32, y as u32, Rgba::from_array(ink.to_le_bytes()));
        }
    }

    fn hline(&mut self, x0: i32, y: i32, x1: i32, ink: u32, mask: Option<&Mask>) {
        if y < 0 || y >= self.height() as i32 {
            return;
        }
        let start = x0.max(0);
        let end = x1.min(self.width() as i32 - 1);
        if start > end {
            return;
        }
        let bytes = ink.to_le_bytes();
        let row = self.row_mut(y as u32);
        for x in start..=end {
            if mask.is_none_or(|clip| clip.get(x as u32, y as u32) != 0) {
                let index = x as usize * 4;
                row[index..index + 4].copy_from_slice(&bytes);
            }
        }
    }
}

/// Error raised by the coordinate checks of `ImageDraw`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawError {
    /// `x1 must be greater than or equal to x0`.
    IncorrectlyOrderedX,
    /// `y1 must be greater than or equal to y0`.
    IncorrectlyOrderedY,
    /// `coordinate list must contain at least 2 coordinates`.
    TooFewCoordinates,
}

impl std::fmt::Display for DrawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::IncorrectlyOrderedX => "x1 must be greater than or equal to x0",
            Self::IncorrectlyOrderedY => "y1 must be greater than or equal to y0",
            Self::TooFewCoordinates => "coordinate list must contain at least 2 coordinates",
        })
    }
}

impl std::error::Error for DrawError {}

/// Geometry `ImageDraw.rounded_rectangle` derives before drawing.
#[derive(Clone, Copy)]
struct Rounded {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    d: f64,
    r: i32,
    full_x: bool,
    full_y: bool,
}

impl Rounded {
    fn corners(self) -> Vec<([f64; 4], f32, f32)> {
        if self.full_x {
            vec![
                (
                    [self.x0, self.y0, self.x0 + self.d, self.y0 + self.d],
                    180.0,
                    360.0,
                ),
                (
                    [self.x0, self.y1 - self.d, self.x0 + self.d, self.y1],
                    0.0,
                    180.0,
                ),
            ]
        } else if self.full_y {
            vec![
                (
                    [self.x0, self.y0, self.x0 + self.d, self.y0 + self.d],
                    90.0,
                    270.0,
                ),
                (
                    [self.x1 - self.d, self.y0, self.x1, self.y0 + self.d],
                    270.0,
                    90.0,
                ),
            ]
        } else {
            vec![
                (
                    [self.x0, self.y0, self.x0 + self.d, self.y0 + self.d],
                    180.0,
                    270.0,
                ),
                (
                    [self.x1 - self.d, self.y0, self.x1, self.y0 + self.d],
                    270.0,
                    360.0,
                ),
                (
                    [self.x1 - self.d, self.y1 - self.d, self.x1, self.y1],
                    0.0,
                    90.0,
                ),
                (
                    [self.x0, self.y1 - self.d, self.x0 + self.d, self.y1],
                    90.0,
                    180.0,
                ),
            ]
        }
    }
}

/// `ImageDraw.Draw` over one image of mode `RGBA` or `L`.
pub struct Draw<'a, S: Surface> {
    surface: &'a mut S,
}

impl<'a, S: Surface> Draw<'a, S> {
    /// Binds a drawing context to one mutable image.
    pub fn new(surface: &'a mut S) -> Self {
        Self { surface }
    }

    /// `ImageDraw.ellipse`.
    pub fn ellipse(
        &mut self,
        xy: [f64; 4],
        fill: Option<S::Ink>,
        outline: Option<S::Ink>,
        width: u32,
    ) -> Result<(), DrawError> {
        check_order(xy)?;
        let (ink, fill_ink) = self.inks(fill, outline);
        if let Some(fill_ink) = fill_ink {
            ellipse::ellipse(self.surface, xy, fill_ink, true, 0);
        }
        if let Some(ink) = ink {
            if Some(ink) != fill_ink && width != 0 {
                ellipse::ellipse(self.surface, xy, ink, false, width as i32);
            }
        }
        Ok(())
    }

    /// `ImageDraw.line` for a flat list of points.
    ///
    /// // DECISION: the HUD never asks for `joint = "curve"`, so joins are not ported.
    pub fn line(&mut self, xy: &[(f64, f64)], fill: Option<S::Ink>, width: u32) {
        let ink = fill.unwrap_or_else(S::Ink::default_ink).packed();
        if width == 0 || xy.len() < 2 {
            return;
        }
        let points: Vec<(i32, i32)> = xy.iter().map(|&(x, y)| (x as i32, y as i32)).collect();
        if width == 1 {
            for pair in points.windows(2) {
                geom::line(
                    self.surface,
                    pair[0].0,
                    pair[0].1,
                    pair[1].0,
                    pair[1].1,
                    ink,
                );
            }
            let last = points[points.len() - 1];
            self.surface.point(last.0, last.1, ink);
        } else {
            for pair in points.windows(2) {
                geom::wide_line(
                    self.surface,
                    (pair[0].0, pair[0].1),
                    (pair[1].0, pair[1].1),
                    ink,
                    width as i32,
                    None,
                );
            }
        }
    }

    /// `ImageDraw.polygon`.
    pub fn polygon(
        &mut self,
        xy: &[(f64, f64)],
        fill: Option<S::Ink>,
        outline: Option<S::Ink>,
        width: u32,
    ) -> Result<(), DrawError> {
        if xy.len() < 2 {
            return Err(DrawError::TooFewCoordinates);
        }
        let points: Vec<(i32, i32)> = xy.iter().map(|&(x, y)| (x as i32, y as i32)).collect();
        let (ink, fill_ink) = self.inks(fill, outline);
        if let Some(fill_ink) = fill_ink {
            geom::polygon_fill(self.surface, &points, fill_ink, None);
        }
        if let Some(ink) = ink {
            if Some(ink) == fill_ink || width == 0 {
                return Ok(());
            }
            if width == 1 {
                geom::polygon_outline(self.surface, &points, ink, 1, None);
            } else {
                // Pillow clips the wide outline against the filled polygon.
                let mut mask = Mask::new(self.surface.width(), self.surface.height(), 0);
                geom::polygon_fill(&mut mask, &points, u32::from(255u8), None);
                geom::polygon_outline(self.surface, &points, ink, width * 2 - 1, Some(&mask));
            }
        }
        Ok(())
    }

    /// `ImageDraw.rectangle`.
    pub fn rectangle(
        &mut self,
        xy: [f64; 4],
        fill: Option<S::Ink>,
        outline: Option<S::Ink>,
        width: u32,
    ) -> Result<(), DrawError> {
        check_order(xy)?;
        let (ink, fill_ink) = self.inks(fill, outline);
        if let Some(fill_ink) = fill_ink {
            fill_rect(
                self.surface,
                [xy[0] as i32, xy[1] as i32, xy[2] as i32, xy[3] as i32],
                fill_ink,
            )?;
        }
        if let Some(ink) = ink {
            if Some(ink) != fill_ink && width != 0 {
                geom::rectangle(
                    self.surface,
                    [xy[0] as i32, xy[1] as i32, xy[2] as i32, xy[3] as i32],
                    ink,
                    false,
                    width as i32,
                );
            }
        }
        Ok(())
    }

    /// `ImageDraw.rounded_rectangle`.
    ///
    /// // DECISION: `corners=(...)` is not used by the HUD; all four corners are on.
    pub fn rounded_rectangle(
        &mut self,
        xy: [f64; 4],
        radius: f64,
        fill: Option<S::Ink>,
        outline: Option<S::Ink>,
        width: u32,
    ) -> Result<(), DrawError> {
        let [x0, y0, x1, y1] = xy;
        if x1 < x0 {
            return Err(DrawError::IncorrectlyOrderedX);
        }
        if y1 < y0 {
            return Err(DrawError::IncorrectlyOrderedY);
        }
        let mut d = (x1 - x0).min(y1 - y0).min(radius * 2.0);
        let x0 = x0.round_ties_even();
        let y0 = y0.round_ties_even();
        let x1 = x1.round_ties_even();
        let y1 = y1.round_ties_even();
        let full_x = d >= x1 - x0 - 1.0;
        if full_x {
            d = x1 - x0;
        }
        let full_y = d >= y1 - y0 - 1.0;
        if full_y {
            d = y1 - y0;
        }
        if full_x && full_y {
            return self.ellipse(xy, fill, outline, width);
        }
        if d == 0.0 {
            return self.rectangle(xy, fill, outline, width);
        }
        if d < 0.0 {
            // A negative radius makes every corner box reversed; Pillow raises.
            return Err(DrawError::IncorrectlyOrderedX);
        }
        let shape = Rounded {
            x0,
            y0,
            x1,
            y1,
            d,
            r: (d / 2.0).floor() as i32,
            full_x,
            full_y,
        };
        let (ink, fill_ink) = self.inks(fill, outline);
        if let Some(fill_ink) = fill_ink {
            self.fill_rounded(shape, fill_ink)?;
        }
        if let Some(ink) = ink {
            if Some(ink) != fill_ink && width != 0 {
                self.outline_rounded(shape, ink, width)?;
            }
        }
        Ok(())
    }

    fn fill_rounded(&mut self, shape: Rounded, ink: u32) -> Result<(), DrawError> {
        for (box_, start, end) in shape.corners() {
            self.pieslice(box_, start, end, ink, 0);
        }
        let Rounded {
            x0, y0, x1, y1, r, ..
        } = shape;
        let r = f64::from(r);
        if shape.full_x {
            fill_rect(
                self.surface,
                [
                    x0 as i32,
                    (y0 + r + 1.0) as i32,
                    x1 as i32,
                    (y1 - r - 1.0) as i32,
                ],
                ink,
            )?;
        } else if x1 - r - 1.0 >= x0 + r + 1.0 {
            fill_rect(
                self.surface,
                [
                    (x0 + r + 1.0) as i32,
                    y0 as i32,
                    (x1 - r - 1.0) as i32,
                    y1 as i32,
                ],
                ink,
            )?;
        }
        if !shape.full_x && !shape.full_y {
            fill_rect(
                self.surface,
                [
                    x0 as i32,
                    (y0 + r + 1.0) as i32,
                    (x0 + r) as i32,
                    (y1 - r - 1.0) as i32,
                ],
                ink,
            )?;
            fill_rect(
                self.surface,
                [
                    (x1 - r) as i32,
                    (y0 + r + 1.0) as i32,
                    x1 as i32,
                    (y1 - r - 1.0) as i32,
                ],
                ink,
            )?;
        }
        Ok(())
    }

    fn outline_rounded(&mut self, shape: Rounded, ink: u32, width: u32) -> Result<(), DrawError> {
        for (box_, start, end) in shape.corners() {
            self.arc(box_, start, end, ink, width);
        }
        let Rounded {
            x0, y0, x1, y1, r, ..
        } = shape;
        let width = f64::from(width);
        if !shape.full_x {
            fill_rect(
                self.surface,
                [
                    (x0 + f64::from(r) + 1.0) as i32,
                    y0 as i32,
                    (x1 - f64::from(r) - 1.0) as i32,
                    (y0 + width - 1.0) as i32,
                ],
                ink,
            )?;
            fill_rect(
                self.surface,
                [
                    (x0 + f64::from(r) + 1.0) as i32,
                    (y1 - width + 1.0) as i32,
                    (x1 - f64::from(r) - 1.0) as i32,
                    y1 as i32,
                ],
                ink,
            )?;
        }
        if !shape.full_y {
            fill_rect(
                self.surface,
                [
                    x0 as i32,
                    (y0 + f64::from(r) + 1.0) as i32,
                    (x0 + width - 1.0) as i32,
                    (y1 - f64::from(r) - 1.0) as i32,
                ],
                ink,
            )?;
            fill_rect(
                self.surface,
                [
                    (x1 - width + 1.0) as i32,
                    (y0 + f64::from(r) + 1.0) as i32,
                    x1 as i32,
                    (y1 - f64::from(r) - 1.0) as i32,
                ],
                ink,
            )?;
        }
        Ok(())
    }

    /// `_getink(outline, fill)`: both omitted means the default ink outline.
    fn inks(&self, fill: Option<S::Ink>, outline: Option<S::Ink>) -> (Option<u32>, Option<u32>) {
        match (outline, fill) {
            (None, None) => (Some(S::Ink::default_ink().packed()), None),
            (ink, fill) => (ink.map(Ink::packed), fill.map(Ink::packed)),
        }
    }

    fn arc(&mut self, xy: [f64; 4], start: f32, end: f32, ink: u32, width: u32) {
        clip::arc(self.surface, xy, start, end, ink, width as i32);
    }

    fn pieslice(&mut self, xy: [f64; 4], start: f32, end: f32, ink: u32, width: i32) {
        clip::pieslice(self.surface, xy, start, end, ink, true, width);
    }
}

/// `draw_rectangle(..., fill_ink, 1)` inside `rounded_rectangle`, with the
/// ordering check of Pillow's binding.
fn fill_rect<S: Surface>(surface: &mut S, box_: [i32; 4], ink: u32) -> Result<(), DrawError> {
    if box_[2] < box_[0] {
        return Err(DrawError::IncorrectlyOrderedX);
    }
    if box_[3] < box_[1] {
        return Err(DrawError::IncorrectlyOrderedY);
    }
    geom::rectangle(surface, box_, ink, true, 0);
    Ok(())
}

fn check_order(xy: [f64; 4]) -> Result<(), DrawError> {
    if xy[2] < xy[0] {
        return Err(DrawError::IncorrectlyOrderedX);
    }
    if xy[3] < xy[1] {
        return Err(DrawError::IncorrectlyOrderedY);
    }
    Ok(())
}

//! Per-channel views over an [`Image`](super::Image).
//!
//! `numpy` paints the HUD through slices such as `arr[..., 0]` or
//! `arr[..., 3:4]`. These views expose the same planes without copying: one
//! reference selects the channel, rows are strided slices, and the mutable
//! variants write through to the interleaved buffer.

use std::ops::{Index, IndexMut};

/// One channel of an [`Image`](super::Image).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// Red.
    Red,
    /// Green.
    Green,
    /// Blue.
    Blue,
    /// Alpha.
    Alpha,
}

impl Channel {
    pub(crate) const ALL: [Self; 4] = [Self::Red, Self::Green, Self::Blue, Self::Alpha];

    pub(crate) const fn offset(self) -> usize {
        match self {
            Self::Red => 0,
            Self::Green => 1,
            Self::Blue => 2,
            Self::Alpha => 3,
        }
    }
}

/// Read-only view of one image channel.
#[derive(Clone, Copy, Debug)]
pub struct ChannelView<'a> {
    pub(super) data: &'a [u8],
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) channel: Channel,
}

impl<'a> ChannelView<'a> {
    /// Width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Value at `(x, y)`.
    pub fn get(&self, x: u32, y: u32) -> u8 {
        self[(x, y)]
    }

    /// One row of channel values.
    pub fn row(&self, y: u32) -> ChannelRow<'a> {
        let stride = self.width as usize * 4;
        ChannelRow {
            row: &self.data[y as usize * stride..(y as usize + 1) * stride],
            channel: self.channel,
        }
    }
}

impl Index<(u32, u32)> for ChannelView<'_> {
    type Output = u8;

    fn index(&self, (x, y): (u32, u32)) -> &Self::Output {
        let index = (y as usize * self.width as usize + x as usize) * 4 + self.channel.offset();
        &self.data[index]
    }
}

/// One row of a read-only channel view.
#[derive(Clone, Copy, Debug)]
pub struct ChannelRow<'a> {
    row: &'a [u8],
    channel: Channel,
}

impl ChannelRow<'_> {
    /// Number of values in the row.
    pub fn len(&self) -> usize {
        self.row.len() / 4
    }

    /// Whether the row is empty.
    pub fn is_empty(&self) -> bool {
        self.row.is_empty()
    }

    /// Iterates the row's channel values.
    pub fn iter(&self) -> impl Iterator<Item = u8> + '_ {
        self.row
            .iter()
            .skip(self.channel.offset())
            .step_by(4)
            .copied()
    }
}

impl Index<usize> for ChannelRow<'_> {
    type Output = u8;

    fn index(&self, x: usize) -> &Self::Output {
        &self.row[x * 4 + self.channel.offset()]
    }
}

/// Mutable view of one image channel.
#[derive(Debug)]
pub struct ChannelViewMut<'a> {
    pub(super) data: &'a mut [u8],
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) channel: Channel,
}

impl ChannelViewMut<'_> {
    /// Width in pixels.
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Value at `(x, y)`.
    pub fn get(&self, x: u32, y: u32) -> u8 {
        self[(x, y)]
    }

    /// Writes the value at `(x, y)`.
    pub fn set(&mut self, x: u32, y: u32, value: u8) {
        self[(x, y)] = value;
    }

    /// One mutable row of channel values.
    pub fn row_mut(&mut self, y: u32) -> ChannelRowMut<'_> {
        let stride = self.width as usize * 4;
        ChannelRowMut {
            row: &mut self.data[y as usize * stride..(y as usize + 1) * stride],
            channel: self.channel,
        }
    }

    /// Iterates every channel value in row-major order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut u8> + '_ {
        self.data.iter_mut().skip(self.channel.offset()).step_by(4)
    }
}

impl Index<(u32, u32)> for ChannelViewMut<'_> {
    type Output = u8;

    fn index(&self, (x, y): (u32, u32)) -> &Self::Output {
        let index = (y as usize * self.width as usize + x as usize) * 4 + self.channel.offset();
        &self.data[index]
    }
}

impl IndexMut<(u32, u32)> for ChannelViewMut<'_> {
    fn index_mut(&mut self, (x, y): (u32, u32)) -> &mut Self::Output {
        let index = (y as usize * self.width as usize + x as usize) * 4 + self.channel.offset();
        &mut self.data[index]
    }
}

/// One row of a mutable channel view.
#[derive(Debug)]
pub struct ChannelRowMut<'a> {
    row: &'a mut [u8],
    channel: Channel,
}

impl ChannelRowMut<'_> {
    /// Number of values in the row.
    pub fn len(&self) -> usize {
        self.row.len() / 4
    }

    /// Whether the row is empty.
    pub fn is_empty(&self) -> bool {
        self.row.is_empty()
    }

    /// Iterates the row's channel values mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut u8> + '_ {
        self.row.iter_mut().skip(self.channel.offset()).step_by(4)
    }
}

impl Index<usize> for ChannelRowMut<'_> {
    type Output = u8;

    fn index(&self, x: usize) -> &Self::Output {
        &self.row[x * 4 + self.channel.offset()]
    }
}

impl IndexMut<usize> for ChannelRowMut<'_> {
    fn index_mut(&mut self, x: usize) -> &mut Self::Output {
        &mut self.row[x * 4 + self.channel.offset()]
    }
}

#[cfg(test)]
mod tests {
    use super::{Channel, ChannelViewMut};
    use crate::raster::Image;
    use crate::test_support::unhex;

    const IMG3: &str = "5b09e63c1ec9e77671e0240b13a98c9eb6b273c82270289cbddc546b92f367563b39eb54f48e32562835810a48836bfc";

    fn image() -> Image {
        Image::from_rgba(4, 3, unhex(IMG3)).expect("the oracle emitted a 4x3 image")
    }

    #[test]
    fn channels_read_the_interleaved_buffer() {
        let image = image();
        for (index, channel) in Channel::ALL.iter().enumerate() {
            let view = image.channel(*channel);
            let expected: Vec<u8> = image
                .row(1)
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| pixel[index])
                .collect();
            assert_eq!(view.row(1).iter().collect::<Vec<_>>(), expected);
            for x in 0..4 {
                assert_eq!(view.get(x, 2), image.pixel(x, 2).to_array()[index]);
            }
            assert_eq!(view[(3, 1)], image.pixel(3, 1).to_array()[index]);
        }
    }

    #[test]
    fn mutable_views_write_only_their_plane() {
        let image = image();
        let mut copy = image.clone();
        {
            let mut green: ChannelViewMut<'_> = copy.channel_mut(Channel::Green);
            for value in green.iter_mut() {
                *value = 5;
            }
            assert!(green.iter_mut().all(|value| *value == 5));
            assert_eq!(green.get(0, 0), 5);
            assert_eq!(green.row_mut(2)[1], 5);
            green.row_mut(1)[0] = 4;
        }
        for (index, pixel) in copy.pixels().enumerate() {
            assert_eq!(pixel.g, if index == 4 { 4 } else { 5 });
        }
        assert_eq!(copy.pixel(0, 1).g, 4);
        for (x, y) in [(0u32, 0u32), (1, 0), (2, 2), (3, 1)] {
            let before = image.pixel(x, y);
            let after = copy.pixel(x, y);
            assert_eq!(after.r, before.r);
            assert_eq!(after.b, before.b);
            assert_eq!(after.a, before.a);
        }
    }
}

//! FIFO caches of `icons.py` (`_MASKS` and `_CACHE`), one per thread.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

use ndarray::Array2;

use super::Color;

/// `(gap, slash)` masks, the value stored by `slash_masks`.
pub(crate) type SlashPair = (Array2<f32>, Array2<f32>);

/// Key of `icon_mask`: `(name, side, round(stroke, 3))`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GlyphKey {
    pub name: String,
    pub side: u32,
    pub stroke: i64,
}

/// Key of `slash_masks`: `("/slash", side, round(stroke, 3), round(progress, 3))`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SlashKey {
    pub side: u32,
    pub stroke: i64,
    pub progress: i64,
}

/// Key of `_sprite`: `(name, tuple(color), float(size), float(scale))`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SpriteKey {
    pub name: String,
    pub color: Color,
    pub size: u64,
    pub scale: u64,
}

/// Insertion-ordered map with the eviction rule of `_remember`.
pub(crate) struct Cache<K, V> {
    limit: usize,
    entries: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K: Clone + Eq + Hash, V> Cache<K, V> {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            limit,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub(crate) fn get(&self, key: &K) -> Option<&V> {
        self.entries.get(key)
    }

    pub(crate) fn insert(&mut self, key: K, value: V) {
        if self.entries.len() >= self.limit {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        if self.entries.insert(key.clone(), value).is_none() {
            self.order.push_back(key);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }
}

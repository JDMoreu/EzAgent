//! Global SDF mask cache shared by the menu helpers (`menu._MASK_CACHE`).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use super::MaskData;

/// `menu._MASK_LIMIT`: masks kept by the global geometry cache.
const MASK_LIMIT: usize = 384;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct MaskKey {
    pub(super) height: u32,
    pub(super) width: u32,
    pub(super) x0: u64,
    pub(super) y0: u64,
    pub(super) x1: u64,
    pub(super) y1: u64,
    pub(super) radius_top: u64,
    pub(super) radius_bottom: u64,
    pub(super) scale: u64,
    pub(super) stroke: bool,
}

#[derive(Default)]
struct MaskCache {
    order: VecDeque<MaskKey>,
    entries: HashMap<MaskKey, Arc<MaskData>>,
}

static MASK_CACHE: OnceLock<Mutex<MaskCache>> = OnceLock::new();

fn cache() -> MutexGuard<'static, MaskCache> {
    MASK_CACHE
        .get_or_init(|| Mutex::new(MaskCache::default()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn get(key: &MaskKey) -> Option<Arc<MaskData>> {
    cache().entries.get(key).cloned()
}

pub(super) fn put(key: MaskKey, value: Arc<MaskData>) {
    let mut guard = cache();
    if guard.entries.len() >= MASK_LIMIT {
        if let Some(oldest) = guard.order.pop_front() {
            guard.entries.remove(&oldest);
        }
    }
    guard.order.push_back(key);
    guard.entries.insert(key, value);
}

/// `round(value, digits)` of Python as the `f64` bit pattern used by the cache
/// key. Rust's fixed-precision formatting rounds the same decimal correctly.
pub(super) fn round_key(value: f64, digits: usize) -> u64 {
    let text = if digits == 3 {
        format!("{value:.3}")
    } else {
        format!("{value:.4}")
    };
    let rounded = text.parse::<f64>().unwrap_or(value);
    // Python's dict treats -0.0 and 0.0 as the same key.
    if rounded == 0.0 {
        0.0f64.to_bits()
    } else {
        rounded.to_bits()
    }
}

#[cfg(test)]
pub(super) fn len() -> usize {
    cache().entries.len()
}

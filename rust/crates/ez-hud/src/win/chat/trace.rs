//! Journal of the frames the chat panel hands to its window (DX-1f).
//!
//! The opening of the chat looked like a jump and the closing did not. From
//! outside, a panel that appears in one frame and a panel that morphs look the
//! same, so the diagnostic needs to see **which frames were produced** and in
//! which state. The journal is off by default (one atomic load per frame); the
//! smoke example and the tests turn it on.

use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;

/// One frame the panel composed for its window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameRecord {
    /// Seconds since the animation started, at the instant of the frame.
    pub elapsed: f64,
    /// Seconds since the animation started when the frame was ready to be
    /// presented (the composition already finished).
    pub ready: f64,
    /// `"opening"`, `"open"`, `"closing"` or `"closed"`.
    pub phase: &'static str,
    /// Eased morph progress: 0 is the bar shape, 1 the whole panel.
    pub p: f64,
    /// Composed width in physical pixels.
    pub width: usize,
    /// Composed height in physical pixels.
    pub height: usize,
    /// Largest alpha of the frame (0..=255).
    pub alpha: u8,
    /// Whether the frame carried the content layer (header, messages, input).
    pub content: bool,
}

/// Frames kept: a diagnostic never needs more than a few dozen.
const CAP: usize = 4096;

static ENABLED: AtomicBool = AtomicBool::new(false);
static JOURNAL: Mutex<Vec<FrameRecord>> = Mutex::new(Vec::new());

/// Turns the journal on or off; turning it off empties it.
pub fn enable(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        clear();
    }
}

/// Whether the journal is on.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Forgets every recorded frame.
pub fn clear() {
    JOURNAL.lock().clear();
}

/// The recorded frames, oldest first.
pub fn records() -> Vec<FrameRecord> {
    JOURNAL.lock().clone()
}

/// Records one frame if the journal is on.
pub(crate) fn push(record: FrameRecord) {
    if !enabled() {
        return;
    }
    let mut journal = JOURNAL.lock();
    if journal.len() < CAP {
        journal.push(record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The journal is global: its tests run one at a time.
    static TESTS: Mutex<()> = Mutex::new(());

    fn frame(elapsed: f64) -> FrameRecord {
        FrameRecord {
            elapsed,
            ready: elapsed,
            phase: "opening",
            p: 0.5,
            width: 440,
            height: 600,
            alpha: 128,
            content: false,
        }
    }

    fn has(frames: &[FrameRecord], elapsed: f64) -> bool {
        frames.iter().any(|frame| frame.elapsed == elapsed)
    }

    #[test]
    fn the_journal_is_off_and_records_nothing_until_it_is_enabled() {
        let _guard = TESTS.lock();
        enable(false);
        clear();
        push(frame(1234.5));
        assert!(!has(&records(), 1234.5));
        assert!(!enabled());
    }

    #[test]
    fn the_journal_keeps_the_frames_in_order_until_it_is_cleared() {
        let _guard = TESTS.lock();
        enable(true);
        clear();
        push(frame(2345.5));
        push(frame(3456.5));
        let kept = records();
        let first = kept.iter().position(|frame| frame.elapsed == 2345.5);
        let second = kept.iter().position(|frame| frame.elapsed == 3456.5);
        assert!(first.is_some() && second.is_some(), "{kept:?}");
        assert!(first < second, "{kept:?}");
        clear();
        assert!(!has(&records(), 2345.5));
        enable(false);
    }

    #[test]
    fn the_journal_stops_at_its_cap() {
        let _guard = TESTS.lock();
        enable(true);
        clear();
        for index in 0..(CAP + 10) {
            push(frame(index as f64));
        }
        assert_eq!(records().len(), CAP);
        enable(false);
    }
}

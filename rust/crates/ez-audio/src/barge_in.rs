//! Port of `src/ezagent/voice/audio.py` (`BargeInDetector`) (F4-01).
//!
//! Interrupción: una voz de verdad dura más que un golpe. Se exige un mínimo
//! de voz por encima del umbral y las pausas entre sílabas no pueden pasar de
//! unos pocos chunks; un clic, un golpe de teclado o un soplido no lo cumplen.

mod adaptive;

pub use adaptive::{
    AdaptiveBargeIn, AdaptiveSettings, BargeInMode, BargeSummary, PlaybackReference,
    ReferenceLevel, REFERENCE_WINDOW_SECS,
};

use crate::CHUNK_SECS;

pub const BARGE_IN_SECS: f64 = 0.25;
/// Chunks seguidos bajo el umbral que rompen la racha (90 ms).
pub const BARGE_IN_MAX_GAP: usize = 3;

/// Decide si el usuario interrumpe de verdad al asistente mientras habla.
///
/// `feed` recibe cada chunk mientras suena el TTS. Devuelve `None` mientras no
/// hay interrupción y, al confirmarla, la racha `[(chunk, nivel), ...]` que
/// forma el principio de la frase del usuario, para no perderlo.
pub struct BargeInDetector {
    pub threshold: f64,
    pub secs: f64,
    loud: usize,
    gap: usize,
    run: Vec<(Vec<u8>, f64)>,
}

impl Default for BargeInDetector {
    fn default() -> Self {
        Self::new(0.008, BARGE_IN_SECS)
    }
}

impl BargeInDetector {
    pub fn new(threshold: f64, secs: f64) -> Self {
        Self {
            threshold,
            secs,
            loud: 0,
            gap: 0,
            run: Vec::new(),
        }
    }

    /// Chunks por encima del umbral que hacen falta para confirmar.
    pub fn need(&self) -> usize {
        let value = self.secs.max(0.0) / CHUNK_SECS;
        // `math.ceil`: NaN e infinito se truncan como en Python.
        let chunks = if value.is_finite() { value.ceil() } else { 0.0 };
        (chunks as usize).max(1)
    }

    pub fn reset(&mut self) {
        self.loud = 0;
        self.gap = 0;
        self.run.clear();
    }

    pub fn feed(&mut self, chunk: &[u8], level: f64) -> Option<Vec<(Vec<u8>, f64)>> {
        if level >= self.threshold {
            self.gap = 0;
            self.loud += 1;
            self.run.push((chunk.to_vec(), level));
        } else if self.loud > 0 {
            self.gap += 1;
            if self.gap > BARGE_IN_MAX_GAP {
                self.reset(); // silencio largo: era un golpe, no una frase
                return None;
            }
            self.run.push((chunk.to_vec(), level));
        } else {
            return None;
        }
        if self.loud >= self.need() {
            let run = std::mem::take(&mut self.run);
            self.reset();
            return Some(run);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector() -> BargeInDetector {
        BargeInDetector::default()
    }

    #[test]
    fn need_rounds_up_to_whole_chunks() {
        assert_eq!(detector().need(), 9); // ceil(0.25 / 0.03)
        assert_eq!(BargeInDetector::new(0.008, 0.0).need(), 1);
        assert_eq!(BargeInDetector::new(0.008, -1.0).need(), 1);
    }

    #[test]
    fn short_burst_is_not_an_interruption() {
        let mut barge = detector();
        for _ in 0..5 {
            assert!(barge.feed(&[1, 2], 0.1).is_none());
        }
    }

    #[test]
    fn sustained_voice_confirms_and_returns_the_run() {
        let mut barge = detector();
        for _ in 0..8 {
            assert!(barge.feed(&[0, 1], 0.1).is_none());
        }
        let run = barge.feed(&[2, 3], 0.1).expect("hay interrupción");
        assert_eq!(run.len(), 9);
        assert_eq!(run[8], (vec![2, 3], 0.1));
        // Tras confirmar, el detector queda limpio.
        assert!(barge.feed(&[4, 5], 0.0).is_none());
    }

    #[test]
    fn gap_between_syllables_is_included() {
        let mut barge = detector();
        for _ in 0..4 {
            barge.feed(&[1], 0.1);
        }
        for _ in 0..3 {
            assert!(barge.feed(&[1], 0.0).is_none());
        }
        for _ in 0..5 {
            if barge.feed(&[1], 0.1).is_some() {
                return;
            }
        }
        panic!("la racha con pausas cortas debe confirmarse");
    }

    #[test]
    fn long_gap_resets_the_streak() {
        let mut barge = detector();
        for _ in 0..4 {
            barge.feed(&[1], 0.1);
        }
        for _ in 0..4 {
            assert!(barge.feed(&[1], 0.0).is_none());
        }
        for _ in 0..8 {
            assert!(barge.feed(&[1], 0.1).is_none());
        }
        assert!(barge.feed(&[1], 0.1).is_some());
    }

    #[test]
    fn voice_while_idle_is_ignored_after_reset() {
        let mut barge = detector();
        barge.reset();
        assert!(barge.feed(&[9], 0.0).is_none());
    }
}

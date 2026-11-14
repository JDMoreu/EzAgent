//! Audio local: captura, VAD, barge-in, remuestreo y Smart Turn (F4-01/F4-02).

pub mod barge_in;
pub mod capture;
pub mod resample;
pub mod segmenter;
pub mod turn;
pub mod wake;

/// Duración de cada chunk de audio (30 ms), como `CHUNK_SECS` en `audio.py`.
pub const CHUNK_SECS: f64 = 0.03;

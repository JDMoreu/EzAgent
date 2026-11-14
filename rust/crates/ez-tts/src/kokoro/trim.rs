//! Port of the `librosa.effects.trim` copy shipped with `kokoro_onnx` (F4-04).
//!
//! Only the call Kokoro makes is ported: mono `float32`, `top_db=60`,
//! `ref=max`, `frame_length=2048`, `hop_length=512`, centered frames padded
//! with zeros. That is what decides which part of every batch is kept.

pub const FRAME_LENGTH: usize = 2048;
pub const HOP_LENGTH: usize = 512;
pub const TOP_DB: f32 = 60.0;
/// `amplitude_to_db(amin=1e-5)` squared by `power_to_db`.
const AMIN_POWER: f32 = 1e-10;

fn frame_rms(audio: &[f32], frame: usize) -> f32 {
    let half = FRAME_LENGTH / 2;
    let mut power = 0.0_f32;
    for offset in 0..FRAME_LENGTH {
        let padded = frame * HOP_LENGTH + offset;
        if padded < half {
            continue;
        }
        if let Some(sample) = audio.get(padded - half) {
            power += sample * sample;
        }
    }
    (power / FRAME_LENGTH as f32).sqrt()
}

fn non_silent_frames(audio: &[f32]) -> Vec<bool> {
    let frames = audio.len() / HOP_LENGTH + 1;
    let rms: Vec<f32> = (0..frames).map(|frame| frame_rms(audio, frame)).collect();
    let loudest = rms.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let reference = 10.0 * (loudest * loudest).max(AMIN_POWER).log10();
    rms.iter()
        .map(|value| 10.0 * (*value * *value).max(AMIN_POWER).log10() - reference > -TOP_DB)
        .collect()
}

/// `trim(y)`: `(y_trimmed, (start, end))`.
pub fn trim(audio: &[f32]) -> (Vec<f32>, (usize, usize)) {
    let non_silent = non_silent_frames(audio);
    let first = non_silent.iter().position(|frame| *frame);
    let last = non_silent.iter().rposition(|frame| *frame);
    let (start, end) = match (first, last) {
        (Some(first), Some(last)) => (first * HOP_LENGTH, audio.len().min((last + 1) * HOP_LENGTH)),
        _ => (0, 0),
    };
    (audio[start..end].to_vec(), (start, end))
}

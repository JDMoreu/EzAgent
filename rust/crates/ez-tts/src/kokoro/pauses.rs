//! Port of `kokoro_onnx.pauses` (F4-04): silence at sentence and clause ends.

use super::chunker::{CLAUSE_MARKS, SENTENCE_MARKS};
use super::sliding::Timing;

const QUIET_DB: f32 = -40.0;
const FRAME_SECS: f64 = 0.01;
const REACH: usize = 15;

/// `wanted_after`: pause the text asks for after this phoneme.
pub fn wanted_after(phoneme: &str, sentence: f64, clause: f64) -> f64 {
    if SENTENCE_MARKS.contains(phoneme) {
        return sentence;
    }
    if CLAUSE_MARKS.contains(phoneme) {
        return clause;
    }
    0.0
}

/// `_quiet_frames`: frames quiet enough to be part of a pause.
fn quiet_frames(audio: &[f32], frame: usize) -> Vec<bool> {
    let mut loudness = Vec::new();
    for chunk in audio.chunks_exact(frame) {
        let power = chunk.iter().map(|value| value * value).sum::<f32>();
        loudness.push((power / frame as f32).sqrt());
    }
    let loudest = loudness.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let threshold = loudest * 10_f32.powf(QUIET_DB / 20.0);
    loudness.iter().map(|value| *value <= threshold).collect()
}

/// `_run_around`: the quiet run touching frame `at`, as `(start, length)`.
fn run_around(quiet: &[bool], at: usize, reach: usize) -> (usize, usize) {
    let mut inside = None;
    for distance in 0..=reach {
        if let Some(index) = at.checked_sub(distance) {
            if quiet.get(index) == Some(&true) {
                inside = Some(index);
                break;
            }
        }
        if distance > 0 && quiet.get(at + distance) == Some(&true) {
            inside = Some(at + distance);
            break;
        }
    }
    let Some(mut start) = inside else {
        return (at, 0);
    };
    let mut end = start;
    while start > 0 && quiet[start - 1] {
        start -= 1;
    }
    while end + 1 < quiet.len() && quiet[end + 1] {
        end += 1;
    }
    (start, end - start + 1)
}

/// `insert`: lengthen the pause after every mark and move later timings.
pub fn insert(
    audio: &[f32],
    timings: &[Timing],
    sample_rate: u32,
    sentence: f64,
    clause: f64,
) -> (Vec<f32>, Vec<Timing>) {
    if timings.is_empty() || (sentence == 0.0 && clause == 0.0) {
        return (audio.to_vec(), timings.to_vec());
    }
    let frame = ((FRAME_SECS * f64::from(sample_rate)) as usize).max(1);
    let quiet = quiet_frames(audio, frame);
    let rate = f64::from(sample_rate);

    let mut parts: Vec<&[f32]> = Vec::new();
    let mut zeroes: Vec<Vec<f32>> = Vec::new();
    let mut moved = Vec::with_capacity(timings.len());
    let mut cut = 0_usize;
    let mut shift = 0.0_f64;

    for timing in timings {
        moved.push(Timing {
            phoneme: timing.phoneme.clone(),
            start: timing.start + shift,
            end: timing.end + shift,
        });
        let target = wanted_after(&timing.phoneme, sentence, clause);
        if target == 0.0 {
            continue;
        }
        let at = (timing.end * rate) as usize / frame;
        let (start, length) = run_around(&quiet, at, REACH);
        let missing = target - length as f64 * FRAME_SECS;
        if length == 0 || missing <= 0.0 {
            continue;
        }
        let middle = (start + length / 2) * frame;
        parts.push(audio.get(cut..middle).unwrap_or(&[]));
        zeroes.push(vec![0.0; (missing * rate) as usize]);
        cut = middle;
        shift += missing;
    }

    if parts.is_empty() {
        return (audio.to_vec(), timings.to_vec());
    }
    parts.push(audio.get(cut..).unwrap_or(&[]));
    let mut output = Vec::new();
    for (index, part) in parts.iter().enumerate() {
        output.extend_from_slice(part);
        if let Some(silence) = zeroes.get(index) {
            output.extend_from_slice(silence);
        }
    }
    (output, moved)
}

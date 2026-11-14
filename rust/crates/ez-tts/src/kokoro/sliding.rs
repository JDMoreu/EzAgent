//! Port of `kokoro_onnx.sliding` (F4-04): windowed synthesis and timings.

use std::collections::HashSet;

pub const KEEP: usize = 300;
pub const CONTEXT: usize = 60;
pub const LOOKAHEAD: usize = 60;
pub const CROSSFADE: f64 = 0.025;

const STOP_SEARCH: i64 = 70;
const GAP_SEARCH: i64 = 12;
const QUIET_WINDOW: f64 = 0.02;

/// When a phoneme is spoken, in seconds from the start of the audio.
#[derive(Debug, Clone, PartialEq)]
pub struct Timing {
    pub phoneme: String,
    pub start: f64,
    pub end: f64,
}

/// Window parameters; the default is the Python's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SynthOptions {
    pub keep: usize,
    pub context: usize,
    pub lookahead: usize,
    pub crossfade: f64,
}

impl Default for SynthOptions {
    fn default() -> Self {
        Self {
            keep: KEEP,
            context: CONTEXT,
            lookahead: LOOKAHEAD,
            crossfade: CROSSFADE,
        }
    }
}

/// `token_edges`: sample offset of every token boundary.
pub fn token_edges(duration: &[i64], samples: usize) -> Result<Vec<i64>, String> {
    let mut frames = Vec::with_capacity(duration.len() + 1);
    frames.push(0_i64);
    let mut total = 0_i64;
    for frame in duration {
        total += *frame;
        frames.push(total);
    }
    if total == 0 {
        // DECISION: the Python divides by zero and casts NaN; an error keeps
        // a broken model from poisoning every timing.
        return Err("duration is empty".to_string());
    }
    let ratio = samples as f64 / total as f64;
    Ok(frames
        .into_iter()
        .map(|frame| frame as f64 * ratio)
        .map(f64::round_ties_even)
        .map(|edge| edge as i64)
        .collect())
}

/// `timings`: pair every phoneme with the span of audio it occupies.
pub fn timings(phonemes: &str, edges: &[i64], sample_rate: u32) -> Vec<Timing> {
    let rate = f64::from(sample_rate);
    phonemes
        .chars()
        .enumerate()
        .filter(|(index, _)| index + 1 < edges.len())
        .map(|(index, phoneme)| Timing {
            phoneme: phoneme.to_string(),
            start: edges[index] as f64 / rate,
            end: edges[index + 1] as f64 / rate,
        })
        .collect()
}

fn quietest(
    audio: &[f32],
    edges: &[i64],
    offset: usize,
    candidates: &[usize],
    sample_rate: u32,
) -> usize {
    let half = (QUIET_WINDOW * f64::from(sample_rate)) as usize;
    let mut best = candidates.first().copied().unwrap_or(0);
    let mut quietest: Option<f32> = None;
    for token in candidates {
        let at = edges[token - offset + 1].max(0) as usize;
        let start = at.saturating_sub(half);
        let end = (at + half).min(audio.len());
        let frame = audio.get(start..end).unwrap_or(&[]);
        let energy = if frame.is_empty() {
            1.0
        } else {
            (frame.iter().map(|value| value * value).sum::<f32>() / frame.len() as f32).sqrt()
        };
        if quietest.is_none_or(|current| energy < current) {
            best = *token;
            quietest = Some(energy);
        }
    }
    best
}

/// `_join`: crossfade the kept regions, reporting where each one starts.
pub(super) fn join(pieces: &[Vec<f32>], fade: usize) -> (Vec<f32>, Vec<usize>) {
    let Some(first) = pieces.first() else {
        return (Vec::new(), Vec::new());
    };
    let mut audio = first.clone();
    let mut starts = vec![0_usize];
    for piece in &pieces[1..] {
        let overlap = fade.min(audio.len()).min(piece.len());
        starts.push(audio.len() - overlap);
        if overlap == 0 {
            audio.extend_from_slice(piece);
            continue;
        }
        let mut blended = Vec::with_capacity(audio.len() + piece.len() - overlap);
        blended.extend_from_slice(&audio[..audio.len() - overlap]);
        for index in 0..overlap {
            let ramp = if overlap == 1 {
                0.0
            } else {
                (index as f64 / (overlap - 1) as f64) as f32
            };
            blended.push(audio[audio.len() - overlap + index] * (1.0 - ramp) + piece[index] * ramp);
        }
        blended.extend_from_slice(&piece[overlap..]);
        audio = blended;
    }
    (audio, starts)
}

/// `_candidates`: tokens a window may end on, sentence ends first.
pub(super) fn candidates(
    tokens: &[i64],
    stops: &HashSet<i64>,
    spaces: &HashSet<i64>,
    target: usize,
    floor: usize,
    limit: usize,
) -> Vec<usize> {
    let within = |reach: i64, wanted: &HashSet<i64>, shift: i64| -> Vec<usize> {
        let reach = reach.min(1_i64.max(target as i64 - floor as i64));
        let mut found = Vec::new();
        for index in target as i64 - reach..target as i64 + reach {
            let token = index + shift;
            if token <= floor as i64 || token >= limit as i64 {
                continue;
            }
            let Some(value) = usize::try_from(index)
                .ok()
                .and_then(|index| tokens.get(index))
            else {
                continue;
            };
            if wanted.contains(value) {
                found.push(token as usize);
            }
        }
        found
    };
    let stops_found = within(STOP_SEARCH, stops, 1);
    if !stops_found.is_empty() {
        return stops_found;
    }
    let spaces_found = within(GAP_SEARCH, spaces, 0);
    if !spaces_found.is_empty() {
        return spaces_found;
    }
    vec![target]
}

/// `synthesize`: overlapping windows with only their middles kept.
///
/// The Python reads `edges[end - first + 1]`, which for the last window is
/// one past the end: the error is reproduced here on purpose (see the report).
pub fn synthesize<F>(
    mut run: F,
    tokens: &[i64],
    sample_rate: u32,
    stops: &HashSet<i64>,
    spaces: &HashSet<i64>,
    options: &SynthOptions,
) -> Result<(Vec<f32>, Vec<i64>), String>
where
    F: FnMut(&[i64]) -> Result<(Vec<f32>, Vec<i64>), String>,
{
    let mut pieces: Vec<Vec<f32>> = Vec::new();
    let mut spans: Vec<Vec<i64>> = Vec::new();
    let mut start = 0_usize;
    while start < tokens.len() {
        let mut end = (start + options.keep).min(tokens.len());
        let first = start.saturating_sub(options.context);
        let last = (end + options.lookahead).min(tokens.len());
        let (audio, duration) = run(&tokens[first..last])?;
        let edges = token_edges(&duration, audio.len())?;
        if end < tokens.len() {
            let found = candidates(tokens, stops, spaces, end, start, last);
            end = quietest(&audio, &edges, first, &found, sample_rate);
        }
        if end <= start {
            return Err(format!("window {start}:{end} makes no progress"));
        }
        let begin = start - first + 1;
        let finish = end - first + 1;
        let size = edges.len();
        let begin_edge = *edges
            .get(begin)
            .ok_or_else(|| format!("index {begin} is out of bounds for axis 0 with size {size}"))?;
        let finish_edge = *edges.get(finish).ok_or_else(|| {
            format!("index {finish} is out of bounds for axis 0 with size {size}")
        })?;
        pieces.push(audio[begin_edge as usize..finish_edge as usize].to_vec());
        spans.push(
            edges[begin..=finish]
                .iter()
                .map(|edge| edge - begin_edge)
                .collect(),
        );
        start = end;
    }

    let fade = (options.crossfade * f64::from(sample_rate)) as usize;
    let (audio, starts) = join(&pieces, fade);
    let mut offsets = Vec::new();
    for (span, base) in spans.iter().zip(&starts) {
        let length = span.len().saturating_sub(1);
        offsets.extend(span[..length].iter().map(|offset| offset + *base as i64));
    }
    offsets.push(audio.len() as i64);
    Ok((audio, offsets))
}

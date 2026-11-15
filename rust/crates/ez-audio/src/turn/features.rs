//! Características de Whisper para Smart Turn (F4-02).
//!
//! Son las de `WhisperFeatureExtractor(chunk_length=8)` reescritas con numpy en
//! `turn.py`, sin depender de `transformers`: log-mel de 80 x 800 sobre 8 s de
//! audio a 16 kHz.

use std::sync::OnceLock;

use crate::resample::resample_linear;

pub const SAMPLE_RATE: u32 = 16_000;
pub const WINDOW_SECS: usize = 8;
pub const N_SAMPLES: usize = SAMPLE_RATE as usize * WINDOW_SECS;
pub const N_FFT: usize = 400;
pub const HOP: usize = 160;
pub const N_MELS: usize = 80;
/// Fotogramas que quedan tras descartar el último (`[:, :-1]`).
pub const N_FRAMES: usize = 800;

/// Entrada del modelo: `(1, 80, 800)` en orden de filas.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnFeatures {
    pub data: Vec<f32>,
}

impl TurnFeatures {
    pub const CHANNELS: usize = 1;
    pub const MELS: usize = N_MELS;
    pub const FRAMES: usize = N_FRAMES;

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Escala mel de Slaney (la que usa Whisper).
pub fn hz_to_mel(freq: &[f64]) -> Vec<f64> {
    let logstep = 27.0 / 6.4f64.ln();
    freq.iter()
        .map(|value| {
            if *value >= 1000.0 {
                15.0 + (value / 1000.0).ln() * logstep
            } else {
                3.0 * value / 200.0
            }
        })
        .collect()
}

pub fn mel_to_hz(mels: &[f64]) -> Vec<f64> {
    let logstep = 6.4f64.ln() / 27.0;
    mels.iter()
        .map(|value| {
            if *value >= 15.0 {
                1000.0 * (logstep * (value - 15.0)).exp()
            } else {
                200.0 * value / 3.0
            }
        })
        .collect()
}

/// `np.linspace(start, stop, count)`.
fn linspace(start: f64, stop: f64, count: usize) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![start];
    }
    let step = (stop - start) / (count - 1) as f64;
    let mut values: Vec<f64> = (0..count)
        .map(|index| start + step * index as f64)
        .collect();
    values[count - 1] = stop;
    values
}

/// Banco de filtros triangulares con normalización de Slaney, en orden de fila
/// mel (`N_MELS x 201`), como `_FILTERS.T`.
pub fn mel_filters() -> &'static [f64] {
    static FILTERS: OnceLock<Vec<f64>> = OnceLock::new();
    FILTERS.get_or_init(build_mel_filters)
}

fn build_mel_filters() -> Vec<f64> {
    let bins = N_FFT / 2 + 1;
    let fft_freqs = linspace(0.0, (SAMPLE_RATE / 2) as f64, bins);
    let edges = hz_to_mel(&[0.0, SAMPLE_RATE as f64 / 2.0]);
    let filter_freqs = mel_to_hz(&linspace(edges[0], edges[1], N_MELS + 2));
    let enorm: Vec<f64> = (0..N_MELS)
        .map(|index| 2.0 / (filter_freqs[index + 2] - filter_freqs[index]))
        .collect();
    let mut filters = vec![0.0f64; bins * N_MELS];
    for (bin, freq) in fft_freqs.iter().enumerate() {
        for mel in 0..N_MELS {
            let down = -(filter_freqs[mel] - freq) / (filter_freqs[mel + 1] - filter_freqs[mel]);
            let up =
                (filter_freqs[mel + 2] - freq) / (filter_freqs[mel + 2] - filter_freqs[mel + 1]);
            let value = down.min(up).max(0.0);
            filters[mel * bins + bin] = value * enorm[mel];
        }
    }
    filters
}

/// Hann periódica (`np.hanning(N_FFT + 1)[:-1]`).
pub fn hann_window() -> &'static [f64] {
    static WINDOW: OnceLock<Vec<f64>> = OnceLock::new();
    WINDOW.get_or_init(|| {
        (0..N_FFT)
            .map(|index| {
                let angle = 2.0 * std::f64::consts::PI * index as f64 / N_FFT as f64;
                0.5 - 0.5 * angle.cos()
            })
            .collect()
    })
}

/// Tablas de coseno y seno de la DFT real de 400 puntos.
///
/// shortcut: DFT directa O(N²) por fotograma (64 M de multiplicaciones por
/// ventana). Si Smart Turn entra en el perfil de CPU, cambiar a una FFT mixta
/// (400 = 2^4 * 5^2) o a Bluestein.
struct Dft {
    cos: Vec<f64>,
    sin: Vec<f64>,
}

fn dft() -> &'static Dft {
    static TABLE: OnceLock<Dft> = OnceLock::new();
    TABLE.get_or_init(|| {
        let bins = N_FFT / 2 + 1;
        let mut cos = vec![0.0f64; bins * N_FFT];
        let mut sin = vec![0.0f64; bins * N_FFT];
        for bin in 0..bins {
            for sample in 0..N_FFT {
                let angle = 2.0 * std::f64::consts::PI * (bin * sample) as f64 / N_FFT as f64;
                cos[bin * N_FFT + sample] = angle.cos();
                sin[bin * N_FFT + sample] = angle.sin();
            }
        }
        Dft { cos, sin }
    })
}

/// `|rfft(x, 400)|²` de una ventana ya multiplicada por Hann.
fn power_spectrum(windowed: &[f64]) -> Vec<f64> {
    let table = dft();
    let bins = N_FFT / 2 + 1;
    let mut spectrum = Vec::with_capacity(bins);
    for bin in 0..bins {
        let offset = bin * N_FFT;
        let cos = &table.cos[offset..offset + N_FFT];
        let sin = &table.sin[offset..offset + N_FFT];
        let mut real = 0.0;
        let mut imaginary = 0.0;
        for ((value, cosine), sine) in windowed.iter().zip(cos).zip(sin) {
            real += value * cosine;
            imaginary += value * sine;
        }
        spectrum.push(real * real + imaginary * imaginary);
    }
    spectrum
}

/// `np.pad(audio, N_FFT // 2, mode="reflect")`.
fn reflect_pad(audio: &[f64], pad: usize) -> Result<Vec<f64>, String> {
    const EMPTY: &str = "can't extend empty axis 0 using modes other than 'constant' or 'empty'";
    if audio.is_empty() {
        return Err(EMPTY.to_string());
    }
    let mut padded = Vec::with_capacity(audio.len() + 2 * pad);
    if audio.len() > pad {
        for index in (1..=pad).rev() {
            padded.push(audio[index]);
        }
        padded.extend_from_slice(audio);
        for index in 0..pad {
            padded.push(audio[audio.len() - 2 - index]);
        }
    } else {
        // DECISION: el Python lanza con menos de `pad + 1` muestras; se rellena
        // con ceros para no entrar en pánico desde el hilo de audio.
        padded.resize(pad, 0.0);
        padded.extend_from_slice(audio);
        padded.resize(pad + audio.len() + pad, 0.0);
    }
    Ok(padded)
}

/// Log-mel de Whisper (80 x 800) de 8 s de audio a 16 kHz, ya normalizado.
pub fn log_mel(audio: &[f64]) -> Result<Vec<f32>, String> {
    let padded = reflect_pad(audio, N_FFT / 2)?;
    let frames = 1 + padded.len().saturating_sub(N_FFT) / HOP;
    if frames <= 1 {
        // `spec[:, :-1]` vacío y `spec.max()` revienta igual que en numpy.
        return Err(
            "zero-size array to reduction operation maximum which has no identity".to_string(),
        );
    }
    let window = hann_window();
    let filters = mel_filters();
    let bins = N_FFT / 2 + 1;
    let mut mel = vec![0.0f64; N_MELS * frames];
    for frame in 0..frames {
        let start = frame * HOP;
        let windowed: Vec<f64> = padded[start..start + N_FFT]
            .iter()
            .zip(window)
            .map(|(value, weight)| value * weight)
            .collect();
        let spectrum = power_spectrum(&windowed);
        for (row, slot) in mel.chunks_exact_mut(frames).enumerate() {
            let low = row * bins;
            let energy: f64 = filters[low..low + bins]
                .iter()
                .zip(&spectrum)
                .map(|(filter, power)| filter * power)
                .sum();
            slot[frame] = energy.max(1e-10);
        }
    }
    let spec_frames = frames - 1;
    let mut spec = Vec::with_capacity(N_MELS * spec_frames);
    for row in 0..N_MELS {
        for frame in 0..spec_frames {
            spec.push(mel[row * frames + frame].log10());
        }
    }
    let top = spec.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Ok(spec
        .iter()
        .map(|value| ((value.max(top - 8.0) + 4.0) / 4.0) as f32)
        .collect())
}

/// PCM int16 mono -> entrada del modelo (1 x 80 x 800).
///
/// `rate` es `i32` como el `rate` de Python: una tasa negativa remuestrea a una
/// sola muestra, y la cero reproduce el `ZeroDivisionError`.
pub fn prepare(pcm: &[u8], rate: i32) -> Result<TurnFeatures, String> {
    if !pcm.len().is_multiple_of(2) {
        return Err("buffer size must be a multiple of element size".to_string());
    }
    let samples: Vec<f32> = pcm
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| i16::from_le_bytes(*pair) as f32 / 32768.0)
        .collect();
    let audio: Vec<f64> = if rate != SAMPLE_RATE as i32 && !samples.is_empty() {
        resample_linear(&samples, rate, SAMPLE_RATE as i32)?
    } else {
        samples.iter().map(|value| f64::from(*value)).collect()
    };
    // Como el inference.py de referencia: los últimos 8 s, con ceros al
    // PRINCIPIO para que el final de la voz quede al final de la ventana.
    let mut window = vec![0.0f64; N_SAMPLES];
    let start = audio.len().saturating_sub(N_SAMPLES);
    let tail = &audio[start..];
    window[N_SAMPLES - tail.len()..].copy_from_slice(tail);
    let mean = window.iter().sum::<f64>() / N_SAMPLES as f64;
    let variance = window
        .iter()
        .map(|value| (value - mean) * (value - mean))
        .sum::<f64>()
        / N_SAMPLES as f64;
    let scale = (variance + 1e-7).sqrt();
    for value in &mut window {
        *value = (*value - mean) / scale;
    }
    Ok(TurnFeatures {
        data: log_mel(&window)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_scale_round_trips() {
        let freqs = [0.0, 500.0, 1000.0, 4000.0, 8000.0];
        let mels = hz_to_mel(&freqs);
        let back = mel_to_hz(&mels);
        for (original, round_trip) in freqs.iter().zip(back) {
            assert!((original - round_trip).abs() < 1e-9);
        }
        assert!((mels[2] - 15.0).abs() < 1e-12);
        let ramp: Vec<f64> = (0..=16).map(|index| 100.0 * index as f64).collect();
        let scale = hz_to_mel(&ramp);
        assert!(scale.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn linspace_matches_numpy() {
        let values = linspace(0.0, 10.0, 5);
        assert_eq!(values, vec![0.0, 2.5, 5.0, 7.5, 10.0]);
        assert_eq!(linspace(3.0, 9.0, 1), vec![3.0]);
        assert!(linspace(0.0, 1.0, 0).is_empty());
    }

    #[test]
    fn filters_are_triangular_and_normalized() {
        let filters = mel_filters();
        let bins = N_FFT / 2 + 1;
        assert_eq!(filters.len(), bins * N_MELS);
        assert!(filters.iter().all(|value| *value >= 0.0));
        let first: f64 = filters[..bins].iter().sum();
        let middle: f64 = filters[20 * bins..21 * bins].iter().sum();
        assert!(first > 0.0);
        assert!(middle > first);
        // El pico del primer filtro cae en el segundo bin de FFT (40 Hz).
        let peak = filters[..bins]
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .expect("hay bins");
        assert_eq!(peak, 1);
    }

    #[test]
    fn hann_window_is_periodic() {
        let window = hann_window();
        assert_eq!(window.len(), N_FFT);
        assert!(window[0].abs() < 1e-12);
        assert!((window[N_FFT / 2] - 1.0).abs() < 1e-12);
        assert!(window[N_FFT - 1] > 0.0);
    }

    #[test]
    fn silence_is_flat_after_normalization() {
        let mel = log_mel(&vec![0.0; N_SAMPLES]).expect("silencio válido");
        assert_eq!(mel.len(), N_MELS * N_FRAMES);
        assert!(mel.iter().all(|value| (value + 1.5).abs() < 1e-5));
    }

    #[test]
    fn prepare_pads_the_tail_at_the_end_of_the_window() {
        // 9 s de audio: 8 s de silencio y medio segundo de tono al final.
        let total = N_SAMPLES + SAMPLE_RATE as usize / 2;
        let mut pcm = Vec::with_capacity(total * 2);
        for index in 0..total {
            let value = if index >= N_SAMPLES {
                let phase =
                    2.0 * std::f64::consts::PI * 440.0 * (index - N_SAMPLES) as f64 / 16_000.0;
                (phase.sin() * 16_000.0) as i16
            } else {
                0
            };
            pcm.extend_from_slice(&value.to_le_bytes());
        }
        let features = prepare(&pcm, SAMPLE_RATE as i32).expect("tasa válida");
        assert_eq!(features.len(), N_MELS * N_FRAMES);
        assert!(features.data.iter().all(|value| value.is_finite()));
        // La ventana son los últimos 8 s: el primer fotograma es silencio y el
        // último tiene el tono de 440 Hz, que cae en la banda mel 11.
        let first: f32 = features.data[..N_MELS].iter().sum();
        let last: f32 = features.data[N_MELS * (N_FRAMES - 1)..].iter().sum();
        assert!(last > first, "el final de la voz debe quedar al final");
        let peak = (0..N_MELS)
            .max_by(|left, right| {
                features.data[left * N_FRAMES + N_FRAMES - 1]
                    .total_cmp(&features.data[right * N_FRAMES + N_FRAMES - 1])
            })
            .expect("hay bandas");
        assert_eq!(peak, 11);
    }

    #[test]
    fn empty_pcm_still_produces_a_valid_input() {
        let features = prepare(&[], SAMPLE_RATE as i32).expect("tasa válida");
        assert_eq!(features.len(), N_MELS * N_FRAMES);
        assert!(features.data.iter().all(|value| (value + 1.5).abs() < 1e-5));
    }

    #[test]
    fn volume_does_not_change_the_features() {
        let mut loud = Vec::with_capacity(N_SAMPLES * 2);
        let mut soft = Vec::with_capacity(N_SAMPLES * 2);
        for index in 0..N_SAMPLES {
            let phase = 2.0 * std::f64::consts::PI * 440.0 * index as f64 / 16_000.0;
            let value = (phase.sin() * 12_000.0) as i16;
            loud.extend_from_slice(&value.to_le_bytes());
            soft.extend_from_slice(&((f64::from(value) * 0.5) as i16).to_le_bytes());
        }
        let loud = prepare(&loud, SAMPLE_RATE as i32).expect("tasa válida");
        let soft = prepare(&soft, SAMPLE_RATE as i32).expect("tasa válida");
        for (loud, soft) in loud.data.iter().zip(soft.data) {
            assert!((loud - soft).abs() < 0.01, "{loud} vs {soft}");
        }
    }
}

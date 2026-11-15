//! Port of `src/ezagent/voice/turn.py` (`prepare`, remuestreo) (F4-01).
//!
//! Interpolación lineal equivalente a
//! `np.interp(np.linspace(0, n - 1, len), np.arange(n), audio)`: para clasificar
//! el fin de turno no hace falta un remuestreo de calidad de estudio.

/// `int(round(n * to / from))` con el redondeo a par de Python.
///
/// `from_rate == 0` reproduce el `ZeroDivisionError` del Python.
pub fn resampled_len(length: usize, from_rate: i32, to_rate: i32) -> Result<usize, String> {
    if from_rate == 0 {
        return Err("division by zero".to_string());
    }
    let exact = length as f64 * f64::from(to_rate) / f64::from(from_rate);
    Ok(exact.round_ties_even().max(0.0) as usize)
}

/// Remuestrea por interpolación lineal a `to_rate`.
///
/// Devuelve `float64` como `np.interp`.
pub fn resample_linear(samples: &[f32], from_rate: i32, to_rate: i32) -> Result<Vec<f64>, String> {
    let source: Vec<f64> = samples.iter().map(|value| f64::from(*value)).collect();
    if source.is_empty() || from_rate == to_rate {
        return Ok(source);
    }
    let length = resampled_len(source.len(), from_rate, to_rate)?.max(1);
    let last = (source.len() - 1) as f64;
    Ok((0..length)
        .map(|index| {
            let position = if length == 1 {
                0.0
            } else {
                last * index as f64 / (length - 1) as f64
            };
            interpolate(&source, position)
        })
        .collect())
}

/// `np.interp(position, 0..n-1, source)` para una posición.
fn interpolate(source: &[f64], position: f64) -> f64 {
    let last = source.len() - 1;
    if position <= 0.0 {
        return source[0];
    }
    if position >= last as f64 {
        return source[last];
    }
    let low = position.floor() as usize;
    let fraction = position - low as f64;
    source[low] + fraction * (source[low + 1] - source[low])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_rate_is_a_copy() {
        let input = [0.0, 0.5, -0.5];
        assert_eq!(
            resample_linear(&input, 16_000, 16_000),
            Ok(vec![0.0, 0.5, -0.5])
        );
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(resample_linear(&[], 8_000, 16_000), Ok(Vec::new()));
    }

    #[test]
    fn linear_interpolation_doubles_a_ramp() {
        let input: Vec<f32> = (0..5).map(|value| value as f32).collect();
        let output = resample_linear(&input, 8_000, 16_000).expect("tasa válida");
        assert_eq!(output.len(), 10); // round(5 * 16000 / 8000)
        for (index, got) in output.iter().enumerate() {
            let want = 4.0 * index as f64 / 9.0;
            assert!((got - want).abs() < 1e-12, "{got} != {want}");
        }
    }

    #[test]
    fn positions_are_evenly_spaced() {
        let input: Vec<f32> = (0..3).map(|value| value as f32).collect();
        let output = resample_linear(&input, 16_000, 8_000).expect("tasa válida");
        assert_eq!(output.len(), 2); // round(3 / 2) = round_ties_even(1.5) = 2
        assert!((output[0] - 0.0).abs() < 1e-12);
        assert!((output[1] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn goes_down_and_up() {
        let input: Vec<f32> = (0..3).map(|value| value as f32).collect();
        let output = resample_linear(&input, 16_000, 48_000).expect("tasa válida");
        assert_eq!(output.len(), 9);
        assert!((output[0] - 0.0).abs() < 1e-12);
        assert!((output[4] - 1.0).abs() < 1e-12);
        assert!((output[8] - 2.0).abs() < 1e-12);
    }

    #[test]
    fn zero_from_rate_is_an_error() {
        assert_eq!(
            resample_linear(&[1.0], 0, 16_000),
            Err("division by zero".to_string())
        );
        assert_eq!(
            resampled_len(3, 0, 16_000),
            Err("division by zero".to_string())
        );
    }

    #[test]
    fn negative_length_collapses_to_one_sample() {
        let output = resample_linear(&[0.5, -0.5], -16_000, 16_000).expect("tasa válida");
        assert_eq!(output, vec![0.5]);
    }
}

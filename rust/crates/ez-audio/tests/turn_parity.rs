//! Paridad del dominio `turn` contra `parity/fixtures/turn.jsonl`.
//!
//! Cubre `hz_to_mel`, `mel_to_hz`, `log_mel` y `prepare` de
//! `ezagent.voice.turn` sobre los WAV de `parity/golden/audio/`.
//!
//! Los 27 casos de `probability` necesitan una sesión de ONNX Runtime, y
//! `ez-audio` no declara `ort` (y este brief no puede tocar `Cargo.toml`): se
//! validan sus entradas (WAV y sha256) y se cuentan aparte con un aviso.

use std::path::Path;

use ez_audio::resample::resampled_len;
use ez_audio::turn::{hz_to_mel, log_mel, mel_to_hz, prepare, N_FFT, N_MELS, SAMPLE_RATE};
use ez_parity::fixtures::{self, FixtureCase};

const FIXTURES: &str = "../../../parity/fixtures/turn.jsonl";
const ROOT: &str = "../../..";
const TOTAL_CASES: usize = 87;

#[test]
fn every_turn_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de turn legibles");
    let mut cases_run = 0;
    let mut cases_skipped = 0;
    for case in &cases {
        match case.function.as_str() {
            "hz_to_mel" => compare_hz(case),
            "mel_to_hz" => compare_mel(case),
            "log_mel" => compare_log_mel(case),
            "prepare" => compare_prepare(case),
            "probability" => {
                check_probability_input(case);
                cases_skipped += 1;
                continue;
            }
            other => panic!("caso sin cubrir: {} ({other})", case.case),
        }
        cases_run += 1;
    }
    if cases_skipped > 0 {
        eprintln!(
            "turn: {cases_skipped} casos de `probability` omitidos: ez-audio no declara \
             `ort` (no hay backend ONNX en este crate); sus WAV y sha256 sí se validaron"
        );
    }
    let cases_in_file = cases.len();
    assert!(
        cases_in_file >= TOTAL_CASES,
        "se esperaban al menos {TOTAL_CASES} casos, hay {cases_in_file}"
    );
    assert_eq!(
        cases_run + cases_skipped,
        cases_in_file,
        "se saltó algún caso"
    );
}

fn wav_pcm(relative: &str) -> Vec<u8> {
    let path = Path::new(ROOT).join(relative);
    let bytes = std::fs::read(&path).expect("WAV del caso");
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        panic!("{}: no es un WAV RIFF", path.display());
    }
    let mut offset = 12;
    let mut data = Vec::new();
    while offset + 8 <= bytes.len() {
        let size =
            u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().expect("tamaño")) as usize;
        let body = offset + 8;
        if &bytes[offset..offset + 4] == b"data" {
            data.extend_from_slice(&bytes[body..(body + size).min(bytes.len())]);
        }
        offset = body + size + (size % 2);
    }
    data
}

fn audio_of(pcm: &[u8]) -> Vec<f64> {
    pcm.as_chunks::<2>()
        .0
        .iter()
        .map(|pair| f64::from(i16::from_le_bytes(*pair)) / 32768.0)
        .collect()
}

fn close(produced: f64, expected: f64, rel: f64, field: &str, case: &str) {
    let tolerance = rel * expected.abs().max(1.0);
    assert!(
        (produced - expected).abs() <= tolerance,
        "{case}: {field} {produced} != {expected}"
    );
}

fn compare_floats(produced: &[f64], expected: &[f64], field: &str, case: &str) {
    assert_eq!(produced.len(), expected.len(), "{case}: largo de {field}");
    for (index, value) in produced.iter().enumerate() {
        close(*value, expected[index], 1e-9, field, case);
    }
}

fn compare_hz(case: &FixtureCase) {
    let freqs: Vec<f64> = case.input["freqs"]
        .as_array()
        .expect("freqs")
        .iter()
        .map(|value| value.as_f64().unwrap_or(0.0))
        .collect();
    let expected: Vec<f64> = case.output["mels"]
        .as_array()
        .expect("mels")
        .iter()
        .map(|value| value.as_f64().unwrap_or(0.0))
        .collect();
    compare_floats(&hz_to_mel(&freqs), &expected, "mels", &case.case);
}

fn compare_mel(case: &FixtureCase) {
    let mels: Vec<f64> = case.input["mels"]
        .as_array()
        .expect("mels")
        .iter()
        .map(|value| value.as_f64().unwrap_or(0.0))
        .collect();
    let expected: Vec<f64> = case.output["freqs"]
        .as_array()
        .expect("freqs")
        .iter()
        .map(|value| value.as_f64().unwrap_or(0.0))
        .collect();
    compare_floats(&mel_to_hz(&mels), &expected, "freqs", &case.case);
}

fn microphone_samples(case: &FixtureCase) -> Vec<f64> {
    let count = case.input["count"].as_u64().unwrap_or(0) as usize;
    let mut samples = vec![0.0; count];
    if count == 3 {
        samples = vec![0.0, 0.1, -0.1];
    } else if count == N_FFT / 2 + 1 {
        samples[N_FFT / 4] = 0.5;
    }
    samples
}

fn compare_log_mel(case: &FixtureCase) {
    let audio = if let Some(relative) = case.input["wav"].as_str() {
        audio_of(&wav_pcm(relative))
    } else {
        microphone_samples(case)
    };
    let produced = log_mel(&audio);
    match case.error.as_deref() {
        Some(expected) => {
            assert_eq!(
                produced.expect_err("el fixture espera un error de numpy"),
                expected,
                "caso {}",
                case.case
            );
        }
        None => compare_summary(case, &produced.expect("audio válido")),
    }
}

fn compare_prepare(case: &FixtureCase) {
    let pcm = if let Some(relative) = case.input["wav"].as_str() {
        wav_pcm(relative)
    } else {
        let hex = case.input["pcm"]["hex"].as_str().unwrap_or("");
        hex.as_bytes()
            .chunks(2)
            .filter(|pair| pair.len() == 2)
            .map(|pair| {
                let high = (pair[0] as char).to_digit(16).unwrap_or(0);
                let low = (pair[1] as char).to_digit(16).unwrap_or(0);
                (high * 16 + low) as u8
            })
            .collect()
    };
    let rate = case.input["rate"].as_i64().unwrap_or(0) as i32;
    let produced = prepare(&pcm, rate);
    match case.error.as_deref() {
        Some(expected) => {
            assert_eq!(
                produced.expect_err("el fixture espera un error"),
                expected,
                "caso {}",
                case.case
            );
        }
        None => {
            let features = produced.expect("caso válido");
            compare_summary(case, &features.data);
            let samples = if pcm.is_empty() {
                0
            } else if rate == SAMPLE_RATE as i32 {
                pcm.len() / 2
            } else {
                resampled_len(pcm.len() / 2, rate, SAMPLE_RATE as i32)
                    .expect("tasa válida")
                    .max(1)
            };
            assert_eq!(
                samples as u64,
                case.output["resampled_samples"].as_u64().unwrap_or(0),
                "resampled_samples de {}",
                case.case
            );
        }
    }
}

/// Resumen exacto de la matriz log-mel, como `_summary` del generador.
fn compare_summary(case: &FixtureCase, data: &[f32]) {
    let output = &case.output;
    let shape = output["shape"].as_array().expect("shape");
    let rows = shape[0].as_u64().unwrap_or(0) as usize;
    let columns = shape[1].as_u64().unwrap_or(0) as usize;
    assert_eq!(rows, N_MELS);
    assert_eq!(data.len(), rows * columns, "largo de {}", case.case);
    let min = data.iter().copied().fold(f32::INFINITY, f32::min);
    let max = data.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    close(
        f64::from(min),
        output["min"].as_f64().unwrap_or(0.0),
        1e-5,
        "min",
        &case.case,
    );
    close(
        f64::from(max),
        output["max"].as_f64().unwrap_or(0.0),
        1e-5,
        "max",
        &case.case,
    );
    let sum: f64 = data.iter().map(|value| f64::from(*value)).sum();
    let mean = sum / data.len() as f64;
    close(
        mean,
        output["mean"].as_f64().unwrap_or(0.0),
        1e-5,
        "mean",
        &case.case,
    );
    close(
        sum,
        output["sum"].as_f64().unwrap_or(0.0),
        1e-5,
        "sum",
        &case.case,
    );

    let expected_rows = output["rows"].as_array().expect("rows");
    assert_eq!(expected_rows.len(), rows);
    for (row, expected) in expected_rows.iter().enumerate() {
        let values = &data[row * columns..(row + 1) * columns];
        let row_min = values.iter().copied().fold(f32::INFINITY, f32::min);
        let row_max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let row_sum: f64 = values.iter().map(|value| f64::from(*value)).sum();
        let argmax = values
            .iter()
            .enumerate()
            .fold((0, f32::NEG_INFINITY), |(best, value), (index, item)| {
                if *item > value {
                    (index, *item)
                } else {
                    (best, value)
                }
            })
            .0;
        close(
            f64::from(row_min),
            expected["min"].as_f64().unwrap_or(0.0),
            1e-5,
            "row.min",
            &case.case,
        );
        close(
            f64::from(row_max),
            expected["max"].as_f64().unwrap_or(0.0),
            1e-5,
            "row.max",
            &case.case,
        );
        close(
            row_sum / columns as f64,
            expected["mean"].as_f64().unwrap_or(0.0),
            1e-5,
            "row.mean",
            &case.case,
        );
        assert_eq!(
            argmax as u64,
            expected["argmax"].as_u64().unwrap_or(0),
            "argmax de {}",
            case.case
        );
    }

    let frame_indices: Vec<usize> = output["frame_indices"]
        .as_array()
        .expect("frame_indices")
        .iter()
        .map(|value| value.as_u64().unwrap_or(0) as usize)
        .collect();
    let frames = output["frames"].as_object().expect("frames");
    for index in frame_indices {
        let expected: Vec<f64> = frames
            .get(&index.to_string())
            .and_then(|value| value.as_array())
            .expect("columna de fotogramas")
            .iter()
            .map(|value| value.as_f64().unwrap_or(0.0))
            .collect();
        let produced: Vec<f64> = (0..rows)
            .map(|row| f64::from(data[row * columns + index]))
            .collect();
        compare_floats(&produced, &expected, "frame", &case.case);
    }
}

fn check_probability_input(case: &FixtureCase) {
    let relative = case.input["wav"].as_str().expect("wav");
    let pcm = wav_pcm(relative);
    assert_eq!(
        pcm.len() / 2,
        case.input["samples"].as_u64().unwrap_or(0) as usize,
        "muestras de {}",
        case.case
    );
    assert_eq!(
        case.input["rate"].as_u64().unwrap_or(0) as u32,
        SAMPLE_RATE,
        "tasa de {}",
        case.case
    );
    assert!(case.input["threshold"].as_f64().is_some());
}

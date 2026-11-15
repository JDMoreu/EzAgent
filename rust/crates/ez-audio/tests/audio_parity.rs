//! Paridad del dominio `audio` contra `parity/fixtures/audio.jsonl`.
//!
//! Cubre `pcm_rms`, `pcm_is_weak`, `pcm_to_wav`, el remuestreo lineal de
//! `turn.prepare`, `BargeInDetector`, `Segmenter` y la descripción de los WAV de
//! `parity/golden/audio/`. El reloj monótono del segmentador se congela a pasos
//! de `CHUNK_SECS` para que `last_speech_end` sea comparable, igual que el
//! oráculo.
//!
//! DECISION: `ez-audio` no declara `serde_json`, así que la prueba no construye
//! valores JSON: extrae los campos del fixture y los compara con tipos locales.

use std::path::Path;

use ez_audio::barge_in::{BargeInDetector, BARGE_IN_SECS};
use ez_audio::capture::{pcm_is_weak, pcm_rms, pcm_to_wav_checked};
use ez_audio::resample::{resample_linear, resampled_len};
use ez_parity::fixtures::{self, FixtureCase};

const FIXTURES: &str = "../../../parity/fixtures/audio.jsonl";
pub(crate) const ROOT: &str = "../../..";
const TOTAL_CASES: usize = 265;

#[path = "audio_parity/segmenter.rs"]
mod segmenter;

use segmenter::compare_segmenter;

#[test]
fn every_audio_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de audio legibles");
    let mut cases_run = 0;
    for case in &cases {
        match case.function.as_str() {
            "wav" => compare_wav(case),
            "pcm_rms" => compare_pcm_rms(case),
            "pcm_is_weak" => compare_pcm_is_weak(case),
            "pcm_to_wav" => compare_pcm_to_wav(case),
            "resample_linear" => compare_resample(case),
            "resample_length" => compare_resample_length(case),
            "barge_in" => compare_barge_in(case),
            "barge_in_need" => compare_barge_in_need(case),
            "segmenter" => compare_segmenter(case),
            other => panic!("caso sin cubrir: {} ({other})", case.case),
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert!(
        cases_in_file >= TOTAL_CASES,
        "se esperaban al menos {TOTAL_CASES} casos, hay {cases_in_file}"
    );
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
}

// ------------------------------------------------------------------- WAV

pub(crate) fn read_wav(path: &Path) -> Result<(Vec<u8>, u16, u32), String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(format!("{}: no es un WAV RIFF", path.display()));
    }
    let mut offset = 12;
    let (mut channels, mut rate) = (1_u16, 0_u32);
    let mut data = Vec::new();
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| "cabecera corta")?,
        ) as usize;
        let body = offset + 8;
        match id {
            b"fmt " if size >= 16 => {
                channels = u16::from_le_bytes([bytes[body + 2], bytes[body + 3]]);
                rate = u32::from_le_bytes(
                    bytes[body + 4..body + 8]
                        .try_into()
                        .map_err(|_| "tasa corta")?,
                );
            }
            b"data" => data.extend_from_slice(&bytes[body..(body + size).min(bytes.len())]),
            _ => {}
        }
        offset = body + size + (size % 2);
    }
    Ok((data, channels, rate))
}

fn compare_wav(case: &FixtureCase) {
    let relative = case.input["path"].as_str().expect("path");
    let path = Path::new(ROOT).join(relative);
    let bytes = std::fs::read(&path).expect("WAV del caso");
    let (pcm, channels, rate) = read_wav(&path).expect("WAV legible");
    let output = &case.output;
    assert_eq!(
        hex(&sha256(&bytes)),
        output["sha256"].as_str().expect("sha256"),
        "sha256 de {}",
        case.case
    );
    assert_eq!(
        bytes.len() as u64,
        output["bytes"].as_u64().expect("bytes"),
        "bytes de {}",
        case.case
    );
    assert_eq!(
        (pcm.len() / 2) as u64,
        output["samples"].as_u64().expect("samples"),
        "muestras de {}",
        case.case
    );
    assert_eq!(u64::from(rate), output["rate"].as_u64().expect("rate"));
    assert_eq!(
        u64::from(channels),
        output["channels"].as_u64().expect("channels")
    );
    let duration = pcm.len() as f64 / 2.0 / f64::from(rate);
    close(
        duration,
        output["duration_secs"].as_f64().expect("duration"),
        1e-9,
        "duration_secs",
        &case.case,
    );
    let peak = pcm
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| (f64::from(i16::from_le_bytes(*pair)) / 32768.0).abs())
        .fold(0.0_f64, f64::max);
    close(
        peak,
        output["peak"].as_f64().expect("peak"),
        1e-12,
        "peak",
        &case.case,
    );
    close(
        pcm_rms(&pcm),
        output["rms"].as_f64().expect("rms"),
        1e-5,
        "rms",
        &case.case,
    );
}

// -------------------------------------------------------------- PCM

pub(crate) fn pcm_from_spec(case: &FixtureCase) -> Vec<u8> {
    let spec = &case.input["pcm"];
    match spec["kind"].as_str().expect("kind del pcm") {
        "hex" => unhex(spec["hex"].as_str().unwrap_or("")),
        "pattern" => {
            let size = spec["bytes"].as_u64().unwrap_or(0) as usize;
            let seed = spec["seed"].as_u64().unwrap_or(0) as usize;
            (0..size)
                .map(|index| ((seed + index) % 256) as u8)
                .collect()
        }
        "wav" => {
            let relative = spec["wav"].as_str().expect("wav del pcm");
            let (pcm, _, _) = read_wav(&Path::new(ROOT).join(relative)).expect("WAV del pcm");
            let start = (spec["start"].as_u64().unwrap_or(0) as usize).min(pcm.len());
            let length = spec["length"].as_u64().map(|value| value as usize);
            let end = length
                .map_or(pcm.len(), |length| start.saturating_add(length))
                .min(pcm.len());
            pcm[start..end].to_vec()
        }
        other => panic!("pcm desconocido: {other}"),
    }
}

fn compare_pcm_rms(case: &FixtureCase) {
    let pcm = pcm_from_spec(case);
    close(
        pcm_rms(&pcm),
        case.output["rms"].as_f64().expect("rms"),
        1e-5,
        "rms",
        &case.case,
    );
}

fn compare_pcm_is_weak(case: &FixtureCase) {
    let pcm = pcm_from_spec(case);
    let rate = case.input["rate"].as_i64().unwrap_or(0);
    // DECISION: una tasa negativa da el mismo `frames` que 0 en Python
    // (`max(1, int(rate * CHUNK_SECS))`), y `u32` no la representa.
    assert_eq!(
        pcm_is_weak(&pcm, rate.max(0) as u32),
        case.output["weak"].as_bool().expect("weak"),
        "caso {}",
        case.case
    );
}

fn compare_pcm_to_wav(case: &FixtureCase) {
    let pcm = pcm_from_spec(case);
    let rate = case.input["rate"].as_i64().unwrap_or(0);
    let Some(expected) = case.error.as_deref() else {
        let wav = pcm_to_wav_checked(&pcm, rate).expect("tasa válida");
        let header = &case.output["header"];
        assert_eq!(
            wav.len() as u64,
            case.output["bytes"].as_u64().expect("bytes"),
            "bytes de {}",
            case.case
        );
        assert_eq!(
            hex(&sha256(&wav)),
            case.output["sha256"].as_str().expect("sha256"),
            "sha256 de {}",
            case.case
        );
        assert_eq!(
            u16::from_le_bytes([wav[22], wav[23]]),
            header["channels"].as_u64().unwrap_or(0) as u16
        );
        assert_eq!(
            u32::from_le_bytes(wav[24..28].try_into().expect("tasa")),
            header["rate"].as_u64().unwrap_or(0) as u32
        );
        assert_eq!(
            (wav.len() - 44) / 2,
            header["frames"].as_u64().unwrap_or(0) as usize
        );
        return;
    };
    let error = pcm_to_wav_checked(&pcm, rate).expect_err("el fixture espera el error");
    assert_eq!(error, expected, "caso {}", case.case);
}

// --------------------------------------------------------------- remuestreo

fn resample_samples(case: &FixtureCase) -> Vec<f32> {
    let spec = &case.input["samples"];
    if let Some(values) = spec.get("samples_i16").and_then(|values| values.as_array()) {
        return values
            .iter()
            .map(|value| value.as_i64().unwrap_or(0) as i16 as f32 / 32768.0)
            .collect();
    }
    let count = spec["count"].as_u64().unwrap_or(0) as usize;
    let start = spec["start"].as_i64().unwrap_or(-12000);
    let step = spec["step"].as_i64().unwrap_or(37);
    (0..count)
        .map(|index| {
            let value = start + step * index as i64;
            (((value + 30000).rem_euclid(60000)) - 30000) as i16 as f32 / 32768.0
        })
        .collect()
}

fn compare_resample(case: &FixtureCase) {
    let resampled = resample_linear(
        &resample_samples(case),
        case.input["from_rate"].as_i64().unwrap_or(0) as i32,
        case.input["to_rate"].as_i64().unwrap_or(0) as i32,
    );
    let output = &case.output;
    if let Some(expected) = case.error.as_deref() {
        assert_eq!(
            resampled.expect_err("el fixture espera division by zero"),
            expected,
            "caso {}",
            case.case
        );
        return;
    }
    let resampled = resampled.expect("tasa válida");
    assert_eq!(
        resampled.len() as u64,
        output["length"].as_u64().expect("length")
    );
    let head: Vec<f64> = output["head"]
        .as_array()
        .expect("head")
        .iter()
        .map(|value| value.as_f64().unwrap_or(0.0))
        .collect();
    compare_list(
        &resampled[..head.len().min(resampled.len())],
        &head,
        "head",
        &case.case,
    );
    let tail_start = resampled.len().saturating_sub(5);
    let tail: Vec<f64> = output["tail"]
        .as_array()
        .expect("tail")
        .iter()
        .map(|value| value.as_f64().unwrap_or(0.0))
        .collect();
    compare_list(&resampled[tail_start..], &tail, "tail", &case.case);
    close(
        resampled.iter().sum(),
        output["sum"].as_f64().expect("sum"),
        1e-9,
        "sum",
        &case.case,
    );
    close(
        resampled.iter().map(|value| value * value).sum(),
        output["sum_squares"].as_f64().expect("sum_squares"),
        1e-9,
        "sum_squares",
        &case.case,
    );
}

fn compare_resample_length(case: &FixtureCase) {
    let produced = resampled_len(
        case.input["length"].as_u64().unwrap_or(0) as usize,
        case.input["from_rate"].as_i64().unwrap_or(0) as i32,
        case.input["to_rate"].as_i64().unwrap_or(0) as i32,
    );
    if let Some(expected) = case.error.as_deref() {
        assert_eq!(
            produced.expect_err("el fixture espera division by zero"),
            expected,
            "caso {}",
            case.case
        );
        return;
    }
    assert_eq!(
        produced.expect("tasa válida") as u64,
        case.output["length"].as_u64().expect("length"),
        "caso {}",
        case.case
    );
}

// ---------------------------------------------------------------- barge-in

fn barge_chunk(index: usize) -> Vec<u8> {
    vec![
        (index % 256) as u8,
        ((index * 7) % 256) as u8,
        ((index * 13) % 256) as u8,
        ((index * 29) % 256) as u8,
    ]
}

fn compare_barge_in(case: &FixtureCase) {
    let threshold = case.input["threshold"].as_f64().unwrap_or(0.0);
    let secs = case.input["secs"].as_f64().unwrap_or(BARGE_IN_SECS);
    let levels: Vec<f64> = case.input["levels"]
        .as_array()
        .expect("levels")
        .iter()
        .map(|value| value.as_f64().unwrap_or(0.0))
        .collect();
    let mut detector = BargeInDetector::new(threshold, secs);
    let expected_runs = case.output["runs"].as_array().expect("runs");
    let mut runs_seen = 0;
    for (index, level) in levels.iter().enumerate() {
        let Some(run) = detector.feed(&barge_chunk(index), *level) else {
            continue;
        };
        let expected = &expected_runs[runs_seen];
        runs_seen += 1;
        let first = index + 1 - run.len();
        assert_eq!(expected["at"].as_u64().unwrap_or(0) as usize, index);
        let chunks: Vec<u64> = expected["chunks"]
            .as_array()
            .expect("chunks")
            .iter()
            .map(|value| value.as_u64().unwrap_or(0))
            .collect();
        assert_eq!(
            chunks,
            (first..=index)
                .map(|value| value as u64)
                .collect::<Vec<_>>()
        );
        let expected_run = expected["run"].as_array().expect("run");
        assert_eq!(expected_run.len(), run.len());
        for (step, (chunk, level)) in run.iter().enumerate() {
            assert_eq!(
                expected_run[step][0].as_u64().unwrap_or(0),
                (first + step) as u64
            );
            close(
                *level,
                expected_run[step][1].as_f64().unwrap_or(0.0),
                1e-12,
                "level",
                &case.case,
            );
            assert_eq!(chunk, &barge_chunk(first + step));
        }
        let payload: Vec<u8> = run.iter().flat_map(|(chunk, _)| chunk.clone()).collect();
        assert_eq!(
            payload.len() as u64,
            expected["bytes"].as_u64().unwrap_or(0),
            "bytes de {}",
            case.case
        );
        assert_eq!(
            hex(&sha256(&payload)),
            expected["sha256"].as_str().unwrap_or(""),
            "sha256 de {}",
            case.case
        );
    }
    assert_eq!(runs_seen, expected_runs.len(), "runs de {}", case.case);
    assert_eq!(
        BargeInDetector::new(threshold, secs).need() as u64,
        case.output["need"].as_u64().expect("need"),
        "need de {}",
        case.case
    );
}

fn compare_barge_in_need(case: &FixtureCase) {
    let secs = case.input["secs"].as_f64().unwrap_or(BARGE_IN_SECS);
    assert_eq!(
        BargeInDetector::new(0.008, secs).need() as u64,
        case.output["need"].as_u64().expect("need"),
        "caso {}",
        case.case
    );
}

// --------------------------------------------------------------- utilidades

fn close(produced: f64, expected: f64, rel: f64, field: &str, case: &str) {
    let tolerance = rel * expected.abs().max(1.0);
    assert!(
        (produced - expected).abs() <= tolerance,
        "{case}: {field} {produced} != {expected}"
    );
}

fn compare_list(produced: &[f64], expected: &[f64], field: &str, case: &str) {
    assert_eq!(produced.len(), expected.len(), "{case}: largo de {field}");
    for (index, value) in produced.iter().enumerate() {
        close(*value, expected[index], 1e-9, field, case);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks(2)
        .filter(|pair| pair.len() == 2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).unwrap_or(0);
            let low = (pair[1] as char).to_digit(16).unwrap_or(0);
            (high * 16 + low) as u8
        })
        .collect()
}

fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut words = [0_u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            *word =
                u32::from_be_bytes(block[index * 4..index * 4 + 4].try_into().expect("palabra"));
        }
        for index in 16..64 {
            let a = words[index - 15];
            let b = words[index - 2];
            let s0 = a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3);
            let s1 = b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let mut v = state;
        for index in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let choice = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let temp1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let majority = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let temp2 = s0.wrapping_add(majority);
            v[7] = v[6];
            v[6] = v[5];
            v[5] = v[4];
            v[4] = v[3].wrapping_add(temp1);
            v[3] = v[2];
            v[2] = v[1];
            v[1] = v[0];
            v[0] = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip(v) {
            *slot = slot.wrapping_add(value);
        }
    }
    let digest: [u8; 32] = state
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect::<Vec<u8>>()
        .try_into()
        .expect("32 bytes");
    digest
}

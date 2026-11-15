//! Utilidades compartidas de `tts_parity`: JSON, npy, sha256 y estilos.

use std::path::{Path, PathBuf};

use ez_parity::fixtures::FixtureCase;
use ez_tts::kokoro::sliding;

macro_rules! input_text {
    ($case:expr, $key:expr) => {
        $case.input[$key].as_str().unwrap_or("").to_string()
    };
}

macro_rules! ints_of {
    ($value:expr) => {
        $value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.as_i64().unwrap_or(0))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
}

macro_rules! strings_of {
    ($value:expr) => {
        $value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.as_str().unwrap_or("").to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
}

macro_rules! timings_of {
    ($value:expr) => {
        $value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| sliding::Timing {
                        phoneme: item["phoneme"].as_str().unwrap_or("").to_string(),
                        start: item["start"].as_f64().unwrap_or(0.0),
                        end: item["end"].as_f64().unwrap_or(0.0),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
}

macro_rules! vocab_of {
    ($value:expr) => {{
        let value = $value;
        if value.is_null() {
            None
        } else {
            value.as_object().map(|entries| {
                entries
                    .iter()
                    .filter_map(|(key, id)| {
                        let mut chars = key.chars();
                        match (chars.next(), chars.next()) {
                            (Some(character), None) => id.as_i64().map(|id| (character, id)),
                            _ => None,
                        }
                    })
                    .collect::<HashMap<char, i64>>()
            })
        }
    }};
}

macro_rules! vocab_list_of {
    ($value:expr) => {
        $value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| {
                        (
                            item[0].as_str().unwrap_or("").to_string(),
                            item[1].as_i64().unwrap_or(0),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
}

macro_rules! compare_pcm {
    ($audio:expr, $expected:expr, $case:expr) => {{
        let audio = $audio;
        let expected = $expected;
        let golden_audio = crate::support::load_npy(&crate::support::golden(
            expected["file"].as_str().expect("file"),
        ));
        assert_eq!(
            audio.len() as u64,
            expected["samples"].as_u64().unwrap_or(0),
            "samples de {}",
            $case
        );
        assert_eq!(audio.len(), golden_audio.len(), "largo de {}", $case);
        let amplitude = audio
            .iter()
            .fold(0.0_f64, |peak, value| peak.max(f64::from(value.abs())));
        let reference = golden_audio
            .iter()
            .fold(0.0_f64, |peak, value| peak.max(f64::from(value.abs())));
        if amplitude == 0.0 && reference == 0.0 {
            return;
        }
        let correlation = crate::support::correlation(&audio, &golden_audio);
        assert!(
            correlation >= 0.999,
            "correlación de {}: {correlation}",
            $case
        );
        let rms = (audio
            .iter()
            .map(|value| f64::from(*value).powi(2))
            .sum::<f64>()
            / audio.len() as f64)
            .sqrt();
        crate::support::close(
            rms,
            expected["rms"].as_f64().unwrap_or(0.0),
            1e-4,
            "rms",
            &$case,
        );
        crate::support::close(
            amplitude,
            expected["peak"].as_f64().unwrap_or(0.0),
            1e-4,
            "peak",
            &$case,
        );
    }};
}

pub(crate) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

pub(crate) fn golden(relative: &str) -> PathBuf {
    root().join("parity").join("golden").join(relative)
}

pub(crate) fn output_text(case: &FixtureCase, key: &str) -> String {
    let mut current = &case.output;
    for part in key.split('.') {
        current = &current[part];
    }
    current.as_str().unwrap_or("").to_string()
}
pub(crate) fn input_path(case: &FixtureCase, key: &str) -> String {
    let mut current = &case.input;
    for part in key.split('.') {
        current = &current[part];
    }
    current.as_str().unwrap_or("").to_string()
}
pub(crate) const SENTENCE_PAUSE: f64 = 0.30;
pub(crate) const CLAUSE_PAUSE: f64 = 0.12;

pub(crate) fn number(case: &FixtureCase, key: &str) -> f64 {
    case.input[key].as_f64().unwrap_or(match key {
        "sentence" => SENTENCE_PAUSE,
        "clause" => CLAUSE_PAUSE,
        _ => 0.0,
    })
}

// --------------------------------------------------------------- utilidades

pub(crate) fn load_npy(path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert_eq!(
        &bytes[..6],
        b"\x93NUMPY",
        "{}: no es un npy",
        path.display()
    );
    let version = bytes[6];
    let (header_len, start) = if version == 1 {
        (u16::from_le_bytes([bytes[8], bytes[9]]) as usize, 10)
    } else {
        (
            u32::from_le_bytes(bytes[8..12].try_into().expect("largo")) as usize,
            12,
        )
    };
    let header = std::str::from_utf8(&bytes[start..start + header_len]).expect("cabecera npy");
    let shape = &header[header.find("'shape'").expect("shape")..];
    let open = shape.find('(').expect("(");
    let close = shape.find(')').expect(")");
    let total: usize = shape[open + 1..close]
        .split(',')
        .filter_map(|part| part.trim().parse::<usize>().ok())
        .product();
    let data = start + header_len;
    bytes[data..data + total * 4]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect()
}

pub(crate) fn correlation(left: &[f32], right: &[f32]) -> f64 {
    let count = left.len() as f64;
    let left_mean = left.iter().map(|value| f64::from(*value)).sum::<f64>() / count;
    let right_mean = right.iter().map(|value| f64::from(*value)).sum::<f64>() / count;
    let mut covariance = 0.0;
    let mut left_var = 0.0;
    let mut right_var = 0.0;
    for (left, right) in left.iter().zip(right) {
        let left = f64::from(*left) - left_mean;
        let right = f64::from(*right) - right_mean;
        covariance += left * right;
        left_var += left * left;
        right_var += right * right;
    }
    if left_var == 0.0 || right_var == 0.0 {
        return if left == right { 1.0 } else { 0.0 };
    }
    covariance / (left_var * right_var).sqrt()
}

pub(crate) fn write_case_files(temp: &Path) {
    let _ = std::fs::write(temp.join("model.onnx"), b"");
    let _ = std::fs::write(temp.join("voices.bin"), b"");
    let _ = std::fs::write(temp.join("table.json"), "{\"vocab\": {\"a\": 1, \"ñ\": 7}}");
    let _ = std::fs::write(temp.join("no-vocab.json"), "{\"other\": {\"a\": 1}}");
    let _ = std::fs::write(temp.join("broken.json"), "{ no json");
}

pub(crate) fn f32_bytes(data: &[f32]) -> Vec<u8> {
    data.iter().flat_map(|value| value.to_le_bytes()).collect()
}

pub(crate) fn close(produced: f64, expected: f64, rel: f64, field: &str, case: &str) {
    let tolerance = rel * expected.abs().max(1.0);
    assert!(
        (produced - expected).abs() <= tolerance,
        "{case}: {field} {produced} != {expected}"
    );
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
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
    let mut digest = [0_u8; 32];
    for (index, value) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    digest
}

pub(crate) fn compare_timings(case: &FixtureCase, produced: &[sliding::Timing], key: &str) {
    let expected = case.output[key].as_array().expect("timings");
    assert_eq!(produced.len(), expected.len(), "timings de {}", case.case);
    for (index, timing) in produced.iter().enumerate() {
        assert_eq!(
            timing.phoneme,
            expected[index]["phoneme"].as_str().unwrap_or("")
        );
        close(
            timing.start,
            expected[index]["start"].as_f64().unwrap_or(0.0),
            1e-9,
            "start",
            &case.case,
        );
        close(
            timing.end,
            expected[index]["end"].as_f64().unwrap_or(0.0),
            1e-9,
            "end",
            &case.case,
        );
    }
}

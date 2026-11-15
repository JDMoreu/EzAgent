//! Casos de pausas, recorte y ventana deslizante.

use std::collections::HashSet;

use ez_parity::fixtures::FixtureCase;
use ez_tts::kokoro::{pauses, sliding, trim};

use crate::support::{compare_timings, golden, input_path, load_npy, number};

// --------------------------------------------------------- pausas y recorte

pub(crate) fn compare_insert(case: &FixtureCase) {
    let audio = load_npy(&golden(&input_path(case, "audio.file")));
    let timings = timings_of!(case.input["timings"]);
    let produced = pauses::insert(
        &audio,
        &timings,
        case.input["sample_rate"].as_u64().unwrap_or(0) as u32,
        number(case, "sentence"),
        number(case, "clause"),
    );
    match case.error.as_deref() {
        Some(expected) => assert_eq!(
            produced.expect_err("error esperado"),
            expected,
            "caso {}",
            case.case
        ),
        None => {
            let (audio, moved) = produced.expect("audio insertado");
            compare_timings(case, &moved, "timings");
            compare_pcm!(audio, &case.output["audio"], case.case);
        }
    }
}

pub(crate) fn compare_trim(case: &FixtureCase) {
    let audio = load_npy(&golden(&input_path(case, "audio.file")));
    let (trimmed, (start, end)) = trim::trim(&audio);
    assert_eq!(
        start as u64,
        case.output["start"].as_u64().unwrap_or(0),
        "start de {}",
        case.case
    );
    assert_eq!(
        end as u64,
        case.output["end"].as_u64().unwrap_or(0),
        "end de {}",
        case.case
    );
    compare_pcm!(trimmed, &case.output["audio"], case.case);
}

// ----------------------------------------------------------- deslizamiento

pub(crate) fn compare_synthesize(case: &FixtureCase) {
    let run = RunSpec {
        samples_per_token: case.input["run"]["samples_per_token"]
            .as_u64()
            .unwrap_or(240) as usize,
        frames_per_token: case.input["run"]["frames_per_token"].as_i64().unwrap_or(2),
        gap_samples: case.input["run"]["gap_samples"].as_u64().unwrap_or(20) as usize,
        amplitude: case.input["run"]["amplitude"].as_f64().unwrap_or(0.05),
        amplitude_step: case.input["run"]["amplitude_step"].as_f64().unwrap_or(0.01),
        amplitude_mod: case.input["run"]["amplitude_mod"].as_i64().unwrap_or(7),
        base_hz: case.input["run"]["base_hz"].as_f64().unwrap_or(120.0),
        hz_step: case.input["run"]["hz_step"].as_f64().unwrap_or(30.0),
        hz_mod: case.input["run"]["hz_mod"].as_i64().unwrap_or(5),
    };
    let options = sliding::SynthOptions {
        keep: case.input["keep"].as_u64().unwrap_or(300) as usize,
        context: case.input["context"].as_u64().unwrap_or(60) as usize,
        lookahead: case.input["lookahead"].as_u64().unwrap_or(60) as usize,
        crossfade: case.input["crossfade"].as_f64().unwrap_or(0.025),
    };
    let stops: HashSet<i64> = ints_of!(case.input["stops"]).into_iter().collect();
    let spaces: HashSet<i64> = ints_of!(case.input["spaces"]).into_iter().collect();
    let produced = sliding::synthesize(
        |window| Ok(fake_run(window, &run)),
        &ints_of!(case.input["tokens"]),
        case.input["sample_rate"].as_u64().unwrap_or(0) as u32,
        &stops,
        &spaces,
        &options,
    );
    let expected = case
        .error
        .as_deref()
        .expect("todos los casos fallan como el Python");
    assert_eq!(
        produced.expect_err("error esperado"),
        expected,
        "caso {}",
        case.case
    );
}

pub(crate) struct RunSpec {
    samples_per_token: usize,
    frames_per_token: i64,
    gap_samples: usize,
    amplitude: f64,
    amplitude_step: f64,
    amplitude_mod: i64,
    base_hz: f64,
    hz_step: f64,
    hz_mod: i64,
}

pub(crate) fn fake_run(window: &[i64], spec: &RunSpec) -> (Vec<f32>, Vec<i64>) {
    let mut audio = vec![0.0_f32; spec.samples_per_token * window.len()];
    for (position, token) in window.iter().enumerate() {
        let amplitude =
            spec.amplitude + spec.amplitude_step * (*token).rem_euclid(spec.amplitude_mod) as f64;
        let hz = spec.base_hz + (*token).rem_euclid(spec.hz_mod) as f64 * spec.hz_step;
        let start = position * spec.samples_per_token;
        for index in 0..spec.samples_per_token {
            let sample = amplitude
                * (2.0 * std::f64::consts::PI * hz * index as f64 / spec.samples_per_token as f64)
                    .sin();
            audio[start + index] = sample as f32;
        }
        for index in 0..spec.gap_samples {
            audio[start + index] = 0.0;
            audio[start + spec.samples_per_token - 1 - index] = 0.0;
        }
    }
    (audio, vec![spec.frames_per_token; window.len()])
}

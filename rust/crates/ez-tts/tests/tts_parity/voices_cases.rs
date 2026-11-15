//! Casos de los estilos de voz de `voices-v1.0.bin`.

use ez_parity::fixtures::FixtureCase;
use ez_tts::kokoro::style_for;
use ez_tts::voices::Voices;

use crate::support::{close, f32_bytes, hex, sha256};

// ------------------------------------------------------------------- voces

pub(crate) fn compare_style_result(case: &FixtureCase, produced: Result<Vec<f32>, String>) {
    match case.error.as_deref() {
        Some(expected) => assert_eq!(
            produced.expect_err("error esperado"),
            expected,
            "caso {}",
            case.case
        ),
        None => {
            let data = produced.expect("estilo");
            let expected = &case.output["style"];
            let total: u64 = expected["shape"]
                .as_array()
                .expect("shape")
                .iter()
                .map(|dimension| dimension.as_u64().unwrap_or(0))
                .product();
            assert_eq!(data.len() as u64, total, "forma de {}", case.case);
            assert_eq!(
                hex(&sha256(&f32_bytes(&data))),
                expected["sha256"].as_str().unwrap_or(""),
                "sha256 de {}",
                case.case
            );
            for (index, value) in expected["first"]
                .as_array()
                .expect("first")
                .iter()
                .enumerate()
            {
                close(
                    f64::from(data[index]),
                    value.as_f64().unwrap_or(0.0),
                    1e-7,
                    "first",
                    &case.case,
                );
            }
        }
    }
}

pub(crate) fn compare_style_for(case: &FixtureCase, voices: &Voices) {
    let style = voices.get("em_alex").expect("voz de referencia");
    let length = case.input["length"].as_u64().unwrap_or(0) as usize;
    let row = style_for(style, length);
    let index = length.min(style.rows()) as i64 - 1;
    assert_eq!(
        index,
        case.output["row"]["index"].as_i64().unwrap_or(0),
        "index de {}",
        case.case
    );
    assert_eq!(
        hex(&sha256(&f32_bytes(row))),
        case.output["row"]["sha256"].as_str().unwrap_or(""),
        "sha256 de {}",
        case.case
    );
}

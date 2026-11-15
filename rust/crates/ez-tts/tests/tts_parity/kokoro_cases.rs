//! Casos que necesitan ONNX Runtime y espeak-ng.

use std::collections::HashMap;
use std::path::Path;

use ez_core::speech_text::normalize_for_kokoro;
use ez_parity::fixtures::FixtureCase;
use ez_tts::kokoro::session::{self, Model};
use ez_tts::kokoro::{style_for, Kokoro, SynthesisOptions, Tokenizer, Voice, MAX_PHONEME_LENGTH};
use ez_tts::voices::{Style, Voices};

use crate::support::{
    close, compare_timings, f32_bytes, hex, number, output_text, sha256, CLAUSE_PAUSE,
    SENTENCE_PAUSE,
};

// --------------------------------------------------------------- ONNX/Kokoro

pub(crate) fn compare_input_dtypes(case: &FixtureCase, model: &dyn Model) {
    let mut produced = HashMap::new();
    for (name, dtype) in model.input_dtypes() {
        let text = match dtype {
            session::DType::Int64 => "int64",
            session::DType::Int32 => "int32",
            session::DType::Float64 => "float64",
            session::DType::Float16 => "float16",
            session::DType::Float32 => "float32",
        };
        produced.insert(name.clone(), text.to_string());
    }
    let expected = case.output["types"].as_object().expect("types");
    assert_eq!(produced.len(), expected.len(), "caso {}", case.case);
    for (name, dtype) in expected {
        assert_eq!(
            produced.get(name).map(String::as_str),
            dtype.as_str(),
            "caso {}",
            case.case
        );
    }
}

pub(crate) fn compare_artifacts(case: &FixtureCase, model: &Path, voices: &Path, kokoro: &Kokoro) {
    let model_bytes = std::fs::read(model).expect("modelo");
    let voices_bytes = std::fs::read(voices).expect("voces");
    let expected = &case.output;
    assert_eq!(
        hex(&sha256(&model_bytes)),
        expected["model"]["sha256"].as_str().unwrap_or(""),
        "sha256 del modelo"
    );
    assert_eq!(
        model_bytes.len() as u64,
        expected["model"]["bytes"].as_u64().unwrap_or(0)
    );
    assert_eq!(
        model.file_name().and_then(|name| name.to_str()),
        expected["model"]["name"].as_str()
    );
    assert_eq!(
        hex(&sha256(&voices_bytes)),
        expected["voices"]["sha256"].as_str().unwrap_or(""),
        "sha256 de las voces"
    );
    assert_eq!(
        kokoro.tokenizer().vocab().len() as u64,
        expected["vocab_size"].as_u64().unwrap_or(0)
    );
    assert_eq!(
        kokoro.has_timings(),
        expected["has_timings"].as_bool().unwrap_or(true)
    );
    if let Some(library) = ez_tts::phonemes::default_library_path() {
        let bytes = std::fs::read(&library).unwrap_or_default();
        assert_eq!(
            library.file_name().and_then(|name| name.to_str()),
            expected["espeak"]["library"].as_str()
        );
        assert_eq!(
            hex(&sha256(&bytes)),
            expected["espeak"]["sha256"].as_str().unwrap_or("")
        );
    }
}

pub(crate) fn options_of(case: &FixtureCase) -> SynthesisOptions<'_> {
    SynthesisOptions {
        speed: case.input["speed"].as_f64().unwrap_or(1.0),
        lang: case.input["lang"].as_str().unwrap_or("es"),
        is_phonemes: case.input["is_phonemes"].as_bool().unwrap_or(false),
        trim: case.input["trim"].as_bool().unwrap_or(true),
        sentence_pause: number(case, "sentence_pause"),
        clause_pause: number(case, "clause_pause"),
        continuous: false,
    }
}

pub(crate) fn compare_create(case: &FixtureCase, kokoro: &mut Kokoro, voices: &Voices) {
    if case.input.get("continuous").is_some() {
        let style = voices.voice_style("em_alex").expect("estilo");
        let options = SynthesisOptions {
            continuous: true,
            ..SynthesisOptions::default()
        };
        let error = kokoro
            .create_timed("Una frase cualquiera.", Voice::Style(&style), &options)
            .expect_err("continuous sin duraciones");
        assert_eq!(
            Some(error.0.as_str()),
            case.error.as_deref(),
            "caso {}",
            case.case
        );
        return;
    }
    if let Some(name) = case.input.get("name").and_then(|name| name.as_str()) {
        compare_extra_create(case, kokoro, voices, name);
        return;
    }
    let is_phonemes = case.input["is_phonemes"].as_bool().unwrap_or(false);
    let normalized = if is_phonemes {
        input_text!(case, "text")
    } else {
        normalize_for_kokoro(&input_text!(case, "text"))
    };
    let options = options_of(case);
    let style = voices
        .voice_style(&input_text!(case, "voice"))
        .expect("estilo");
    if let Some(expected) = case.error.as_deref() {
        let error = kokoro
            .create_timed(&normalized, Voice::Style(&style), &options)
            .expect_err("el fixture espera un error");
        assert_eq!(error.0, expected, "caso {}", case.case);
        return;
    }
    let prepared = kokoro.prepare(&normalized, &options).expect("preparación");
    assert_eq!(
        prepared.phonemes,
        output_text(case, "phonemes"),
        "fonemas de {}",
        case.case
    );
    assert_eq!(
        normalized,
        output_text(case, "normalized"),
        "normalized de {}",
        case.case
    );
    assert_eq!(
        kokoro.has_timings(),
        case.output["has_timings"].as_bool().unwrap_or(true)
    );
    compare_batches(case, &prepared.batches, &style, kokoro.tokenizer());
    let (audio, rate, spoken) = kokoro
        .create_timed(&normalized, Voice::Style(&style), &options)
        .expect("síntesis");
    assert_eq!(
        u64::from(rate),
        case.output["audio"]["sample_rate"].as_u64().unwrap_or(0)
    );
    compare_timings(case, &spoken, "spoken");
    compare_pcm!(audio, &case.output["audio"], case.case);
}

pub(crate) fn compare_extra_create(
    case: &FixtureCase,
    kokoro: &mut Kokoro,
    voices: &Voices,
    name: &str,
) {
    if name == "speed_text" {
        // El Python compara una cadena con floats; no hay equivalente f64.
        assert_eq!(
            case.error.as_deref(),
            Some("'<=' not supported between instances of 'float' and 'str'"),
            "caso {}",
            case.case
        );
        return;
    }
    let style = voices.voice_style("em_alex").expect("estilo");
    let by_name = name == "voice_name" || name == "voice_missing";
    let voice_name = input_text!(case, "voice");
    let voice = if by_name {
        Voice::Name(&voice_name)
    } else {
        Voice::Style(&style)
    };
    let produced = kokoro.create_timed(&input_text!(case, "text"), voice, &options_of(case));
    match case.error.as_deref() {
        Some(expected) => assert_eq!(
            produced.expect_err("error esperado").0,
            expected,
            "caso {}",
            case.case
        ),
        None => {
            let (audio, _, spoken) = produced.expect("síntesis");
            compare_timings(case, &spoken, "spoken");
            compare_pcm!(audio, &case.output["audio"], case.case);
        }
    }
}

pub(crate) fn compare_batches(
    case: &FixtureCase,
    batches: &[(String, f64)],
    style: &Style,
    tokenizer: &Tokenizer,
) {
    let expected = case.output["batches"].as_array().expect("batches");
    assert_eq!(batches.len(), expected.len(), "batches de {}", case.case);
    for (index, (phonemes, pause)) in batches.iter().enumerate() {
        let row = &expected[index];
        assert_eq!(phonemes, row["phonemes"].as_str().unwrap_or(""));
        close(
            *pause,
            row["pause"].as_f64().unwrap_or(0.0),
            1e-9,
            "pause",
            &case.case,
        );
        let tokens = tokenizer
            .tokenize(phonemes, Some(MAX_PHONEME_LENGTH as i64))
            .expect("tokens");
        assert_eq!(tokens, ints_of!(row["tokens"]), "tokens de {}", case.case);
        assert_eq!(
            tokens.len() as u64,
            row["token_count"].as_u64().unwrap_or(0)
        );
        let style_row = style_for(style, tokens.len());
        let slot = &row["style"];
        assert_eq!(slot["shape"][0].as_u64().unwrap_or(0), 1);
        assert_eq!(
            style_row.len() as u64,
            slot["shape"][1].as_u64().unwrap_or(0)
        );
        assert_eq!(
            hex(&sha256(&f32_bytes(style_row))),
            slot["sha256"].as_str().unwrap_or(""),
            "sha256 del estilo de {}",
            case.case
        );
    }
}

pub(crate) fn compare_stream(case: &FixtureCase, kokoro: &mut Kokoro, voices: &Voices) {
    let style = voices.voice_style("em_alex").expect("estilo");
    if let Some(expected) = case.error.as_deref() {
        let options = SynthesisOptions {
            speed: 3.0,
            ..SynthesisOptions::default()
        };
        let error = kokoro
            .create_stream("Hola.", Voice::Style(&style), &options, |_, _| true)
            .expect_err("velocidad inválida");
        assert_eq!(error.0, expected, "caso {}", case.case);
        return;
    }
    let options = SynthesisOptions {
        speed: case.input["speed"].as_f64().unwrap_or(0.95),
        lang: "es",
        sentence_pause: SENTENCE_PAUSE,
        clause_pause: CLAUSE_PAUSE,
        ..SynthesisOptions::default()
    };
    let mut chunks: Vec<Vec<f32>> = Vec::new();
    kokoro
        .create_stream(
            &input_text!(case, "text"),
            Voice::Style(&style),
            &options,
            |audio, _| {
                chunks.push(audio.to_vec());
                true
            },
        )
        .expect("stream");
    let expected = case.output["chunks"].as_array().expect("chunks");
    assert_eq!(chunks.len(), expected.len(), "chunks de {}", case.case);
    let mut total = 0_usize;
    for (index, chunk) in chunks.iter().enumerate() {
        total += chunk.len();
        assert_eq!(
            chunk.len() as u64,
            expected[index]["samples"].as_u64().unwrap_or(0)
        );
        assert_eq!(
            hex(&sha256(&f32_bytes(chunk))),
            expected[index]["pcm_sha256"].as_str().unwrap_or(""),
            "pcm de {}",
            case.case
        );
    }
    assert_eq!(
        total as u64,
        case.output["total_samples"].as_u64().unwrap_or(0)
    );
}

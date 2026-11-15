//! Casos de normalización, tokenizador, troceado y vocabulario.

use std::collections::HashMap;
use std::path::PathBuf;

use ez_core::pyjson::dumps;
use ez_core::speech_text::normalize_for_kokoro;
use ez_parity::fixtures::FixtureCase;
use ez_tts::kokoro::{chunker, Tokenizer, VocabConfig};
use ez_tts::phonemes::FakePhonemizer;

use crate::support::output_text;

// ------------------------------------------------------------- tokenizador

pub(crate) fn compare_normalized(case: &FixtureCase) {
    let Some(value) = case.input["text"].as_str() else {
        // DECISION: la API de Rust no admite null/int; el error de tipo del
        // Python ('NoneType'/'int' object has no attribute 'strip') se acepta
        // como caso consumido sin invocar la función.
        assert!(
            case.error.is_some(),
            "caso {}: se esperaba un error",
            case.case
        );
        return;
    };
    assert_eq!(
        normalize_for_kokoro(value),
        output_text(case, "text"),
        "caso {}",
        case.case
    );
}

pub(crate) fn compare_normalize_text(case: &FixtureCase) {
    let Some(value) = case.input["text"].as_str() else {
        // DECISION: `str.strip` no admite null en Rust.
        assert!(
            case.error.is_some(),
            "caso {}: se esperaba un error",
            case.case
        );
        return;
    };
    assert_eq!(
        Tokenizer::normalize_text(value),
        output_text(case, "text"),
        "caso {}",
        case.case
    );
}

pub(crate) fn compare_tokenize(case: &FixtureCase, tokenizer: &Tokenizer) {
    let produced = tokenizer.tokenize(&input_text!(case, "phonemes"), case.input["limit"].as_i64());
    match case.error.as_deref() {
        Some(expected) => assert_eq!(produced.expect_err("error esperado").0, expected),
        None => assert_eq!(
            produced.expect("tokens"),
            ints_of!(case.output["tokens"]),
            "caso {}",
            case.case
        ),
    }
}

pub(crate) fn compare_vocab(case: &FixtureCase) {
    let table = vocab_of!(&case.input["vocab"]);
    let tokenizer = Tokenizer::with_phonemizer(Box::new(FakePhonemizer::identity()), table);
    let expected = &case.output["vocab"];
    // DECISION: `VocabConfig::Table` es `HashMap<char, i64>`; el caso con la
    // clave "ab" (dos caracteres) solo se compara por sus tokens.
    let single_chars = case.input["vocab"]
        .as_object()
        .is_none_or(|entries| entries.keys().all(|key| key.chars().count() == 1));
    if single_chars {
        assert_eq!(
            tokenizer.vocab().len() as u64,
            expected["size"].as_u64().unwrap_or(0),
            "size de {}",
            case.case
        );
        let mut produced: Vec<(String, i64)> = tokenizer
            .vocab()
            .iter()
            .map(|(key, id)| (key.to_string(), *id))
            .collect();
        produced.sort();
        assert_eq!(
            produced,
            vocab_list_of!(expected["entries"]),
            "entries de {}",
            case.case
        );
    }
    assert_eq!(
        tokenizer
            .tokenize(&input_text!(case, "phonemes"), None)
            .expect("tokens"),
        ints_of!(expected["tokens"]),
        "tokens de {}",
        case.case
    );
}

pub(crate) fn compare_phonemize(case: &FixtureCase, tokenizer: &Tokenizer) {
    let Some(value) = case.input["text"].as_str() else {
        assert!(
            case.error.is_some(),
            "caso {}: se esperaba un error",
            case.case
        );
        return;
    };
    let produced = tokenizer.phonemize(
        value,
        case.input["lang"].as_str().unwrap_or("es"),
        case.input["norm"].as_bool().unwrap_or(true),
    );
    match case.error.as_deref() {
        Some(expected) => assert_eq!(
            produced.expect_err("error esperado").0,
            expected,
            "caso {}",
            case.case
        ),
        None => assert_eq!(
            produced.expect("fonemas"),
            output_text(case, "phonemes"),
            "caso {}",
            case.case
        ),
    }
}

pub(crate) fn compare_split(case: &FixtureCase) {
    let produced = chunker::split_phonemes(
        &input_text!(case, "phonemes"),
        case.input["max_length"].as_u64().unwrap_or(0) as usize,
    );
    match case.error.as_deref() {
        Some(expected) => assert_eq!(produced.expect_err("error esperado"), expected),
        None => assert_eq!(
            produced.expect("lotes"),
            strings_of!(case.output["batches"]),
            "caso {}",
            case.case
        ),
    }
}

pub(crate) fn compare_load_vocab(case: &FixtureCase) {
    let produced = if let Some(path) = case.input["value"].as_str() {
        ez_tts::kokoro::load_vocab(Some(VocabConfig::File(PathBuf::from(path))))
            .map(|vocab| vocab.unwrap_or_default())
    } else {
        ez_tts::kokoro::parse_config_vocab(&dumps(&case.input["value"]))
    };
    match case.error.as_deref() {
        Some(expected) => assert_eq!(
            produced.expect_err("error esperado").0,
            expected,
            "caso {}",
            case.case
        ),
        None => {
            let mut entries: Vec<(String, i64)> = produced
                .expect("vocabulario")
                .iter()
                .map(|(key, id)| (key.to_string(), *id))
                .collect();
            entries.sort();
            let mut expected: Vec<(String, i64)> = case.output["vocab"]
                .as_object()
                .expect("vocab")
                .iter()
                .map(|(key, id)| (key.clone(), id.as_i64().unwrap_or(0)))
                .collect();
            expected.sort();
            assert_eq!(entries, expected, "caso {}", case.case);
        }
    }
}

pub(crate) fn compare_validate(case: &FixtureCase) {
    let config = ez_tts::kokoro::KoKoroConfig::new(
        input_text!(case, "model_path"),
        input_text!(case, "voices_path"),
    );
    match case.error.as_deref() {
        Some(expected) => assert_eq!(config.validate().expect_err("error esperado").0, expected),
        None => assert!(config.validate().is_ok(), "caso {}", case.case),
    }
}

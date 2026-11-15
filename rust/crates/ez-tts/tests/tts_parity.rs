//! Paridad del dominio `tts` contra `parity/fixtures/tts.jsonl`.
//!
//! Cubre `normalize_for_kokoro`, el tokenizador, el troceado, las pausas, el
//! deslizamiento, el recorte, las voces y la síntesis de Kokoro. Lo que necesita
//! `onnxruntime.dll` o `espeak-ng` se omite con un mensaje explícito cuando esta
//! máquina no los tiene en `ORT_DYLIB_PATH`/`EZAGENT_ESPEAK_*`.
//!
//! DECISION: `ez-tts` no declara `serde_json`; la prueba extrae los campos del
//! fixture con macros y compara con tipos locales, sin construir valores JSON.

use std::path::Path;

use ez_parity::fixtures;
use ez_tts::kokoro::session;
use ez_tts::kokoro::{chunker, embedded_vocab, pauses, sliding, Kokoro, Tokenizer};
use ez_tts::phonemes::{EspeakConfig, EspeakPhonemizer, FakePhonemizer};
use ez_tts::voices::Voices;

#[path = "tts_parity/support.rs"]
#[macro_use]
mod support;
#[path = "tts_parity/audio_cases.rs"]
mod audio_cases;
#[path = "tts_parity/kokoro_cases.rs"]
mod kokoro_cases;
#[path = "tts_parity/text_cases.rs"]
mod text_cases;
#[path = "tts_parity/voices_cases.rs"]
mod voices_cases;

use audio_cases::{compare_insert, compare_synthesize, compare_trim};
use kokoro_cases::{compare_artifacts, compare_create, compare_input_dtypes, compare_stream};
use support::{close, compare_timings, number, output_text, write_case_files};
use text_cases::{
    compare_load_vocab, compare_normalize_text, compare_normalized, compare_phonemize,
    compare_split, compare_tokenize, compare_validate, compare_vocab,
};
use voices_cases::{compare_style_for, compare_style_result};

const FIXTURES: &str = "../../../parity/fixtures/tts.jsonl";
const TOTAL_CASES: usize = 297;

#[test]
fn every_tts_case_matches_python() {
    let cases = fixtures::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES))
        .expect("fixtures de tts legibles");
    let directory = ez_core::downloads::data_dir().join("models").join("kokoro");
    let model_path = directory.join("kokoro-v1.0.int8.onnx");
    let voices_path = directory.join("voices-v1.0.bin");
    let voices = Voices::load(&voices_path).ok();
    let espeak_ready = EspeakPhonemizer::new(&EspeakConfig::default()).is_ok();
    let plain = Tokenizer::with_phonemizer(Box::new(FakePhonemizer::identity()), None);
    let mut kokoro = if espeak_ready
        && session::onnxruntime_path().is_some()
        && model_path.is_file()
        && voices_path.is_file()
    {
        Kokoro::new(
            &model_path.display().to_string(),
            &voices_path.display().to_string(),
            None,
            None,
        )
        .ok()
    } else {
        None
    };
    let temp = std::env::temp_dir().join(format!("ez-tts-parity-{}", std::process::id()));
    std::fs::create_dir_all(&temp).expect("carpeta temporal");
    write_case_files(&temp);
    let original = std::env::current_dir().expect("directorio actual");
    std::env::set_current_dir(&temp).expect("directorio del caso");

    let mut cases_run = 0;
    let mut skipped_native = 0;
    for case in &cases {
        let ran = match case.function.as_str() {
            "normalize_for_kokoro" => {
                compare_normalized(case);
                true
            }
            "tokenizer.normalize_text" => {
                compare_normalize_text(case);
                true
            }
            "tokenizer.tokenize" => {
                compare_tokenize(case, &plain);
                true
            }
            "tokenizer.known" => {
                assert_eq!(
                    plain.known(&input_text!(case, "phonemes")),
                    output_text(case, "known"),
                    "caso {}",
                    case.case
                );
                true
            }
            "tokenizer.vocab" => {
                compare_vocab(case);
                true
            }
            "tokenizer.phonemize" => match Tokenizer::new(&EspeakConfig::default(), None) {
                Ok(tokenizer) => {
                    compare_phonemize(case, &tokenizer);
                    true
                }
                Err(_) => false,
            },
            "chunker.split_phonemes" => {
                compare_split(case);
                true
            }
            "chunker.pause_after" => {
                let pause = chunker::pause_after(
                    &input_text!(case, "phonemes"),
                    number(case, "sentence"),
                    number(case, "clause"),
                );
                close(
                    pause,
                    case.output["pause"].as_f64().unwrap_or(0.0),
                    1e-12,
                    "pause",
                    &case.case,
                );
                true
            }
            "pauses.wanted_after" => {
                let pause = pauses::wanted_after(
                    &input_text!(case, "phoneme"),
                    number(case, "sentence"),
                    number(case, "clause"),
                );
                close(
                    pause,
                    case.output["pause"].as_f64().unwrap_or(0.0),
                    1e-12,
                    "pause",
                    &case.case,
                );
                true
            }
            "sliding.token_edges" => {
                let edges = sliding::token_edges(
                    &ints_of!(case.input["duration"]),
                    case.input["samples"].as_u64().unwrap_or(0) as usize,
                );
                assert_eq!(
                    edges.expect("bordes"),
                    ints_of!(case.output["edges"]),
                    "caso {}",
                    case.case
                );
                true
            }
            "sliding.timings" => {
                let timings = sliding::timings(
                    &input_text!(case, "phonemes"),
                    &ints_of!(case.input["edges"]),
                    case.input["sample_rate"].as_u64().unwrap_or(0) as u32,
                );
                compare_timings(case, &timings, "timings");
                true
            }
            "sliding.synthesize" => {
                compare_synthesize(case);
                true
            }
            "pauses.insert" => {
                compare_insert(case);
                true
            }
            "trim.trim" => {
                compare_trim(case);
                true
            }
            "tts.resolve_kokoro_files" => {
                let model = case.input["model_path"]
                    .as_str()
                    .filter(|path| !path.is_empty());
                let voices = case.input["voices_path"]
                    .as_str()
                    .filter(|path| !path.is_empty());
                let (model, voices) = ez_core::models::resolve_kokoro_files(
                    case.input["variant"].as_str().unwrap_or(""),
                    model,
                    voices,
                )
                .expect("rutas de Kokoro");
                assert_eq!(
                    model.as_deref(),
                    case.output["model"].as_str(),
                    "modelo de {}",
                    case.case
                );
                assert_eq!(
                    voices.as_deref(),
                    case.output["voices"].as_str(),
                    "voces de {}",
                    case.case
                );
                true
            }
            "config.KoKoroConfig.validate" => {
                compare_validate(case);
                true
            }
            "session.resolve_providers" => {
                let name = case.input["env"]["ONNX_PROVIDER"].as_str().unwrap_or("");
                std::env::set_var("ONNX_PROVIDER", name);
                let produced = session::resolve_providers();
                std::env::remove_var("ONNX_PROVIDER");
                assert_eq!(
                    produced,
                    strings_of!(case.output["providers"]),
                    "caso {}",
                    case.case
                );
                true
            }
            "kokoro._load_vocab" => {
                compare_load_vocab(case);
                true
            }
            "kokoro.get_voices" => match &voices {
                Some(voices) => {
                    assert_eq!(
                        voices.names(),
                        strings_of!(case.output["voices"]),
                        "caso {}",
                        case.case
                    );
                    true
                }
                None => false,
            },
            "kokoro.get_voice_style" => match &voices {
                Some(voices) => {
                    let produced = voices
                        .style(&input_text!(case, "name"))
                        .map(|style| style.data().to_vec());
                    compare_style_result(case, produced);
                    true
                }
                None => false,
            },
            "voice._voice_style" => match &voices {
                Some(voices) => {
                    let produced = voices
                        .voice_style(&input_text!(case, "voice"))
                        .map(|style| style.data().to_vec());
                    compare_style_result(case, produced);
                    true
                }
                None => false,
            },
            "kokoro._style_for" => match &voices {
                Some(voices) => {
                    compare_style_for(case, voices);
                    true
                }
                None => false,
            },
            "session.input_dtypes" => match &mut kokoro {
                Some(kokoro) => {
                    compare_input_dtypes(case, kokoro.model());
                    true
                }
                None => false,
            },
            "session.embedded_vocab" => match &mut kokoro {
                Some(kokoro) => {
                    let produced = embedded_vocab(kokoro.model()).unwrap_or_default();
                    assert_eq!(
                        produced.len() as u64,
                        case.output["vocab"]
                            .as_object()
                            .map_or(0, |vocab| vocab.len() as u64),
                        "caso {}",
                        case.case
                    );
                    true
                }
                None => false,
            },
            "tts.artifacts" => match &mut kokoro {
                Some(kokoro) => {
                    compare_artifacts(case, &model_path, &voices_path, kokoro);
                    true
                }
                None => false,
            },
            "kokoro.create_timed" => match &mut kokoro {
                Some(kokoro) => {
                    compare_create(case, kokoro, voices.as_ref().expect("voces"));
                    true
                }
                None => false,
            },
            "kokoro.create_stream" => match &mut kokoro {
                Some(kokoro) => {
                    compare_stream(case, kokoro, voices.as_ref().expect("voces"));
                    true
                }
                None => false,
            },
            other => panic!("caso sin cubrir: {} ({other})", case.case),
        };
        if ran {
            cases_run += 1;
        } else {
            skipped_native += 1;
        }
    }
    std::env::set_current_dir(&original).expect("restaurar el directorio");
    let _ = std::fs::remove_dir_all(&temp);
    if skipped_native > 0 {
        eprintln!(
            "tts: {skipped_native} casos omitidos: falta onnxruntime.dll (ORT_DYLIB_PATH o \
             junto al ejecutable), espeak-ng (EZAGENT_ESPEAK_LIBRARY) o el modelo Kokoro; \
             se contaron aparte, no como paso silencioso"
        );
    }
    let cases_in_file = cases.len();
    assert!(
        cases_in_file >= TOTAL_CASES,
        "faltan casos: {cases_in_file}"
    );
    assert_eq!(
        cases_run + skipped_native,
        cases_in_file,
        "se saltó algún caso"
    );
}

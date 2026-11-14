//! Unit tests for `kokoro.rs` and its submodules (F4-04).

use std::sync::{Arc, Mutex};

use super::*;
use crate::phonemes::FakePhonemizer;

#[derive(Default)]
struct Trace {
    calls: usize,
    speed: Option<f64>,
    windows: Vec<Vec<i64>>,
}

struct FakeModel {
    dtypes: HashMap<String, DType>,
    outputs: Vec<String>,
    metadata: HashMap<String, String>,
    durations: bool,
    trace: Arc<Mutex<Trace>>,
}

impl FakeModel {
    fn new() -> Self {
        Self {
            dtypes: HashMap::from([
                ("tokens".to_string(), DType::Int64),
                ("style".to_string(), DType::Float32),
                ("speed".to_string(), DType::Float32),
            ]),
            outputs: vec!["audio".to_string()],
            metadata: HashMap::new(),
            durations: false,
            trace: Arc::new(Mutex::new(Trace::default())),
        }
    }

    fn with_duration() -> Self {
        let mut model = Self::new();
        model.outputs.push("duration".to_string());
        model.durations = true;
        model
    }

    fn trace(&self) -> Arc<Mutex<Trace>> {
        Arc::clone(&self.trace)
    }
}

impl Model for FakeModel {
    fn input_dtypes(&self) -> &HashMap<String, DType> {
        &self.dtypes
    }

    fn tokens_input(&self) -> &str {
        "tokens"
    }

    fn output_names(&self) -> &[String] {
        &self.outputs
    }

    fn metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(String::as_str)
    }

    fn infer(
        &mut self,
        tokens: &[i64],
        _style: &[f32],
        speed: f64,
    ) -> Result<(Vec<f32>, Option<Vec<i64>>), String> {
        let mut trace = self.trace.lock().expect("traza");
        trace.calls += 1;
        trace.speed = Some(speed);
        trace.windows.push(tokens.to_vec());
        let audio = vec![0.1; tokens.len() * 100];
        let duration = self.durations.then(|| vec![2_i64; tokens.len() + 2]);
        Ok((audio, duration))
    }
}

fn tokenizer() -> Tokenizer {
    Tokenizer::with_phonemizer(
        Box::new(FakePhonemizer::new(|text, _lang| format!("{text} "))),
        None,
    )
}

fn voices() -> Voices {
    let style = Style::new(5, 2, vec![0.1; 10]).expect("estilo");
    Voices::from_styles(HashMap::from([("voz".to_string(), style)]))
}

fn kokoro(model: FakeModel) -> (Kokoro, Arc<Mutex<Trace>>) {
    let trace = model.trace();
    let kokoro = Kokoro::with_tokenizer(
        Box::new(model),
        tokenizer(),
        voices(),
        KoKoroConfig::new("modelo.onnx", "voces.bin"),
    );
    (kokoro, trace)
}

#[test]
fn tokenizer_strips_filters_and_checks_the_limit() {
    let tokenizer = tokenizer();
    assert_eq!(Tokenizer::normalize_text("  hola \n"), "hola");
    assert_eq!(
        tokenizer.tokenize("ao", Some(510)).expect("corto"),
        vec![43, 57]
    );
    assert_eq!(
        tokenizer.known("a#b"),
        "ab",
        "las letras fuera del vocabulario se descartan"
    );
    let error = tokenizer
        .tokenize("ˈola", Some(3))
        .expect_err("demasiado largo");
    assert_eq!(error.0, "text is too long, must be less than 3 phonemes");
    assert_eq!(tokenizer.vocab().len(), 114);
}

#[test]
fn tokenizer_phonemize_filters_and_strips() {
    let phonemizer = FakePhonemizer::new(|text, _lang| format!("  {text} # extra  "));
    let tokenizer = Tokenizer::with_phonemizer(Box::new(phonemizer), None);
    let phonemes = tokenizer.phonemize("hola", "es", true).expect("fonemas");
    assert_eq!(phonemes, "hola  extra");
}

#[test]
fn chunker_balances_batches_like_the_python() {
    assert_eq!(
        chunker::split_phonemes("a b c d e f g h i j", 5),
        vec!["a b c", "d e f", "g h i", "j"]
    );
    assert_eq!(
        chunker::split_phonemes("hola mundo. esto es una prueba, con calma; y mas", 20),
        vec!["hola mundo.", "esto es una prueba,", "con calma; y mas"]
    );
    assert_eq!(
        chunker::split_phonemes(&"x".repeat(12), 5),
        vec!["xxxxx", "xxxxx", "xx"]
    );
    assert!(chunker::split_phonemes("   ", 5).is_empty());
    assert_eq!(chunker::pause_after("hola.", 0.3, 0.1), 0.3);
    assert_eq!(chunker::pause_after("hola,", 0.3, 0.1), 0.1);
    assert_eq!(chunker::pause_after("hola", 0.3, 0.1), 0.0);
    assert_eq!(chunker::pause_after("", 0.3, 0.1), 0.3);
}

#[test]
fn trim_drops_the_silence_around_the_voice() {
    let mut audio = vec![0.0_f32; 10_000];
    audio.extend(std::iter::repeat_n(0.5_f32, 5_000));
    audio.extend(std::iter::repeat_n(0.0_f32, 10_000));
    let (trimmed, (start, end)) = trim::trim(&audio);
    assert_eq!((start, end), (9_216, 16_384));
    assert_eq!(trimmed.len(), 7_168);
    let (silence, (start, end)) = trim::trim(&vec![0.0_f32; 4_096]);
    assert_eq!((start, end), (0, 4_096));
    assert_eq!(silence.len(), 4_096);
    assert_eq!(trim::trim(&[]), (Vec::new(), (0, 0)));
}

#[test]
fn token_edges_and_timings_follow_numpy() {
    let edges = sliding::token_edges(&[2, 3, 1, 4], 1_000).expect("bordes");
    assert_eq!(edges, vec![0, 200, 500, 600, 1_000]);
    let edges = sliding::token_edges(&[1, 1, 1], 10).expect("redondeo");
    assert_eq!(edges, vec![0, 3, 7, 10]);
    assert!(sliding::token_edges(&[], 100).is_err());
    let spoken = sliding::timings("ab", &[0, 200, 500, 600, 1_000], 100);
    assert_eq!(spoken.len(), 2);
    assert_eq!(
        (spoken[0].phoneme.as_str(), spoken[0].start, spoken[0].end),
        ("a", 0.0, 2.0)
    );
    assert_eq!(
        (spoken[1].phoneme.as_str(), spoken[1].start, spoken[1].end),
        ("b", 2.0, 5.0)
    );
}

#[test]
fn window_candidates_prefer_sentence_ends() {
    let tokens: Vec<i64> = (0..30).collect();
    let stops = std::collections::HashSet::from([12_i64]);
    let spaces = std::collections::HashSet::new();
    assert_eq!(
        sliding::candidates(&tokens, &stops, &spaces, 10, 5, 20),
        vec![13]
    );
    let spaces = std::collections::HashSet::from([10_i64]);
    assert_eq!(
        sliding::candidates(
            &tokens,
            &std::collections::HashSet::new(),
            &spaces,
            10,
            5,
            20
        ),
        vec![10]
    );
    assert_eq!(
        sliding::candidates(
            &tokens,
            &std::collections::HashSet::new(),
            &std::collections::HashSet::new(),
            10,
            5,
            20
        ),
        vec![10]
    );
}

#[test]
fn sliding_synthesizes_overlapping_windows() {
    let run = |tokens: &[i64]| {
        Ok((
            vec![0.5_f32; tokens.len() * 100],
            vec![1_i64; tokens.len() + 2],
        ))
    };
    let options = sliding::SynthOptions::default();
    let (audio, edges) = sliding::synthesize(
        run,
        &[1, 2, 3],
        100,
        &std::collections::HashSet::new(),
        &std::collections::HashSet::new(),
        &options,
    )
    .expect("una ventana");
    assert_eq!(audio.len(), 180);
    assert_eq!(edges, vec![0, 60, 120, 180]);

    let mut calls = 0;
    let options = sliding::SynthOptions {
        keep: 4,
        context: 1,
        lookahead: 1,
        crossfade: 0.0,
    };
    let (audio, edges) = sliding::synthesize(
        |tokens: &[i64]| {
            calls += 1;
            Ok((
                vec![0.5_f32; tokens.len() * 100],
                vec![1_i64; tokens.len() + 2],
            ))
        },
        &(0..10).collect::<Vec<i64>>(),
        100,
        &std::collections::HashSet::new(),
        &std::collections::HashSet::new(),
        &options,
    )
    .expect("tres ventanas");
    assert_eq!(calls, 3);
    assert_eq!(audio.len(), 706);
    assert_eq!(
        edges,
        vec![0, 72, 143, 215, 286, 361, 436, 511, 586, 646, 706]
    );
}

#[test]
fn join_crossfades_the_kept_regions() {
    let (audio, starts) = sliding::join(&[vec![0.0, 0.0], vec![1.0, 1.0]], 1);
    assert_eq!(audio, vec![0.0, 0.0, 1.0]);
    assert_eq!(starts, vec![0, 1]);
    assert_eq!(sliding::join(&[], 1).0, Vec::<f32>::new());
}

#[test]
fn pauses_are_inserted_inside_the_model_silence() {
    let mut audio = vec![0.5_f32; 24_000];
    audio[9_600..12_000].fill(0.0);
    let timings = vec![
        sliding::Timing {
            phoneme: ".".to_string(),
            start: 0.4,
            end: 0.5,
        },
        sliding::Timing {
            phoneme: "a".to_string(),
            start: 0.5,
            end: 0.6,
        },
    ];
    let (output, moved) = pauses::insert(&audio, &timings, 24_000, 0.3, 0.1);
    assert_eq!(output.len(), 28_800);
    assert_eq!((moved[0].start, moved[0].end), (0.4, 0.5));
    assert_eq!((moved[1].start, moved[1].end), (0.7, 0.7999999999999999));
    assert_eq!(pauses::wanted_after(".", 0.3, 0.1), 0.3);
    assert_eq!(pauses::wanted_after(",", 0.3, 0.1), 0.1);
    assert_eq!(pauses::wanted_after("a", 0.3, 0.1), 0.0);
    let (same, kept) = pauses::insert(&audio, &timings[..0], 24_000, 0.3, 0.1);
    assert_eq!(same.len(), audio.len());
    assert!(kept.is_empty());
}

#[test]
fn create_uses_the_model_and_trims() {
    let (mut kokoro, trace) = kokoro(FakeModel::new());
    let (audio, rate) = kokoro
        .create("hola.", Voice::Name("voz"), &SynthesisOptions::default())
        .expect("audio");
    assert_eq!(rate, SAMPLE_RATE);
    assert_eq!(audio.len(), 500);
    assert!(audio.iter().all(|sample| *sample == 0.1));
    let trace = trace.lock().expect("traza");
    assert_eq!(trace.calls, 1);
    assert_eq!(trace.windows[0], vec![50, 57, 54, 43, 4]);
    drop(trace);
    assert_eq!(kokoro.get_voices(), vec!["voz"]);
    assert_eq!(
        kokoro.get_voice_style("nadie").expect_err("sin voz").0,
        "KeyError: 'nadie'"
    );
    let style = kokoro.get_voice_style("voz").expect("estilo");
    let (styled, _) = kokoro
        .create("hola.", Voice::Style(&style), &SynthesisOptions::default())
        .expect("audio con estilo");
    assert_eq!(styled.len(), 500);
}

#[test]
fn create_rejects_bad_options() {
    let (mut kokoro, _) = kokoro(FakeModel::new());
    let options = SynthesisOptions {
        speed: 0.4,
        ..SynthesisOptions::default()
    };
    assert_eq!(
        kokoro
            .create("hola.", Voice::Name("voz"), &options)
            .expect_err("velocidad")
            .0,
        "Speed should be between 0.5 and 2.0, got 0.4"
    );
    assert_eq!(
        kokoro
            .create("hola.", Voice::Name("nadie"), &SynthesisOptions::default())
            .expect_err("voz")
            .0,
        "Voice nadie not found in available voices"
    );
    let mut silent = Kokoro::with_tokenizer(
        Box::new(FakeModel::new()),
        Tokenizer::with_phonemizer(
            Box::new(FakePhonemizer::new(|_text, _lang| "###".to_string())),
            None,
        ),
        voices(),
        KoKoroConfig::new("modelo.onnx", "voces.bin"),
    );
    assert_eq!(
        silent
            .create("hola.", Voice::Name("voz"), &SynthesisOptions::default())
            .expect_err("sin fonemas")
            .0,
        "Nothing to synthesize, 'hola.' produced no phonemes"
    );
}

#[test]
fn integer_speed_is_rounded_ties_even() {
    let dtypes = HashMap::from([("speed".to_string(), DType::Int64)]);
    assert_eq!(super::speed_value(&dtypes, 1.5), 2.0);
    assert_eq!(super::speed_value(&dtypes, 2.5), 2.0);
    assert_eq!(super::speed_value(&dtypes, 1.0), 1.0);
    let dtypes = HashMap::from([("speed".to_string(), DType::Float32)]);
    assert_eq!(super::speed_value(&dtypes, 1.5), 1.5);
}

#[test]
fn long_phonemes_are_split_in_batches() {
    let (mut kokoro, trace) = kokoro(FakeModel::new());
    let options = SynthesisOptions {
        is_phonemes: true,
        ..SynthesisOptions::default()
    };
    let (audio, _) = kokoro
        .create(&"a".repeat(700), Voice::Name("voz"), &options)
        .expect("audio");
    assert_eq!(audio.len(), 70_000);
    let trace = trace.lock().expect("traza");
    assert_eq!(trace.calls, 2);
    assert_eq!(trace.windows[0].len(), 510);
    assert_eq!(trace.windows[1].len(), 190);
}

#[test]
fn create_stream_can_be_abandoned() {
    let (mut kokoro, trace) = kokoro(FakeModel::new());
    let options = SynthesisOptions {
        is_phonemes: true,
        ..SynthesisOptions::default()
    };
    let mut chunks = Vec::new();
    kokoro
        .create_stream(
            &"a".repeat(700),
            Voice::Name("voz"),
            &options,
            |audio, rate| {
                chunks.push((audio.len(), rate));
                false
            },
        )
        .expect("flujo");
    assert_eq!(chunks, vec![(51_000, SAMPLE_RATE)]);
    assert_eq!(trace.lock().expect("traza").calls, 1);
}

#[test]
fn create_timed_reports_every_phoneme() {
    let (mut kokoro, _) = kokoro(FakeModel::with_duration());
    let (audio, rate, spoken) = kokoro
        .create_timed("hola.", Voice::Name("voz"), &SynthesisOptions::default())
        .expect("audio");
    assert_eq!(rate, SAMPLE_RATE);
    assert_eq!(audio.len(), 500);
    assert_eq!(spoken.len(), 5);
    assert_eq!(spoken[4].phoneme, ".");
    assert!(spoken[4].end > spoken[4].start);
}

#[test]
fn continuous_needs_a_model_with_durations() {
    let (mut kokoro, _) = kokoro(FakeModel::new());
    let options = SynthesisOptions {
        continuous: true,
        ..SynthesisOptions::default()
    };
    assert_eq!(
        kokoro
            .create("hola.", Voice::Name("voz"), &options)
            .expect_err("sin duraciones")
            .0,
        "continuous synthesis needs a model that reports durations, export one with scripts/export.py"
    );
}

#[test]
fn config_validate_uses_the_python_messages() {
    let config = KoKoroConfig::new("no-modelo.onnx", "no-voces.bin");
    let error = config.validate().expect_err("sin archivos");
    assert!(error.0.starts_with("Voices file not found at no-voces.bin"));
    assert!(error.0.contains(
        "wget https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin"
    ));
    let path = std::env::temp_dir().join(format!("ez-tts-voces-{}.bin", std::process::id()));
    std::fs::write(&path, b"x").expect("archivo temporal");
    let config = KoKoroConfig::new("no-modelo.onnx", path.to_string_lossy().to_string());
    let error = config.validate().expect_err("sin modelo");
    assert!(error.0.starts_with("Model file not found at "));
    let _ = std::fs::remove_file(path);
}

#[test]
fn vocabularies_come_from_metadata_and_json() {
    let mut model = FakeModel::new();
    model.metadata.insert(
        session::CONFIG_KEY.to_string(),
        "{\"vocab\": {\"a\": 7, \"zz\": 9}}".to_string(),
    );
    let vocab = super::embedded_vocab(&model).expect("vocabulario");
    assert_eq!(vocab.get(&'a'), Some(&7));
    assert_eq!(vocab.len(), 1);
    model
        .metadata
        .insert(session::CONFIG_KEY.to_string(), "no es json".to_string());
    assert!(super::embedded_vocab(&model).is_none());
    model
        .metadata
        .insert(session::CONFIG_KEY.to_string(), "{}".to_string());
    assert!(super::embedded_vocab(&model).is_none());

    let path = std::env::temp_dir().join(format!("ez-tts-vocab-{}.json", std::process::id()));
    std::fs::write(&path, "{\"vocab\": {\"b\": 3}}").expect("json temporal");
    let loaded =
        super::tokenizer::load_vocab(Some(VocabConfig::File(path.clone()))).expect("vacío");
    assert_eq!(loaded.and_then(|vocab| vocab.get(&'b').copied()), Some(3));
    let _ = std::fs::remove_file(path);
    assert!(super::tokenizer::load_vocab(None)
        .expect("sin config")
        .is_none());
}

#[test]
fn style_rows_are_capped_at_the_file() {
    let style = Style::new(3, 2, vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]).expect("estilo");
    assert_eq!(super::style_for(&style, 5), style.row(2).expect("fila"));
    assert_eq!(super::style_for(&style, 1), style.row(0).expect("fila"));
}

/// End to end with the real graph; runs only when the paths are configured.
#[test]
fn real_model_produces_audio_when_configured() {
    let (Ok(model), Ok(voices)) = (
        std::env::var("EZAGENT_TTS_MODEL"),
        std::env::var("EZAGENT_TTS_VOICES"),
    ) else {
        return;
    };
    let mut kokoro = Kokoro::new(&model, &voices, None, None).expect("Kokoro real");
    assert_eq!(kokoro.get_voices().len(), 54);
    let (audio, rate) = kokoro
        .create(
            "Hola mundo.",
            Voice::Name("ef_dora"),
            &SynthesisOptions {
                speed: 0.95,
                lang: "es",
                sentence_pause: 0.30,
                clause_pause: 0.12,
                ..SynthesisOptions::default()
            },
        )
        .expect("audio real");
    assert_eq!(rate, SAMPLE_RATE);
    assert!(
        audio.len() > SAMPLE_RATE as usize / 4,
        "audio demasiado corto"
    );
    assert!(audio.iter().all(|sample| sample.is_finite()));
    assert!(audio.iter().any(|sample| *sample != 0.0));
}

//! Port of `kokoro_onnx` inference (F4-04).
//!
//! The model is Kokoro v1.0 through ONNX Runtime: text goes to espeak-ng for
//! phonemes, the phonemes become token ids, and the graph returns `float32`
//! audio at 24 kHz. The pure parts (chunker, trim, pauses, sliding) and the
//! tokenizer are tested without the model or the DLL.

pub mod chunker;
pub mod pauses;
pub mod session;
pub mod sliding;
mod tokenizer;
pub mod trim;
pub mod vocab;

pub use tokenizer::{load_vocab, parse_config_vocab, Tokenizer, VocabConfig};

use std::collections::{HashMap, HashSet};
use std::path::Path;

use ez_core::pyjson::float_repr;
use ez_core::pyvalue::{collapse_spaces, repr_str};

use crate::phonemes::{EspeakConfig, PhonemeError};
use crate::voices::{Style, Voices};
use session::{DType, Model, OrtModel};
pub use sliding::Timing;

/// `MAX_PHONEME_LENGTH`: the model context.
pub const MAX_PHONEME_LENGTH: usize = 510;
/// `SAMPLE_RATE` of the exported graph.
pub const SAMPLE_RATE: u32 = 24000;

/// `Kokoro` failures, with the Python texts when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KokoroError(pub String);

impl std::fmt::Display for KokoroError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for KokoroError {}

impl From<String> for KokoroError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<PhonemeError> for KokoroError {
    fn from(error: PhonemeError) -> Self {
        Self(error.0)
    }
}

/// `KoKoroConfig`: the two files Kokoro needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KoKoroConfig {
    pub model_path: String,
    pub voices_path: String,
    pub espeak_config: Option<EspeakConfig>,
}

impl KoKoroConfig {
    pub fn new(model_path: impl Into<String>, voices_path: impl Into<String>) -> Self {
        Self {
            model_path: model_path.into(),
            voices_path: voices_path.into(),
            espeak_config: None,
        }
    }

    /// `KoKoroConfig.validate` with the Python messages.
    pub fn validate(&self) -> Result<(), KokoroError> {
        if !Path::new(&self.voices_path).exists() {
            return Err(KokoroError(format!(
                "Voices file not found at {}\
                 \nYou can download the voices file using the following command:\
                 \nwget https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin",
                self.voices_path
            )));
        }
        if !Path::new(&self.model_path).exists() {
            return Err(KokoroError(format!(
                "Model file not found at {}\
                 \nYou can download the model file from https://github.com/thewh1teagle/kokoro-onnx/releases",
                self.model_path
            )));
        }
        Ok(())
    }
}

/// Options of `create`/`create_timed`; the default is the Python's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SynthesisOptions<'a> {
    pub speed: f64,
    pub lang: &'a str,
    pub is_phonemes: bool,
    pub trim: bool,
    pub sentence_pause: f64,
    pub clause_pause: f64,
    pub continuous: bool,
}

impl Default for SynthesisOptions<'_> {
    fn default() -> Self {
        Self {
            speed: 1.0,
            lang: "en-us",
            is_phonemes: false,
            trim: true,
            sentence_pause: 0.25,
            clause_pause: 0.1,
            continuous: false,
        }
    }
}

/// `voice`: a name of `voices-v1.0.bin` or an already resolved style.
#[derive(Debug, Clone, Copy)]
pub enum Voice<'a> {
    Name(&'a str),
    Style(&'a Style),
}

impl<'a> From<&'a str> for Voice<'a> {
    fn from(name: &'a str) -> Self {
        Self::Name(name)
    }
}

impl<'a> From<&'a Style> for Voice<'a> {
    fn from(style: &'a Style) -> Self {
        Self::Style(style)
    }
}

/// The inference session plus everything `create` needs.
pub struct Kokoro {
    config: KoKoroConfig,
    model: Box<dyn Model>,
    tokenizer: Tokenizer,
    voices: Voices,
    stops: HashSet<i64>,
    spaces: HashSet<i64>,
    has_timings: bool,
}

impl Kokoro {
    /// `Kokoro(model_path, voices_path, espeak_config, vocab_config)`.
    pub fn new(
        model_path: &str,
        voices_path: &str,
        espeak_config: Option<EspeakConfig>,
        vocab_config: Option<VocabConfig>,
    ) -> Result<Self, KokoroError> {
        let config = KoKoroConfig {
            model_path: model_path.to_string(),
            voices_path: voices_path.to_string(),
            espeak_config,
        };
        config.validate()?;
        let model = Box::new(OrtModel::open(Path::new(model_path))?);
        let voices = Voices::load(Path::new(voices_path))?;
        let vocab = tokenizer::load_vocab(vocab_config)?.or_else(|| embedded_vocab(model.as_ref()));
        Self::assemble(model, config, vocab, voices)
    }

    /// `from_session`: reuse an already built model (tests, callers).
    pub fn from_model(
        model: Box<dyn Model>,
        config: KoKoroConfig,
        vocab_config: Option<VocabConfig>,
        voices: Voices,
    ) -> Result<Self, KokoroError> {
        let vocab = tokenizer::load_vocab(vocab_config)?.or_else(|| embedded_vocab(model.as_ref()));
        Self::assemble(model, config, vocab, voices)
    }

    fn assemble(
        model: Box<dyn Model>,
        config: KoKoroConfig,
        vocab: Option<HashMap<char, i64>>,
        voices: Voices,
    ) -> Result<Self, KokoroError> {
        let espeak = config.espeak_config.clone().unwrap_or_default();
        let tokenizer = Tokenizer::new(&espeak, vocab)?;
        Ok(Self::with_tokenizer(model, tokenizer, voices, config))
    }

    /// Injection seam: the caller provides the tokenizer (tests, F4-05).
    pub fn with_tokenizer(
        model: Box<dyn Model>,
        tokenizer: Tokenizer,
        voices: Voices,
        config: KoKoroConfig,
    ) -> Self {
        let stops = ".!?"
            .chars()
            .filter_map(|mark| tokenizer.vocab().get(&mark).copied())
            .collect();
        let spaces = " "
            .chars()
            .filter_map(|mark| tokenizer.vocab().get(&mark).copied())
            .collect();
        let has_timings = model.output_names().iter().any(|name| name == "duration");
        Self {
            config,
            model,
            tokenizer,
            voices,
            stops,
            spaces,
            has_timings,
        }
    }

    pub fn config(&self) -> &KoKoroConfig {
        &self.config
    }

    /// `has_timings`: whether the graph reports phoneme durations.
    pub fn has_timings(&self) -> bool {
        self.has_timings
    }

    /// The inference model behind this instance.
    pub fn model(&mut self) -> &mut dyn Model {
        self.model.as_mut()
    }

    /// The tokenizer in use (phonemizer plus vocabulary).
    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// `_prepare`: full phoneme string and batches with their pauses.
    pub fn prepare(
        &self,
        text: &str,
        options: &SynthesisOptions<'_>,
    ) -> Result<Prepared, KokoroError> {
        prepare(&self.tokenizer, text, options)
    }

    /// `get_voice_style`: the style of one voice of the file.
    pub fn get_voice_style(&self, name: &str) -> Result<Style, KokoroError> {
        self.voices
            .get(name)
            .cloned()
            .ok_or_else(|| KokoroError(format!("KeyError: {}", repr_str(name))))
    }

    /// `get_voices`: sorted names.
    pub fn get_voices(&self) -> Vec<String> {
        self.voices.names()
    }

    /// `create`: `(audio, sample_rate)`.
    pub fn create(
        &mut self,
        text: &str,
        voice: Voice<'_>,
        options: &SynthesisOptions<'_>,
    ) -> Result<(Vec<f32>, u32), KokoroError> {
        let (audio, rate, _) = self.create_timed(text, voice, options)?;
        Ok((audio, rate))
    }

    /// `create_timed`: audio and when every phoneme is spoken.
    pub fn create_timed(
        &mut self,
        text: &str,
        voice: Voice<'_>,
        options: &SynthesisOptions<'_>,
    ) -> Result<(Vec<f32>, u32, Vec<Timing>), KokoroError> {
        check_speed(options.speed)?;
        let style = resolve_voice(&self.voices, voice)?;
        let prepared = prepare(&self.tokenizer, text, options)?;
        let speed = speed_value(self.model.input_dtypes(), options.speed);

        let (audio, spoken) = if options.continuous {
            create_continuous(
                self.model.as_mut(),
                &self.tokenizer,
                &prepared.phonemes,
                style,
                speed,
                self.has_timings,
                &self.stops,
                &self.spaces,
            )?
        } else {
            create_batches(
                self.model.as_mut(),
                &self.tokenizer,
                &prepared.batches,
                style,
                speed,
                options.trim,
            )?
        };
        let (audio, spoken) = pauses::insert(
            &audio,
            &spoken,
            SAMPLE_RATE,
            options.sentence_pause,
            options.clause_pause,
        )?;
        Ok((audio, SAMPLE_RATE, spoken))
    }

    /// `create_stream`: one callback per batch, `false` abandons the stream.
    pub fn create_stream<F>(
        &mut self,
        text: &str,
        voice: Voice<'_>,
        options: &SynthesisOptions<'_>,
        mut on_chunk: F,
    ) -> Result<(), KokoroError>
    where
        F: FnMut(&[f32], u32) -> bool,
    {
        check_speed(options.speed)?;
        let style = resolve_voice(&self.voices, voice)?;
        let prepared = prepare(&self.tokenizer, text, options)?;
        let speed = speed_value(self.model.input_dtypes(), options.speed);
        for (phonemes, pause) in &prepared.batches {
            let (audio, _) = create_batch(
                self.model.as_mut(),
                &self.tokenizer,
                phonemes,
                style,
                speed,
                options.trim,
                *pause,
            )?;
            if !on_chunk(&audio, SAMPLE_RATE) {
                break;
            }
        }
        Ok(())
    }
}

fn check_speed(speed: f64) -> Result<(), KokoroError> {
    if (0.5..=2.0).contains(&speed) {
        return Ok(());
    }
    Err(KokoroError(format!(
        "Speed should be between 0.5 and 2.0, got {}",
        float_repr(speed)
    )))
}

fn resolve_voice<'a>(voices: &'a Voices, voice: Voice<'a>) -> Result<&'a Style, KokoroError> {
    match voice {
        Voice::Name(name) => voices
            .get(name)
            .ok_or_else(|| KokoroError(format!("Voice {name} not found in available voices"))),
        Voice::Style(style) => Ok(style),
    }
}

/// `_speed_value`: adapt the speed to an integer input of the graph.
fn speed_value(dtypes: &HashMap<String, DType>, speed: f64) -> f64 {
    let dtype = dtypes.get("speed").copied().unwrap_or(DType::Float32);
    if dtype.kind_is_integer() && speed != speed.trunc() {
        let rounded = speed.round_ties_even().max(1.0);
        tracing::warn!(
            "This model was exported with an integer speed input, rounding speed {} to {}",
            float_repr(speed),
            float_repr(rounded)
        );
        return rounded;
    }
    speed
}

/// What `_prepare` resolves: the full phoneme string and the batches.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    pub phonemes: String,
    pub batches: Vec<(String, f64)>,
}

/// `_prepare`: collapsed phonemes split into non-empty batches.
fn prepare(
    tokenizer: &Tokenizer,
    text: &str,
    options: &SynthesisOptions<'_>,
) -> Result<Prepared, KokoroError> {
    let phonemes = if options.is_phonemes {
        text.to_string()
    } else {
        tokenizer.phonemize(text, options.lang, true)?
    };
    let phonemes = collapse_spaces(&phonemes);
    let batched = chunker::split_phonemes(&phonemes, MAX_PHONEME_LENGTH)?;
    if batched.is_empty() {
        return Err(KokoroError(format!(
            "Nothing to synthesize, {} produced no phonemes",
            repr_str(text)
        )));
    }
    let last = batched.len() - 1;
    let batches = batched
        .iter()
        .enumerate()
        .map(|(index, batch)| {
            let pause = if index == last {
                0.0
            } else {
                chunker::pause_after(batch, options.sentence_pause, options.clause_pause)
            };
            (batch.clone(), pause)
        })
        .collect();
    Ok(Prepared { phonemes, batches })
}

/// `_style_for`: `n` phonemes use row `n - 1`, capped at the file.
///
/// `length == 0` da `-1` en el Python, es decir, la última fila.
pub fn style_for(style: &Style, length: usize) -> &[f32] {
    let index = length.min(style.rows());
    let index = if index == 0 {
        style.rows().saturating_sub(1)
    } else {
        index - 1
    };
    style.row(index).unwrap_or(&[])
}

/// `_create_batch`: one graph run, trimmed and padded with its pause.
#[allow(clippy::too_many_arguments)]
fn create_batch(
    model: &mut dyn Model,
    tokenizer: &Tokenizer,
    phonemes: &str,
    style: &Style,
    speed: f64,
    trim: bool,
    pause: f64,
) -> Result<(Vec<f32>, Option<Vec<i64>>), KokoroError> {
    let tokens = tokenizer.tokenize(phonemes, Some(MAX_PHONEME_LENGTH as i64))?;
    if tokens.is_empty() {
        return Err(KokoroError(format!(
            "No phonemes of {} are in the model vocabulary",
            repr_str(phonemes)
        )));
    }
    let (mut audio, duration) = model.infer(&tokens, style_for(style, tokens.len()), speed)?;
    let mut edges = duration
        .map(|duration| sliding::token_edges(&duration, audio.len()))
        .transpose()?;
    if let Some(edges) = &mut edges {
        edges.remove(0);
    }
    if trim {
        let (trimmed, (head, _)) = trim::trim(&audio);
        audio = trimmed;
        if let Some(edges) = &mut edges {
            let limit = audio.len() as i64;
            for edge in edges {
                *edge = (*edge - head as i64).clamp(0, limit);
            }
        }
        if pause != 0.0 {
            audio.resize(audio.len() + (pause * f64::from(SAMPLE_RATE)) as usize, 0.0);
        }
    }
    Ok((audio, edges))
}

/// `_create_batches`: synthesize each batch and join the results.
fn create_batches(
    model: &mut dyn Model,
    tokenizer: &Tokenizer,
    batches: &[(String, f64)],
    style: &Style,
    speed: f64,
    trim: bool,
) -> Result<(Vec<f32>, Vec<Timing>), KokoroError> {
    let mut parts: Vec<Vec<f32>> = Vec::new();
    let mut spoken = Vec::new();
    let mut offset = 0_i64;
    for (phonemes, pause) in batches {
        let (audio, edges) = create_batch(model, tokenizer, phonemes, style, speed, trim, *pause)?;
        if let Some(edges) = edges {
            let shifted: Vec<i64> = edges.iter().map(|edge| edge + offset).collect();
            spoken.extend(sliding::timings(
                &tokenizer.known(phonemes),
                &shifted,
                SAMPLE_RATE,
            ));
        }
        offset += audio.len() as i64;
        parts.push(audio);
    }
    Ok((parts.concat(), spoken))
}

/// `_create_continuous`: the whole text as overlapping windows.
#[allow(clippy::too_many_arguments)]
fn create_continuous(
    model: &mut dyn Model,
    tokenizer: &Tokenizer,
    phonemes: &str,
    style: &Style,
    speed: f64,
    has_timings: bool,
    stops: &HashSet<i64>,
    spaces: &HashSet<i64>,
) -> Result<(Vec<f32>, Vec<Timing>), KokoroError> {
    if !has_timings {
        return Err(KokoroError(
            "continuous synthesis needs a model that reports durations, \
             export one with scripts/export.py"
                .to_string(),
        ));
    }
    let tokens = tokenizer.tokenize(phonemes, None)?;
    let row = style_for(style, tokens.len());
    let (audio, edges) = sliding::synthesize(
        |window| {
            let (audio, duration) = model.infer(window, row, speed)?;
            let duration = duration.ok_or_else(|| "model without durations".to_string())?;
            Ok((audio, duration))
        },
        &tokens,
        SAMPLE_RATE,
        stops,
        spaces,
        &sliding::SynthOptions::default(),
    )?;
    Ok((
        audio,
        sliding::timings(&tokenizer.known(phonemes), &edges, SAMPLE_RATE),
    ))
}

/// `embedded_vocab`: the vocabulary the graph was exported with.
pub fn embedded_vocab(model: &dyn Model) -> Option<HashMap<char, i64>> {
    let config = model.metadata(session::CONFIG_KEY)?;
    match tokenizer::parse_config_vocab(config) {
        Ok(vocab) => (!vocab.is_empty()).then_some(vocab),
        Err(error) => {
            tracing::warn!(
                "Ignoring unreadable {} metadata: {}",
                session::CONFIG_KEY,
                error.0
            );
            None
        }
    }
}

#[path = "kokoro/tests.rs"]
#[cfg(test)]
mod tests;

//! Port of `kokoro_onnx.tokenizer` and the vocabulary sources (F4-04).

use std::collections::HashMap;
use std::path::PathBuf;

use ez_core::pyloads;

use super::vocab;
use super::KokoroError;
use crate::phonemes::{EspeakConfig, EspeakPhonemizer, Phonemizer};

/// `vocab_config`: a `config.json` path or its `config["vocab"]` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VocabConfig {
    File(PathBuf),
    Table(HashMap<char, i64>),
}

/// `Tokenizer`: phonemes in, token ids out.
pub struct Tokenizer {
    phonemizer: Box<dyn Phonemizer>,
    vocab: HashMap<char, i64>,
}

impl Tokenizer {
    /// Builds the espeak phonemizer and the vocabulary (`DEFAULT_VOCAB` last).
    pub fn new(
        espeak: &EspeakConfig,
        vocab: Option<HashMap<char, i64>>,
    ) -> Result<Self, KokoroError> {
        let phonemizer = EspeakPhonemizer::new(espeak)?;
        Ok(Self::with_phonemizer(Box::new(phonemizer), vocab))
    }

    /// Injection seam for tests and for callers with their own phonemizer.
    pub fn with_phonemizer(
        phonemizer: Box<dyn Phonemizer>,
        vocab: Option<HashMap<char, i64>>,
    ) -> Self {
        let vocab = match vocab {
            Some(vocab) if !vocab.is_empty() => vocab,
            _ => vocab::default_vocab(),
        };
        Self { phonemizer, vocab }
    }

    pub fn vocab(&self) -> &HashMap<char, i64> {
        &self.vocab
    }

    /// `normalize_text`: `text.strip()`.
    pub fn normalize_text(text: &str) -> &str {
        ez_core::pyvalue::py_strip(text)
    }

    /// `tokenize`: drop every character outside the vocabulary.
    pub fn tokenize(&self, phonemes: &str, limit: Option<i64>) -> Result<Vec<i64>, KokoroError> {
        if let Some(limit) = limit {
            if ez_core::pyvalue::char_len(phonemes) as i64 > limit {
                return Err(KokoroError(format!(
                    "text is too long, must be less than {limit} phonemes"
                )));
            }
        }
        Ok(phonemes
            .chars()
            .filter_map(|character| self.vocab.get(&character).copied())
            .collect())
    }

    /// `known`: the phonemes that survive tokenization, one to one with it.
    pub fn known(&self, phonemes: &str) -> String {
        phonemes
            .chars()
            .filter(|character| self.vocab.contains_key(character))
            .collect()
    }

    /// `phonemize`: espeak output restricted to the vocabulary, then stripped.
    pub fn phonemize(&self, text: &str, lang: &str, norm: bool) -> Result<String, KokoroError> {
        let text = if norm {
            Self::normalize_text(text)
        } else {
            text
        };
        let phonemes = self.phonemizer.phonemize(text, lang)?;
        let kept: String = phonemes
            .chars()
            .filter(|character| self.vocab.contains_key(character))
            .collect();
        Ok(ez_core::pyvalue::py_strip(&kept).to_string())
    }
}

/// `_load_vocab`: a `config.json` with `vocab`, or the given table.
pub fn load_vocab(config: Option<VocabConfig>) -> Result<Option<HashMap<char, i64>>, KokoroError> {
    let vocab = match config {
        None => None,
        Some(VocabConfig::Table(vocab)) => Some(vocab),
        Some(VocabConfig::File(path)) => {
            let text = std::fs::read_to_string(&path)
                .map_err(|error| KokoroError(python_os_error(&path, &error)))?;
            Some(parse_config_vocab(&text)?)
        }
    };
    Ok(vocab.filter(|vocab| !vocab.is_empty()))
}

/// Texto de `OSError` de CPython para los errores de archivo que cubre el oráculo.
fn python_os_error(path: &std::path::Path, error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => {
            format!("[Errno 2] No such file or directory: '{}'", path.display())
        }
        _ => error.to_string(),
    }
}

/// `config["vocab"]` of a JSON text, without naming `serde_json::Value`.
pub fn parse_config_vocab(text: &str) -> Result<HashMap<char, i64>, KokoroError> {
    let config = pyloads::loads(text).map_err(|error| KokoroError(error.to_string()))?;
    let vocab = config
        .get("vocab")
        .ok_or_else(|| KokoroError("'vocab'".to_string()))?;
    let entries = vocab
        .as_object()
        .ok_or_else(|| KokoroError("vocab is not an object".to_string()))?;
    let mut parsed = HashMap::new();
    for (key, id) in entries {
        let mut characters = key.chars();
        let (Some(character), None) = (characters.next(), characters.next()) else {
            continue;
        };
        if let Some(id) = id.as_i64() {
            parsed.insert(character, id);
        }
    }
    Ok(parsed)
}

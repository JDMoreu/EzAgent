//! TTS local de EzAgent: fonemas de espeak-ng, inferencia Kokoro y voces.
//!
//! Puerto de `kokoro_onnx` (fonemizador, tokenizador, inferencia y procesado
//! de audio) y del estilo de voz de `src/ezagent/voice/tts.py` (F4-04). La
//! reproducción y la cola de frases son de F4-05.

pub mod kokoro;
pub mod phonemes;
pub mod queue;
pub mod speaker;
pub mod voices;

pub use kokoro::{
    Kokoro, KokoroError, SynthesisOptions, Tokenizer, Voice, MAX_PHONEME_LENGTH, SAMPLE_RATE,
};
pub use phonemes::{EspeakConfig, PhonemeError, Phonemizer};
pub use voices::{voice_style, Style, Voices};

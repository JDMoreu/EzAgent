//! Transcripción con whisper.cpp: arranca el servidor si hace falta y guarda el
//! proceso para cerrarlo al salir de la app.

use std::process::Child;
use std::sync::Arc;

use ez_core::config::VoiceConfig;
use ez_stt::client;
use ez_stt::server::ensure_whisper;
use parking_lot::Mutex;

use super::{NoteError, Transcriber};

/// Cliente de transcripción; el `Child` se comparte con el dueño de la app.
pub struct WhisperTranscriber {
    config: VoiceConfig,
    whisper: Arc<Mutex<Option<Child>>>,
}

impl WhisperTranscriber {
    pub fn new(config: VoiceConfig, whisper: Arc<Mutex<Option<Child>>>) -> Self {
        Self { config, whisper }
    }
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(&mut self, wav: &[u8]) -> Result<String, NoteError> {
        let mut config = self.config.clone();
        let started = ensure_whisper(&mut config);
        if let Some(mut process) = started {
            let mut slot = self.whisper.lock();
            if slot.is_some() {
                let _ = process.kill();
            } else {
                *slot = Some(process);
            }
        }
        client::transcribe(&config, wav, None)
            .map_err(|error| NoteError::Transcribe(error.message().to_string()))
    }
}

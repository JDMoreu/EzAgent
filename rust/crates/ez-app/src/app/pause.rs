//! Port of `src/ezagent/voice/app.py 3397-3493` (F6-07).
//!
//! Pausa y reactivación: cierre suave tras una respuesta, silencio del
//! micrófono, la frase de reactivación y la comprobación en segundo plano.

use std::sync::atomic::Ordering;

use ez_audio::capture::pcm_to_wav;
use ez_audio::wake::{is_wake_phrase, worth_transcribing, DEFAULT_WAKE_PHRASE};
use ez_core::pyvalue::text_or_empty;
use ez_stt::client::transcribe;

use super::core::{cfg_truthy, log, AppEvent, VoiceApp, LISTENING};

impl VoiceApp {
    /// `_request_close`: la respuesta de este turno cierra la escucha.
    pub fn request_close(&self) {
        self.inner
            .turn
            .close_after_answer
            .store(true, Ordering::SeqCst);
    }

    /// `_close_action`: solo `pause` y `hangup`; lo demás es `pause`.
    pub fn close_action(&self) -> String {
        let cfg = self.config();
        let value = text_or_empty(cfg.get("close_action")).to_lowercase();
        match value.as_str() {
            "pause" | "hangup" => value,
            _ => "pause".to_string(),
        }
    }

    /// `_pause_message`: texto de la pausa; lo genera el código, nunca el LLM.
    pub fn pause_message(&self) -> String {
        let cfg = self.config();
        if !cfg_truthy(&cfg, "voice_wake", true) {
            return "En pausa. Toca el micrófono para volver.".to_string();
        }
        let phrase = text_or_empty(cfg.get("wake_phrase"));
        let phrase = if phrase.is_empty() {
            DEFAULT_WAKE_PHRASE.to_string()
        } else {
            phrase
        };
        format!("En pausa. Di «{phrase}» cuando me necesites.")
    }

    /// `_close_listening`: cierra la escucha; `true` si la llamada sigue (pausa).
    pub fn close_listening(&self) -> bool {
        let stored = {
            let mut mode = self.inner.turn.close_mode.write();
            std::mem::take(&mut *mode)
        };
        let mode = if stored.is_empty() {
            self.close_action()
        } else {
            stored
        };
        if mode != "pause" || !self.inner.session.in_call.load(Ordering::SeqCst) {
            self.hang_up();
            return false;
        }
        self.set_mic_muted(true, "cierre");
        self.set_state(LISTENING);
        true
    }

    /// `_llm_mic_mute`: la herramienta del LLM solo deja la escucha en pausa.
    pub fn llm_mic_mute(&self, muted: bool) {
        if !muted {
            log("El modelo pidió activar el micro; ya está activo.");
            return;
        }
        self.events()
            .put(AppEvent::CloseRequest("pause".to_string()));
    }

    /// `_maybe_wake`: en pausa, solo una frase corta busca la reactivación.
    pub fn maybe_wake(&self, pcm: &[u8], rate: u32) {
        let cfg = self.config();
        if !self.inner.session.in_call.load(Ordering::SeqCst)
            || !cfg_truthy(&cfg, "voice_wake", false)
        {
            return;
        }
        if !worth_transcribing(pcm, rate) {
            return;
        }
        // DECISION: el lock de Python se adquiere sin bloquear y se pasa al
        // hilo; aquí se mantiene hasta crear el hilo y se vuelve a tomar en él.
        // La ventana entre ambos es mínima y solo podría duplicar una
        // comprobación de la frase. `try_lock` evita bloquear el hilo de audio.
        let Some(held) = self.inner.turn.wake_lock.try_lock() else {
            return;
        };
        let app = self.clone();
        let pcm = pcm.to_vec();
        self.start_worker("ezagent-wake", move || app.wake_worker(&pcm, rate));
        drop(held);
    }

    /// `_wake_worker`: transcribe la frase en pausa y avisa si era la clave.
    fn wake_worker(&self, pcm: &[u8], rate: u32) {
        let _guard = self.inner.turn.wake_lock.lock();
        let cfg = self.config();
        let text = match transcribe(&cfg, &pcm_to_wav(pcm, rate), None) {
            Ok(text) => {
                super::core::trace::notify(&self.inner.runtime.trace, |trace| {
                    trace.stt_transcript(&text);
                });
                text
            }
            Err(_) => {
                // Python usa `type(exc).__name__`; el error de `ez-stt` es STTError.
                log("Pausa: no pude comprobar la frase (STTError).");
                return;
            }
        };
        let phrase = text_or_empty(cfg.get("wake_phrase"));
        if is_wake_phrase(&text, &phrase) {
            self.events().put(AppEvent::Wake);
        }
    }

    /// `_on_wake`: la frase de reactivación devuelve el micrófono.
    pub fn on_wake(&self) {
        if !self.inner.session.in_call.load(Ordering::SeqCst)
            || !self.inner.session.muted.load(Ordering::SeqCst)
        {
            return;
        }
        log("Frase de reactivación: vuelvo a escuchar.");
        self.set_mic_muted(false, "frase");
        self.speak("Dime.", "answer");
    }

    /// `_set_mic_muted`: un solo punto para silenciar y reactivar.
    pub fn set_mic_muted(&self, muted: bool, source: &str) {
        self.inner.session.muted.store(muted, Ordering::SeqCst);
        self.inner.ui.hud.set_muted(muted);
        self.inner.audio.reset_capture.set();
        self.inner
            .session
            .ignore_utterance
            .store(muted, Ordering::SeqCst);
        let origin = if source.is_empty() {
            "sin origen"
        } else {
            source
        };
        let state = if muted { "silenciado" } else { "activo" };
        log(format!("Micrófono {state} ({origin})."));
        self.update_tray_title();
    }
}

/// `bool(value)` para los textos de configuración que podrían venir de fuera.
#[cfg(test)]
mod tests;

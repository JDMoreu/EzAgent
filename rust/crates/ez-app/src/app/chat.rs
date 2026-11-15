//! Port of `src/ezagent/voice/app.py 2014-2173` (F6-05).
//!
//! Chat escrito: abrir la ventana, cambiar de conversación, detener el turno y
//! el turno del LLM en modo chat (`ezagent-chat`). `_speech_queue` y
//! `_tts_worker` ya están portados en F6-04 y se reutilizan.

#[cfg(test)]
mod tests;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use ez_chat::speech::spoken;
use ez_chat::store::Source;
use ez_core::config::VoiceConfig;
use ez_core::pyvalue::{py_lstrip, py_strip, text_or_empty};
use ez_llm::errors::{friendly_llm_error, LlmError};
use ez_llm::messenger::TurnError;
use ez_tts::queue::SpeechQueue;
use serde_json::Value;

use crate::sentences::Sentences;

use super::core::state::Flag;
use super::core::{advance_turn, cfg_text, log, AppEvent, VoiceApp, LISTENING, THINKING};
use super::events::turn_gen;
use super::states::StopBridge;

/// `self._llm_label()`: nombre visible del proveedor activo.
// DECISION: `_llm_label` es de F6-07; el turno del chat necesita el mismo texto
// de error y `workers::llm_label` no es visible desde aquí, así que se repite.
fn llm_label(cfg: &VoiceConfig) -> String {
    let provider = text_or_empty(cfg.get("llm_provider"));
    ez_core::llm_providers::provider_spec(Some(&provider)).map_or_else(
        || {
            if provider.is_empty() {
                "LLM".to_string()
            } else {
                provider
            }
        },
        |spec| spec.label.to_string(),
    )
}

/// `type(exc).__name__` para el log del fallo del turno.
fn turn_error_name(error: &TurnError) -> &str {
    match error {
        TurnError::Cancelled(_) => "TurnCancelled",
        TurnError::Timeout(_) => "LLMTurnTimeout",
        TurnError::Llm(_) => "LlmError",
        TurnError::Runtime(failure) => &failure.kind,
    }
}

/// Error del proveedor equivalente al de Python para `friendly_llm_error`.
fn llm_error_of(error: &TurnError) -> LlmError {
    match error {
        TurnError::Timeout(message) => LlmError::turn_timeout(message.clone()),
        TurnError::Llm(error) => error.clone(),
        TurnError::Runtime(failure) => LlmError::other(&failure.message),
        TurnError::Cancelled(message) => LlmError::other(message.clone()),
    }
}

/// Estado del turno del chat mientras llega el flujo del LLM.
struct ChatRun<'a> {
    app: &'a VoiceApp,
    generation: i64,
    stop: &'a Arc<Flag>,
    message: i64,
    speech: Option<&'a SpeechQueue>,
    sentences: Sentences,
    rich: bool,
    spoken_any: bool,
    got_text: bool,
    finished: bool,
    outcome: &'static str,
}

impl ChatRun<'_> {
    /// `gen != self._turn_gen or stop_event.is_set()`.
    fn cancelled(&self) -> bool {
        self.generation != turn_gen(self.app) || self.stop.is_set()
    }

    /// Una frase lista: se dice solo si queda algo tras quitar el markdown.
    fn say_sentence(&mut self, sentence: &str) {
        let Some(speech) = self.speech else {
            return;
        };
        let text = spoken(sentence);
        let text = py_strip(&text);
        if !text.is_empty() {
            speech.say(text);
            self.spoken_any = true;
        }
    }

    /// Un trozo del modelo al chat; las frases se dicen mientras llegan.
    fn append(&mut self, delta: &str) {
        self.got_text |= !py_strip(delta).is_empty();
        self.app.inner.chat.chat.append(self.message, delta);
        if self.speech.is_none() || self.rich {
            return;
        }
        if delta.contains("```") || delta.contains("\n|") || py_lstrip(delta).starts_with('|') {
            // Tablas y bloques: no se leen en voz alta.
            self.rich = true;
            return;
        }
        for sentence in self.sentences.feed(delta) {
            self.say_sentence(&sentence);
        }
    }

    fn stream(&mut self, text: &str) -> &'static str {
        let messenger = self.app.inner.agents.messenger.lock().clone();
        let Some(messenger) = messenger else {
            let error = LlmError::other("'NoneType' object has no attribute 'ask_stream'");
            self.fail("AttributeError", &error);
            return self.outcome;
        };
        let mode = ez_llm::messenger::AskMode::Chat;
        for item in messenger.ask_stream(text, mode) {
            match item {
                Ok(delta) => {
                    if self.cancelled() {
                        self.outcome = "cancelled";
                        return self.outcome;
                    }
                    self.append(&delta);
                }
                Err(error) if error.is_cancelled() => {
                    self.outcome = "cancelled";
                    log("Turno del chat cancelado por el usuario.");
                    return self.outcome;
                }
                Err(error) => {
                    let name = turn_error_name(&error);
                    let llm_error = llm_error_of(&error);
                    self.fail(name, &llm_error);
                    return self.outcome;
                }
            }
        }
        if !self.got_text {
            self.app.inner.chat.chat.fail(
                self.message,
                "El modelo no devolvió respuesta. Prueba de nuevo.",
            );
            self.finished = true;
            self.outcome = "empty";
            return self.outcome;
        }
        self.complete()
    }

    /// Respuesta completa: se guarda, se resume y se dice lo que quede.
    fn complete(&mut self) -> &'static str {
        self.app.inner.chat.chat.finish(self.message);
        self.finished = true;
        let answer = self.app.inner.chat.chat.last_assistant_text();
        let spoken_answer = if answer.is_empty() {
            String::new()
        } else {
            spoken(&answer)
        };
        if !answer.is_empty() {
            *self.app.inner.session.last_summary.write() = if spoken_answer.is_empty() {
                answer.clone()
            } else {
                spoken_answer
            };
        }
        if self.speech.is_some() && self.generation == turn_gen(self.app) {
            self.finish_speech(&answer);
        }
        self.outcome
    }

    /// Parte hablada del cierre: aviso de lo que queda en pantalla o frases.
    fn finish_speech(&mut self, answer: &str) {
        if self.rich {
            if self.spoken_any {
                self.say_sentence("El resto está en el chat.");
            } else {
                self.say_sentence(answer);
            }
        } else {
            for sentence in self.sentences.flush() {
                self.say_sentence(&sentence);
            }
        }
        *self.app.inner.speech.speaking_kind.write() = Some("answer".to_string());
        if self.spoken_any {
            self.app.events().put(AppEvent::Speaking(self.generation));
        }
        let Some(speech) = self.speech else {
            return;
        };
        if let Err(error) = speech.finish(None) {
            self.fail_speech(&error);
        }
    }

    /// `speech.finish()` dentro del `try`: su fallo se trata como el del LLM.
    // DECISION: Python deja que la excepción de Kokoro la atrape el `except`
    // genérico y la pase por `friendly_llm_error`; aquí se conserva ese efecto
    // aunque el error llegue como texto y no como excepción.
    fn fail_speech(&mut self, message: &str) {
        let error = LlmError::other(message);
        self.fail("RuntimeError", &error);
    }

    /// `except Exception`: falla el chat y registra el motivo del proveedor.
    fn fail(&mut self, name: &str, error: &LlmError) {
        self.outcome = "error";
        let cfg = self.app.config();
        log(format!(
            "Chat ({}): {name}: {}",
            cfg_text(&cfg, "llm_provider", ""),
            error.message
        ));
        let reason = friendly_llm_error(error, &llm_label(&cfg), &cfg_text(&cfg, "llm_model", ""));
        self.app.inner.chat.chat.fail(self.message, &reason);
        self.finished = true;
    }
}

/// `Path(value).expanduser()`: solo expande `~` y `~/…` (no variables de entorno).
fn expand_user(root: &str) -> std::path::PathBuf {
    let Some(rest) = root.strip_prefix('~') else {
        return root.into();
    };
    if !(rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\')) {
        return root.into();
    }
    std::path::PathBuf::from(format!("{}{rest}", ez_core::dirs::home().display()))
}

impl VoiceApp {
    /// `_toggle_chat`: el chat vive en la barra (botón 💬 o atajo).
    pub fn toggle_chat(&self) {
        // DECISION: `DropUI::toggle_chat` no devuelve error; el `try/except`
        // del Python solo registraba el fallo al abrir la ventana.
        self.inner.ui.hud.toggle_chat();
    }

    /// `_set_chat_voice`: aplica el altavoz del chat a los turnos y lo guarda.
    pub fn set_chat_voice(&self, enabled: bool) {
        self.inner
            .config
            .cfg
            .write()
            .set("chat_voice", Value::Bool(enabled));
        self.inner.chat.chat.set_voice_replies(enabled);
        let saved = self.inner.config.cfg.read().save_key(
            "chat_voice",
            Value::Bool(enabled),
            Some(&self.inner.config.config_file),
        );
        if let Err(error) = saved {
            log(format!("No pude guardar chat_voice: {error}"));
        }
    }

    /// `_chat_media_roots`: carpetas de lectura del agente para el chat.
    pub fn chat_media_roots(&self) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
        let config = self
            .inner
            .agents
            .agent
            .lock()
            .as_ref()
            .map(ez_agent::runtime::AgentRuntime::config);
        let Some(config) = config else {
            return (vec![ez_core::dirs::home()], Vec::new());
        };
        let mut roots: Vec<std::path::PathBuf> = config
            .paths
            .read_roots
            .iter()
            .map(|root| expand_user(root))
            .collect();
        roots.push(config.workspace.clone());
        roots.push(config.app_dir.clone());
        (roots, vec![config.workspace])
    }

    /// `_chat_stop`: el usuario pulsó «detener»; se corta la voz y la respuesta.
    pub fn chat_stop(&self) {
        advance_turn(self);
        self.signal_tts_stop();
        *self.inner.speech.speaking_kind.write() = None;
        if self.inner.session.in_call.load(Ordering::SeqCst) {
            self.set_state(LISTENING);
        } else {
            self.set_idle();
        }
    }

    /// `_chat_new`: conversación nueva sin decirlo en voz alta.
    pub fn chat_new(&self) {
        advance_turn(self);
        self.signal_tts_stop();
        *self.inner.speech.speaking_kind.write() = None;
        self.inner.chat.contexts.lock().new_chat();
        // El historial anota la sesión nueva (`ChatStore.new_session`).
        super::core::trace::notify(&self.inner.runtime.trace, |trace| {
            trace.chat_file_write("");
        });
        *self.inner.session.last_summary.write() = String::new();
        log("Conversación nueva desde el chat.");
    }

    /// `_chat_switch`: retoma otra conversación con su contexto del modelo.
    pub fn chat_switch(&self, session: i64) {
        if self.inner.chat.chat.busy() {
            return;
        }
        advance_turn(self);
        self.signal_tts_stop();
        *self.inner.speech.speaking_kind.write() = None;
        let switched = self.inner.chat.contexts.lock().switch(session);
        if switched {
            // El historial anota la conversación retomada (`ChatStore.resume`).
            super::core::trace::notify(&self.inner.runtime.trace, |trace| {
                trace.chat_file_write("");
            });
            let last = self.inner.chat.chat.last_assistant_text();
            *self.inner.session.last_summary.write() = if last.is_empty() {
                String::new()
            } else {
                spoken(&last)
            };
            log(format!("Conversación {session} retomada desde el chat."));
        }
    }

    /// `_chat_delete`: borra una conversación (y su contexto) a petición del usuario.
    pub fn chat_delete(&self, session: i64) {
        if self.inner.chat.contexts.lock().delete(session) {
            if self.inner.chat.chat.current_session() == Some(session) {
                *self.inner.session.last_summary.write() = String::new();
            }
            log(format!("Conversación {session} borrada del historial."));
        }
    }

    /// `_chat_send`: un mensaje escrito es un turno más, como uno hablado.
    pub fn chat_send(&self, text: &str) {
        let text = py_strip(text);
        if text.is_empty() {
            return;
        }
        let pending = self
            .inner
            .agents
            .agent
            .lock()
            .as_ref()
            .is_some_and(ez_agent::runtime::AgentRuntime::confirm_pending);
        if pending {
            // «sí» o «confirmo 3» responde a la confirmación pendiente.
            self.inner.chat.chat.add_user(text, Source::Chat);
            if let Some(agent) = self.inner.agents.agent.lock().as_ref() {
                agent.submit_confirmation(text, None);
            }
            return;
        }
        advance_turn(self);
        let generation = turn_gen(self);
        self.signal_tts_stop();
        *self.inner.speech.speaking_kind.write() = None;
        self.inner.chat.chat.add_user(text, Source::Chat);
        log(format!("Chat: {}", ez_core::pyvalue::head_chars(text, 80)));
        self.set_state(THINKING);
        let app = self.clone();
        let text = text.to_string();
        self.start_worker("ezagent-chat", move || {
            app.chat_worker(&text, generation);
        });
    }

    /// `_chat_worker`: el texto llega en streaming; solo habla si se quiere.
    fn chat_worker(&self, text: &str, generation: i64) {
        let stop = self.new_tts_stop();
        let message = self.inner.chat.chat.begin_assistant(Source::Chat);
        let voice = self.inner.chat.chat.voice_replies();
        let bridge = voice.then(|| StopBridge::new(&stop));
        let speech = bridge
            .as_ref()
            .map(|bridge| self.speech_queue(bridge.atomic(), None));
        let mut run = ChatRun {
            app: self,
            generation,
            stop: &stop,
            message: message.id,
            speech: speech.as_ref(),
            sentences: Sentences::default(),
            rich: false,
            spoken_any: false,
            got_text: false,
            finished: false,
            outcome: "ok",
        };
        let outcome = run.stream(text);
        // `finally`: un turno interrumpido conserva lo que llegó.
        if !run.finished {
            self.inner.chat.chat.cancel(message.id);
            if let Some(speech) = &speech {
                speech.abort();
            }
        }
        self.events()
            .put(AppEvent::AnswerDone(generation, outcome.to_string()));
    }

    /// `_speak_busy`: aviso de espera sin tocar el stop del turno que espera.
    pub fn speak_busy(&self, text: &str, generation: i64) {
        if self.inner.speech.busy_speaking.is_set() {
            return;
        }
        let stop = Arc::new(Flag::new());
        *self.inner.speech.busy_stop.lock() = Some(Arc::clone(&stop));
        self.inner.speech.busy_barged.store(false, Ordering::SeqCst);
        self.inner.speech.busy_speaking.set();
        let app = self.clone();
        let text = text.to_string();
        self.start_worker("ezagent-busy", move || {
            let bridge = StopBridge::new(&stop);
            let atomic = bridge.atomic();
            let outcome = app.inner.speech.speaker.speak(&text, &atomic, None, None);
            if let Err(error) = outcome {
                log(format!("Aviso de espera: {error}"));
            }
            app.events().put(AppEvent::BusyDone(generation));
        });
    }
}

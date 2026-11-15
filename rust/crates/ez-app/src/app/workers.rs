//! Port of `src/ezagent/voice/app.py 1870-2013, 2175-2212` (F6-04).
//!
//! Transcripción del turno (`_start_stt`, `_stt_worker`), respuesta hablada
//! (`_answer_worker`) y la cola de voz que usa (`_speech_queue`). El hilo de
//! respuesta es `ezagent-answer` (F6-03 lo lanza) y el del LLM, `ezagent-turn`.

#[cfg(test)]
mod tests;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use ez_audio::capture::{pcm_is_weak, pcm_to_wav};
use ez_chat::store::Source;
use ez_core::metrics::TurnTimings;
use ez_core::pyvalue::{head_chars, py_strip, repr_str, text_or_empty};
use ez_llm::errors::{friendly_llm_error, LlmError};
use ez_llm::messenger::{AskMode, Messenger, TurnError};
use ez_stt::client::{is_hallucination, transcribe};
use ez_tts::queue::{SpeechQueue, SpeechQueueOptions};

use crate::sentences::Sentences;

use super::core::state::{Flag, PendingStt};
use super::core::trace;
use super::core::{cfg_text, is_filler_sentence, log, timed_stream, AppEvent, VoiceApp, SPEAKING};
use super::events::turn_gen;
use super::states::StopBridge;

/// `self._llm_label()`: nombre visible del proveedor activo.
// DECISION: `_llm_label` es de F6-07; aquí se necesita para el texto de error y
// se deja como función libre para que esa tarea la reutilice.
fn llm_label(cfg: &ez_core::config::VoiceConfig) -> String {
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
        // DECISION: el cliente no expone el nombre de la clase del SDK; el
        // texto de usuario sale igual por `friendly_llm_error`.
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
        // Ya se atiende como «cancelled» antes de llegar aquí.
        TurnError::Cancelled(message) => LlmError::other(message.clone()),
    }
}

/// `outcome == "ok" and not "".join(full).strip()` del `finally`.
fn final_outcome(outcome: &'static str, answer: &str) -> &'static str {
    if outcome == "ok" && answer.is_empty() {
        "empty"
    } else {
        outcome
    }
}

/// Estado del turno de respuesta mientras llega el flujo del LLM.
struct AnswerRun {
    sentences: Sentences,
    full: Vec<String>,
    started: bool,
    first_token: bool,
    first_audio: bool,
    opening_checked: bool,
    chat_done: bool,
    finished: bool,
    outcome: &'static str,
}

/// Datos fijos del turno de respuesta.
struct AnswerTurn<'a> {
    app: &'a VoiceApp,
    generation: i64,
    stop: &'a Arc<Flag>,
    timing: Option<&'a Arc<TurnTimings>>,
    speech: Option<&'a SpeechQueue>,
    chat_id: i64,
}

impl AnswerTurn<'_> {
    /// `gen != self._turn_gen or stop_event.is_set()`.
    fn cancelled(&self) -> bool {
        self.generation != turn_gen(self.app) || self.stop.is_set()
    }
}

impl AnswerRun {
    fn new() -> Self {
        Self {
            sentences: Sentences::default(),
            full: Vec::new(),
            started: false,
            first_token: false,
            first_audio: false,
            opening_checked: false,
            chat_done: false,
            finished: false,
            outcome: "ok",
        }
    }

    fn stream(&mut self, turn: &AnswerTurn<'_>, text: &str) {
        let messenger = turn.app.inner.agents.messenger.lock().clone();
        let Some(messenger) = messenger else {
            let error = LlmError::other("'NoneType' object has no attribute 'ask_stream'");
            self.fail(turn, "AttributeError", &error);
            return;
        };
        if let Some(timing) = turn.timing {
            timing.mark("llm_start", None);
        }
        let stream = messenger.ask_stream(text, AskMode::Voice);
        for item in timed_stream(stream, turn.timing.cloned()) {
            let delta = match item {
                Ok(delta) => delta,
                Err(error) => {
                    let name = turn_error_name(&error);
                    let llm_error = llm_error_of(&error);
                    if error.is_cancelled() {
                        self.outcome = "cancelled";
                        log("Turno cancelado por el usuario.");
                    } else {
                        self.fail(turn, name, &llm_error);
                    }
                    return;
                }
            };
            if turn.cancelled() {
                self.outcome = "cancelled";
                return;
            }
            if let Some(timing) = turn.timing {
                if !self.first_token && !py_strip(&delta).is_empty() {
                    timing.mark("first_token", None);
                    self.first_token = true;
                }
            }
            self.full.push(delta.clone());
            turn.app.inner.chat.chat.append(turn.chat_id, &delta);
            if turn.speech.is_some() {
                let ready = self.sentences.feed(&delta);
                if !self.say_all(turn, ready) {
                    return;
                }
            }
        }
        if turn.speech.is_some() {
            let pending = self.sentences.flush();
            if !self.say_all(turn, pending) {
                return;
            }
        }
        self.finish(turn, &messenger);
    }

    fn say_all(&mut self, turn: &AnswerTurn<'_>, sentences: Vec<String>) -> bool {
        let Some(speech) = turn.speech else {
            return true;
        };
        for sentence in sentences {
            if turn.cancelled() {
                self.outcome = "cancelled";
                return false;
            }
            if !self.started {
                self.started = true;
                turn.app.events().put(AppEvent::Speaking(turn.generation));
            }
            self.say(turn, speech, &sentence);
        }
        true
    }

    /// Una frase lista: la primera muletilla se calla, y se marca el primer audio.
    fn say(&mut self, turn: &AnswerTurn<'_>, speech: &SpeechQueue, sentence: &str) {
        if !self.opening_checked {
            self.opening_checked = true;
            if is_filler_sentence(sentence) {
                return;
            }
        }
        if let Some(timing) = turn.timing {
            if !self.first_audio {
                timing.mark("first_sentence", None);
                self.first_audio = true;
            }
        }
        // DECISION: `say` solo encola; los fallos de Kokoro llegan en `finish`.
        speech.say(sentence);
    }

    fn finish(&mut self, turn: &AnswerTurn<'_>, messenger: &Messenger) {
        let answer = py_strip(&self.full.join("")).to_string();
        turn.app.inner.chat.chat.finish(turn.chat_id);
        self.chat_done = true;
        if !answer.is_empty() {
            *turn.app.inner.session.last_summary.write() = answer.clone();
            log(format!("Mensajero: {}", head_chars(&answer, 600)));
        }
        let rounds = messenger.last_rounds();
        if rounds.len() > 1 {
            log(format!("LLM por llamada: {}", rounds.join(" | ")));
        }
        match turn.speech {
            Some(speech) => match speech.finish(None) {
                Ok(_) => self.finished = true,
                Err(error) => {
                    self.outcome = "error";
                    turn.app.events().put(AppEvent::Error(
                        format!("Kokoro: {error}"),
                        Some(turn.generation),
                    ));
                }
            },
            None => self.finished = true,
        }
    }

    /// `except Exception`: falla el chat y publica el motivo amable.
    fn fail(&mut self, turn: &AnswerTurn<'_>, name: &str, error: &LlmError) {
        self.outcome = "error";
        let cfg = turn.app.config();
        log(format!(
            "LLM ({}): {name}: {}",
            cfg_text(&cfg, "llm_provider", ""),
            error.message
        ));
        let reason = friendly_llm_error(error, &llm_label(&cfg), &cfg_text(&cfg, "llm_model", ""));
        turn.app.inner.chat.chat.fail(turn.chat_id, &reason);
        self.chat_done = true;
        turn.app
            .events()
            .put(AppEvent::Error(reason, Some(turn.generation)));
    }
}

impl VoiceApp {
    /// `_start_stt`: una sola transcripción en vuelo; la última sustituye.
    pub fn start_stt(&self, pcm: Vec<u8>, rate: u32, gen: i64, timing: Arc<TurnTimings>) {
        {
            let mut stt = self.inner.turn.stt.lock();
            if stt.busy {
                stt.pending = Some(PendingStt {
                    pcm,
                    rate,
                    generation: gen,
                    timing,
                });
                return;
            }
            stt.busy = true;
        }
        let app = self.clone();
        self.start_worker("ezagent-stt", move || {
            app.stt_worker(pcm, rate, gen, timing)
        });
    }

    /// `_stt_worker`: transcribe, descarta alucinaciones y publica el texto.
    fn stt_worker(&self, pcm: Vec<u8>, rate: u32, generation: i64, timing: Arc<TurnTimings>) {
        let cfg = self.config();
        match transcribe(&cfg, &pcm_to_wav(&pcm, rate), None) {
            Ok(text) => {
                trace::notify(&self.inner.runtime.trace, |trace| {
                    trace.stt_transcript(&text);
                });
                timing.mark("stt_ready", None);
                let text = if is_hallucination(&text, pcm_is_weak(&pcm, rate)) {
                    log(format!(
                        "STT: descarto posible alucinación de Whisper: {}",
                        repr_str(&text)
                    ));
                    String::new()
                } else {
                    text
                };
                self.events().put(AppEvent::Transcribed(text, generation));
            }
            // DECISION: el cliente solo devuelve `STTError`, así que la rama
            // genérica «STT: {exc}» del Python no tiene equivalente propio.
            Err(error) => self.events().put(AppEvent::Error(
                error.message().to_string(),
                Some(generation),
            )),
        }
        self.finish_stt();
    }

    /// `finally` de `_stt_worker`: suelta el turno y arranca el pendiente.
    fn finish_stt(&self) {
        let pending = {
            let mut stt = self.inner.turn.stt.lock();
            stt.busy = false;
            stt.pending.take()
        };
        if let Some(pending) = pending {
            if pending.generation == turn_gen(self) {
                self.start_stt(
                    pending.pcm,
                    pending.rate,
                    pending.generation,
                    pending.timing,
                );
            }
        }
    }

    /// `_answer_worker`: flujo del LLM al chat y a la voz del turno.
    pub fn answer_worker(&self, text: &str, generation: i64) {
        let stop = self.new_tts_stop();
        *self.inner.speech.speaking_kind.write() = Some("answer".to_string());
        let timing = if generation == turn_gen(self) {
            self.inner.turn.turn_timing.lock().clone()
        } else {
            None
        };
        let bridge = if self.inner.chat.chat.voice_replies() {
            Some(StopBridge::new(&stop))
        } else {
            None
        };
        let speech = bridge
            .as_ref()
            .map(|bridge| self.speech_queue(bridge.atomic(), timing.clone()));
        let message = self.inner.chat.chat.begin_assistant(Source::Voice);
        let mut run = AnswerRun::new();
        let turn = AnswerTurn {
            app: self,
            generation,
            stop: &stop,
            timing: timing.as_ref(),
            speech: speech.as_ref(),
            chat_id: message.id,
        };
        run.stream(&turn, text);
        if !run.chat_done {
            self.inner.chat.chat.cancel(message.id);
        }
        if !run.finished {
            if let Some(speech) = &speech {
                speech.abort();
            }
        }
        let answer = py_strip(&run.full.join("")).to_string();
        let outcome = final_outcome(run.outcome, &answer);
        self.events()
            .put(AppEvent::AnswerDone(generation, outcome.to_string()));
    }

    /// `_speech_queue`: sintetiza en este hilo y reproduce aparte.
    pub(crate) fn speech_queue(
        &self,
        stop: Arc<AtomicBool>,
        timing: Option<Arc<TurnTimings>>,
    ) -> SpeechQueue {
        let speaker = Arc::clone(&self.inner.speech.speaker);
        let hud = Arc::clone(&self.inner.ui.hud);
        let synth_speaker = Arc::clone(&speaker);
        let synthesize = move |text: &str| synth_speaker.synthesize(text);
        let play_speaker = speaker;
        let play_stop = Arc::clone(&stop);
        let play = move |samples: &[f32], rate: u32, on_start: &mut dyn FnMut()| {
            let mut on_level = |level: f64| hud.draw(level, Some(SPEAKING));
            play_speaker.play(
                samples,
                rate,
                &play_stop,
                Some(&mut on_level),
                Some(on_start),
            )
        };
        let on_sound = timing.as_ref().map(|timing| {
            let timing = Arc::clone(timing);
            Arc::new(move || {
                let seen = timing
                    .marks
                    .lock()
                    .map_or(true, |marks| marks.contains_key("first_sound"));
                if !seen {
                    timing.mark("first_sound", None);
                }
            }) as Arc<dyn Fn() + Send + Sync>
        });
        let on_synth = timing.as_ref().map(|timing| {
            let timing = Arc::clone(timing);
            Arc::new(move |secs: f64| timing.add_span("tts_synth", secs))
                as Arc<dyn Fn(f64) + Send + Sync>
        });
        let on_play = timing.as_ref().map(|timing| {
            let timing = Arc::clone(timing);
            Arc::new(move |secs: f64| timing.add_span("tts_play", secs))
                as Arc<dyn Fn(f64) + Send + Sync>
        });
        SpeechQueue::new(
            synthesize,
            play,
            stop,
            SpeechQueueOptions {
                on_sound,
                on_synth,
                on_play,
            },
        )
    }
}

//! Port of `src/ezagent/voice/app.py 1436-1673` (F6-03).
//!
//! `_start_call`, `_hang_up`, `_call_worker`, `_on_mic_level`, `_is_speaking`,
//! `_on_barge_in`, `_on_speech_start` y `_on_utterance`. El hilo de audio solo
//! publica intenciones; el hilo de la interfaz decide generación y estado.

#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ez_audio::capture::StreamHooks;
use ez_core::metrics::{monotonic, TurnTimings};

use super::core::state::Flag;
use super::core::{
    advance_turn, cfg_float, cfg_truthy, log, AppEvent, VoiceApp, LISTENING, RESUME_SECS, SPEAKING,
    THINKING,
};
use super::events::{confirm_pending, in_call, muted, turn_gen};

/// Espejo de los `threading.Event` que la captura espera como `AtomicBool`.
#[derive(Default)]
struct CaptureFlags {
    stop: AtomicBool,
    reset: AtomicBool,
}

impl VoiceApp {
    /// `_start_call`.
    pub fn start_call(&self) {
        if in_call(self) {
            return;
        }
        self.signal_tts_stop();
        *self.inner.speech.speaking_kind.write() = None;
        self.inner.session.in_call.store(true, Ordering::SeqCst);
        self.inner
            .session
            .user_speaking
            .store(false, Ordering::SeqCst);
        advance_turn(self);
        let stop = Arc::new(Flag::new());
        *self.inner.turn.call_stop.lock() = Some(Arc::clone(&stop));
        self.inner.ui.hud.set_in_call(true);
        self.set_state(LISTENING);
        log("Llamada iniciada. Habla cuando quieras; Ctrl+Alt+E para colgar.");
        // DECISION: los dobles de prueba no abren el micrófono real.
        if self.inner.runtime.background {
            let app = self.clone();
            match std::thread::Builder::new()
                .name("ezagent-call".to_string())
                .spawn(move || app.call_worker(stop))
            {
                Ok(handle) => *self.inner.turn.call_thread.lock() = Some(handle),
                Err(error) => log(format!("No pude crear el hilo ezagent-call: {error}")),
            }
        }
        // Una llamada nueva siempre empieza escuchando.
        if muted(self) {
            self.set_mic_muted(false, "llamada nueva");
        }
    }

    /// `_hang_up`.
    pub fn hang_up(&self) {
        if let Some(agent) = self.inner.agents.agent.lock().as_ref() {
            agent.cancel_confirmation();
        }
        self.inner.ui.hud.hide_confirmation();
        if !in_call(self) {
            return;
        }
        self.inner.session.in_call.store(false, Ordering::SeqCst);
        self.inner.ui.hud.set_mic_level(0.0);
        self.inner
            .session
            .user_speaking
            .store(false, Ordering::SeqCst);
        if let Some(stop) = self.inner.turn.call_stop.lock().as_ref() {
            stop.set();
        }
        if self.inner.speech.speaking_kind.read().as_deref() == Some("announce") {
            // Un aviso de agente en curso se entrega aunque cuelgues.
            log("Llamada terminada; termino el aviso en curso.");
            return;
        }
        advance_turn(self);
        self.signal_tts_stop();
        *self.inner.speech.speaking_kind.write() = None;
        self.inner.ui.hud.set_in_call(false);
        self.set_idle();
        log("Llamada terminada.");
        let app = self.clone();
        self.inner
            .ui
            .ui
            .schedule(Duration::from_millis(200), move || {
                app.pump_announcements();
            });
    }

    /// `_call_worker`: captura continua hasta que cuelguen.
    pub fn call_worker(&self, stop: Arc<Flag>) {
        let flags = Arc::new(CaptureFlags::default());
        let done = Arc::new(Flag::new());
        let bridge = self.spawn_flag_bridge(&stop, &done, Arc::clone(&flags));
        let check_flags = Arc::clone(&flags);
        let reset_flag = Arc::clone(&self.inner.audio.reset_capture);
        let app = self.clone();
        // `MicCapture::stream` espera `AtomicBool`; se refleja el `Flag` de la
        // app en cada bloque de audio (30 ms) y el puente cubre un micrófono
        // que deje de entregar bloques.
        let check = move || {
            if stop.is_set() {
                check_flags.stop.store(true, Ordering::SeqCst);
            }
            if reset_flag.is_set() {
                reset_flag.clear();
                check_flags.reset.store(true, Ordering::SeqCst);
            }
            app.is_speaking()
        };
        let speech_events = Arc::clone(&self.inner.ui.events);
        let utterance_events = Arc::clone(&self.inner.ui.events);
        let barge_events = Arc::clone(&self.inner.ui.events);
        let mut on_level = |level: f64| self.on_mic_level(level);
        let mut on_speech = |level: f64| speech_events.put(AppEvent::AudioSpeechStart(level));
        let mut on_utterance = |pcm: &[u8], rate: u32| {
            utterance_events.put(AppEvent::AudioUtterance(pcm.to_vec(), rate))
        };
        let mut on_barge = |level: f64| barge_events.put(AppEvent::AudioBargeIn(level));
        let hooks = StreamHooks {
            on_level: Some(&mut on_level),
            on_speech_start: Some(&mut on_speech),
            on_utterance: Some(&mut on_utterance),
            on_barge_in: Some(&mut on_barge),
        };
        let result = self.inner.audio.mic.lock().stream(
            &flags.stop,
            hooks,
            Some(&flags.reset),
            Some(&check as &dyn Fn() -> bool),
            0.0,
        );
        done.set();
        if let Ok(handle) = bridge {
            let _ = handle.join();
        }
        if let Err(error) = result {
            self.inner.ui.events.put(AppEvent::Error(
                format!("No puedo usar el micrófono: {error}"),
                None,
            ));
        }
    }

    /// Refleja `_call_stop` y `_reset_capture` en los `AtomicBool` de la captura.
    // DECISION: `ez-audio` pide `AtomicBool` y la app usa `Flag`; el puente
    // evita que el hilo de captura retenga el micrófono si el dispositivo deja
    // de entregar bloques (el apagado espera ese cerrojo).
    fn spawn_flag_bridge(
        &self,
        stop: &Arc<Flag>,
        done: &Arc<Flag>,
        flags: Arc<CaptureFlags>,
    ) -> std::io::Result<std::thread::JoinHandle<()>> {
        let stop = Arc::clone(stop);
        let done = Arc::clone(done);
        let reset = Arc::clone(&self.inner.audio.reset_capture);
        std::thread::Builder::new()
            .name("ezagent-call-flags".to_string())
            .spawn(move || {
                while !done.is_set() {
                    if stop.wait_timeout(0.01) {
                        flags.stop.store(true, Ordering::SeqCst);
                        break;
                    }
                    if reset.is_set() {
                        reset.clear();
                        flags.reset.store(true, Ordering::SeqCst);
                    }
                }
                if stop.is_set() {
                    flags.stop.store(true, Ordering::SeqCst);
                }
            })
    }

    /// `_on_mic_level`.
    pub fn on_mic_level(&self, level: f64) {
        let level = if muted(self) { 0.0 } else { level };
        self.inner.ui.hud.set_mic_level(level);
        if in_call(self) && self.state() == LISTENING {
            self.inner.ui.hud.draw(level, Some(LISTENING));
        }
    }

    /// `_is_speaking`: el aviso de espera también cuenta como TTS.
    pub fn is_speaking(&self) -> bool {
        self.state() == SPEAKING || self.inner.speech.busy_speaking.is_set()
    }

    /// `_on_barge_in`.
    pub fn on_barge_in(&self, _level: f64) {
        if muted(self) {
            return;
        }
        let state = self.state();
        let busy = self.inner.speech.busy_speaking.is_set() && state != SPEAKING;
        if !in_call(self)
            || !(state == SPEAKING || busy)
            || !cfg_truthy(&self.config(), "barge_in", true)
        {
            return;
        }
        if confirm_pending(self) {
            self.barge_in_confirmation();
            return;
        }
        if self
            .inner
            .turn
            .close_after_answer
            .swap(false, Ordering::SeqCst)
        {
            // El usuario interrumpe el cierre: no cerramos.
            *self.inner.turn.close_mode.write() = String::new();
        }
        if busy {
            self.barge_in_busy();
            return;
        }
        log("Barge-in: te escucho.");
        advance_turn(self);
        self.signal_tts_stop();
        self.inner
            .session
            .user_speaking
            .store(true, Ordering::SeqCst);
        self.inner
            .session
            .ignore_utterance
            .store(false, Ordering::SeqCst);
        self.set_state(LISTENING);
    }

    /// Barge-in sobre la pregunta de confirmación: corta sin invalidar el turno.
    fn barge_in_confirmation(&self) {
        self.inner.speech.busy_speaking.clear();
        self.signal_tts_stop();
        self.inner
            .session
            .user_speaking
            .store(true, Ordering::SeqCst);
        self.inner
            .session
            .ignore_utterance
            .store(false, Ordering::SeqCst);
        self.set_state(LISTENING);
    }

    /// Barge-in sobre el aviso de espera: su propio stop corta la frase.
    fn barge_in_busy(&self) {
        if let Some(stop) = self.inner.speech.busy_stop.lock().as_ref() {
            stop.set();
        }
        self.inner.speech.busy_speaking.clear();
        self.inner.speech.busy_barged.store(true, Ordering::SeqCst);
        self.inner
            .session
            .user_speaking
            .store(true, Ordering::SeqCst);
        self.inner
            .session
            .ignore_utterance
            .store(false, Ordering::SeqCst);
        self.inner
            .turn
            .post_tts_ignore
            .store(false, Ordering::SeqCst);
        log("Barge-in durante el aviso: corto la frase.");
    }

    /// `_on_speech_start`.
    pub fn on_speech_start(&self, level: f64) {
        if !in_call(self) {
            return;
        }
        if muted(self) {
            self.ignore_utterance(true);
            return;
        }
        if self.inner.speech.busy_speaking.is_set() {
            // Eco del aviso de espera: BargeInDetector corta si es sostenido.
            self.ignore_utterance(true);
            return;
        }
        if confirm_pending(self) {
            self.speech_start_confirmation();
            return;
        }
        if monotonic() < *self.inner.turn.post_tts_until.lock() {
            // Cola del TTS: se descarta el eco; _on_utterance decide.
            self.inner
                .session
                .ignore_utterance
                .store(true, Ordering::SeqCst);
            self.inner
                .turn
                .post_tts_ignore
                .store(true, Ordering::SeqCst);
            return;
        }
        self.inner
            .session
            .user_speaking
            .store(true, Ordering::SeqCst);
        *self.inner.turn.resume_pcm.lock() = None;
        *self.inner.turn.merge_user_id.lock() = None;
        match self.state().as_str() {
            THINKING => self.speech_start_thinking(),
            SPEAKING => self.speech_start_speaking(level),
            _ => {}
        }
    }

    /// Arranque de voz sobre la pregunta de confirmación.
    fn speech_start_confirmation(&self) {
        let cfg = self.config();
        if self.state() == SPEAKING
            && cfg_truthy(&cfg, "barge_in", true)
            && cfg_float(&cfg, "barge_in_rms", 0.008) > 0.0
        {
            self.ignore_utterance(true);
            return;
        }
        // El usuario responde: se corta el TTS y se captura sin otro turno.
        self.inner
            .session
            .user_speaking
            .store(true, Ordering::SeqCst);
        self.inner
            .session
            .ignore_utterance
            .store(false, Ordering::SeqCst);
        self.inner
            .turn
            .post_tts_ignore
            .store(false, Ordering::SeqCst);
        self.signal_tts_stop();
        self.set_state(LISTENING);
    }

    /// El usuario corrige o repite mientras el modelo piensa.
    fn speech_start_thinking(&self) {
        self.resume_previous_turn();
        advance_turn(self);
        self.signal_tts_stop();
        self.set_state(LISTENING);
    }

    /// Arranque de voz mientras suena el TTS: solo corta sin umbral.
    fn speech_start_speaking(&self, level: f64) {
        let cfg = self.config();
        if cfg_truthy(&cfg, "barge_in", true) && cfg_float(&cfg, "barge_in_rms", 0.008) <= 0.0 {
            // Sin umbral configurado no hay detector: se interrumpe al hablar.
            self.on_barge_in(level);
            return;
        }
        self.ignore_utterance(true);
    }

    /// `_on_utterance`.
    pub fn on_utterance(&self, mut pcm: Vec<u8>, rate: u32) {
        self.inner
            .session
            .user_speaking
            .store(false, Ordering::SeqCst);
        if pcm.is_empty() {
            // Arranque descartado por el VAD: se suelta «hablando».
            self.clear_ignores();
            return;
        }
        if muted(self) {
            self.clear_ignores();
            self.maybe_wake(&pcm, rate);
            return;
        }
        if self.inner.session.ignore_utterance.load(Ordering::SeqCst) {
            let now = monotonic();
            let until = *self.inner.turn.post_tts_until.lock();
            let continued = self.inner.turn.post_tts_ignore.load(Ordering::SeqCst) && now >= until;
            self.clear_ignores();
            if !continued {
                return;
            }
        }
        if !in_call(self) {
            return;
        }
        if let Some((head, resume_rate)) = self.inner.turn.resume_pcm.lock().take() {
            if resume_rate == rate {
                let mut merged = head;
                merged.extend_from_slice(&pcm);
                pcm = merged;
            }
        }
        let now = monotonic();
        *self.inner.turn.last_turn_audio.lock() = Some((pcm.clone(), rate, now));
        *self.inner.turn.turn_started.lock() = now;
        let speech_end = self
            .inner
            .audio
            .mic
            .try_lock()
            .and_then(|mic| mic.last_speech_end)
            .unwrap_or(now);
        let timing = Arc::new(TurnTimings::new(speech_end, now));
        *self.inner.turn.turn_timing.lock() = Some(Arc::clone(&timing));
        let generation = if confirm_pending(self) {
            turn_gen(self)
        } else {
            advance_turn(self)
        };
        self.set_state(THINKING);
        self.start_stt(pcm, rate, generation, timing);
    }

    /// `ignore_utterance = true; post_tts_ignore = false`.
    fn ignore_utterance(&self, ignore: bool) {
        self.inner
            .session
            .ignore_utterance
            .store(ignore, Ordering::SeqCst);
        self.inner
            .turn
            .post_tts_ignore
            .store(false, Ordering::SeqCst);
    }

    /// `ignore_utterance = false; post_tts_ignore = false`.
    fn clear_ignores(&self) {
        self.inner
            .session
            .ignore_utterance
            .store(false, Ordering::SeqCst);
        self.inner
            .turn
            .post_tts_ignore
            .store(false, Ordering::SeqCst);
    }

    /// Une la frase anterior si el turno se cerró en una pausa.
    fn resume_previous_turn(&self) {
        let last = self.inner.turn.last_turn_audio.lock().clone();
        let Some((pcm, rate, at)) = last else {
            return;
        };
        if monotonic() - at > RESUME_SECS {
            return;
        }
        *self.inner.turn.resume_pcm.lock() = Some((pcm, rate));
        *self.inner.turn.merge_user_id.lock() = *self.inner.turn.last_voice_user_id.lock();
        log("Sigues hablando: uno esta frase con la anterior.");
    }
}

//! Port of `src/ezagent/voice/speech_queue.py` (F4-05).
//!
//! Dos hilos, síntesis y reproducción, en orden. El hilo que llama a
//! [`SpeechQueue::say`] solo encola texto; un hilo sintetiza y deja el audio en
//! una cola acotada, y otro lo reproduce. Así el lector del LLM sigue
//! consumiendo tokens mientras Kokoro prepara la frase siguiente.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, SendTimeoutError, Sender};
use parking_lot::Mutex;

const TEXT_MAXSIZE: usize = 64;
const AUDIO_MAXSIZE: usize = 8;
const POLL_SECS: Duration = Duration::from_millis(50);

/// `synthesize(text) -> (samples, rate)`.
pub type SynthFn = dyn Fn(&str) -> Result<(Vec<f32>, u32), String> + Send + Sync;

/// `play(samples, rate, on_start) -> True si sonó entera`.
pub type PlayFn = dyn Fn(&[f32], u32, &mut dyn FnMut()) -> Result<bool, String> + Send + Sync;

/// Optional callbacks of `SpeechQueue(synthesize, play, stop_event, ...)`.
#[derive(Default, Clone)]
pub struct SpeechQueueOptions {
    /// Called just before the first sound of the queue.
    pub on_sound: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Seconds spent synthesizing each phrase.
    pub on_synth: Option<Arc<dyn Fn(f64) + Send + Sync>>,
    /// Seconds each phrase was sounding.
    pub on_play: Option<Arc<dyn Fn(f64) + Send + Sync>>,
}

struct Audio {
    samples: Vec<f32>,
    rate: u32,
}

/// State shared with the two worker threads.
struct Shared {
    stop: Arc<AtomicBool>,
    aborted: AtomicBool,
    closed: AtomicBool,
    error: Mutex<Option<String>>,
    sounded: AtomicBool,
    on_sound: Option<Arc<dyn Fn() + Send + Sync>>,
    on_synth: Option<Arc<dyn Fn(f64) + Send + Sync>>,
    on_play: Option<Arc<dyn Fn(f64) + Send + Sync>>,
}

impl Shared {
    /// `_stop or _aborted or _closed or _error`.
    fn halted(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
            || self.aborted.load(Ordering::Relaxed)
            || self.closed.load(Ordering::Relaxed)
            || self.error.lock().is_some()
    }

    /// `_put_audio` checks everything except `_closed`.
    fn halted_audio(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
            || self.aborted.load(Ordering::Relaxed)
            || self.error.lock().is_some()
    }
}

/// Sintetiza y reproduce en orden el texto que se va encolando.
pub struct SpeechQueue {
    shared: Arc<Shared>,
    text_tx: Sender<Option<String>>,
    text_rx: Receiver<Option<String>>,
    audio_tx: Sender<Option<Audio>>,
    audio_rx: Receiver<Option<Audio>>,
    lifecycle: Mutex<()>,
    synth_running: Arc<AtomicBool>,
    synth_handle: Mutex<Option<JoinHandle<()>>>,
    play_handle: Mutex<Option<JoinHandle<()>>>,
}

impl SpeechQueue {
    /// `SpeechQueue(synthesize, play, stop_event, ...)`.
    pub fn new<S, P>(
        synthesize: S,
        play: P,
        stop: Arc<AtomicBool>,
        options: SpeechQueueOptions,
    ) -> Self
    where
        S: Fn(&str) -> Result<(Vec<f32>, u32), String> + Send + Sync + 'static,
        P: Fn(&[f32], u32, &mut dyn FnMut()) -> Result<bool, String> + Send + Sync + 'static,
    {
        let synth: Arc<SynthFn> = Arc::new(synthesize);
        let play: Arc<PlayFn> = Arc::new(play);
        let shared = Arc::new(Shared {
            stop,
            aborted: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            error: Mutex::new(None),
            sounded: AtomicBool::new(false),
            on_sound: options.on_sound,
            on_synth: options.on_synth,
            on_play: options.on_play,
        });
        let (text_tx, text_rx) = bounded(TEXT_MAXSIZE);
        let (audio_tx, audio_rx) = bounded(AUDIO_MAXSIZE);
        let synth_running = Arc::new(AtomicBool::new(true));
        let play_running = Arc::new(AtomicBool::new(true));

        let synth_shared = Arc::clone(&shared);
        let synth_running_flag = Arc::clone(&synth_running);
        let play_running_flag = Arc::clone(&play_running);
        let synth_audio_tx = audio_tx.clone();
        let synth_text_rx = text_rx.clone();
        let synth_handle = thread::Builder::new()
            .name("ezagent-speech-synth".to_string())
            .spawn(move || {
                synthesizer(
                    synth_shared,
                    synth_text_rx,
                    synth_audio_tx,
                    synth,
                    synth_running_flag,
                    play_running_flag,
                );
            })
            .expect("no se pudo crear el hilo ezagent-speech-synth");

        let play_shared = Arc::clone(&shared);
        let play_audio_rx = audio_rx.clone();
        let play_running_flag = Arc::clone(&play_running);
        let play_handle = thread::Builder::new()
            .name("ezagent-speech".to_string())
            .spawn(move || player(play_shared, play_audio_rx, play, play_running_flag))
            .expect("no se pudo crear el hilo ezagent-speech");

        Self {
            shared,
            text_tx,
            text_rx,
            audio_tx,
            audio_rx,
            lifecycle: Mutex::new(()),
            synth_running,
            synth_handle: Mutex::new(Some(synth_handle)),
            play_handle: Mutex::new(Some(play_handle)),
        }
    }

    /// Error de síntesis o reproducción, si lo hubo.
    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().clone()
    }

    /// Encola `text` para sintetizar. `false` si el turno ya se cortó.
    pub fn say(&self, text: &str) -> bool {
        if self.shared.halted() {
            return false;
        }
        put(&self.text_tx, Some(text.to_string()), &self.shared)
    }

    /// Espera a que se sintetice y suene todo lo encolado. `false` si no
    /// terminó dentro de `timeout`; devuelve el error si lo hubo.
    ///
    /// DECISION: el `timeout` del Python son segundos flotantes; aquí es
    /// [`Duration`], como el resto del crate.
    pub fn finish(&self, timeout: Option<Duration>) -> Result<bool, String> {
        let deadline = timeout.map(|timeout| Instant::now() + timeout);
        {
            let _lifecycle = self.lifecycle.lock();
            if !self.shared.closed.load(Ordering::Relaxed) {
                self.shared.closed.store(true, Ordering::Relaxed);
                put_done(
                    &self.text_tx,
                    &self.shared,
                    Some(self.synth_running.as_ref()),
                );
            }
        }
        join_thread(&self.synth_handle, deadline);
        join_thread(&self.play_handle, deadline);
        if let Some(error) = self.error() {
            return Err(error);
        }
        Ok(!self.shared.stop.load(Ordering::Relaxed)
            && is_finished(&self.synth_handle)
            && is_finished(&self.play_handle))
    }

    /// Descarta lo pendiente sin esperar (la frase en curso acaba).
    pub fn abort(&self) {
        self.shared.aborted.store(true, Ordering::Relaxed);
        self.shared.closed.store(true, Ordering::Relaxed);
        drain(&self.text_rx);
        drain(&self.audio_rx);
        put_done(&self.text_tx, &self.shared, None);
        put_done(&self.audio_tx, &self.shared, None);
    }
}

impl Drop for SpeechQueue {
    fn drop(&mut self) {
        // Python usa hilos daemon; aquí se les pide que terminen para que no
        // queden bloqueados en una cola que ya nadie va a consumir.
        self.abort();
    }
}

fn synthesizer(
    shared: Arc<Shared>,
    text_rx: Receiver<Option<String>>,
    audio_tx: Sender<Option<Audio>>,
    synthesize: Arc<SynthFn>,
    synth_running: Arc<AtomicBool>,
    play_running: Arc<AtomicBool>,
) {
    let _running = FlagGuard(synth_running);
    // El reproductor siempre recibe su fin, aunque el worker falle (finally).
    let _done = AudioDone {
        tx: audio_tx.clone(),
        shared: Arc::clone(&shared),
        until: play_running,
    };
    while let Ok(item) = text_rx.recv() {
        let Some(text) = item else {
            break;
        };
        if shared.halted_audio() {
            continue;
        }
        let began = Instant::now();
        match synthesize(&text) {
            Ok((samples, rate)) => {
                if let Some(callback) = &shared.on_synth {
                    callback(began.elapsed().as_secs_f64());
                }
                if shared.halted_audio() {
                    continue;
                }
                put_audio(&audio_tx, Audio { samples, rate }, &shared);
            }
            Err(error) => {
                tracing::warn!("Síntesis: {error}");
                *shared.error.lock() = Some(error);
                shared.closed.store(true, Ordering::Relaxed);
                break;
            }
        }
    }
}

fn player(
    shared: Arc<Shared>,
    audio_rx: Receiver<Option<Audio>>,
    play: Arc<PlayFn>,
    play_running: Arc<AtomicBool>,
) {
    let _running = FlagGuard(play_running);
    while let Ok(item) = audio_rx.recv() {
        let Some(audio) = item else {
            return;
        };
        if shared.halted_audio() {
            continue; // se vacía la cola sin sonar
        }
        play_one(&shared, play.as_ref(), &audio);
    }
}

/// `finally: self._put_done(self._audio_q)` of the synthesis thread.
struct AudioDone {
    tx: Sender<Option<Audio>>,
    shared: Arc<Shared>,
    until: Arc<AtomicBool>,
}

impl Drop for AudioDone {
    fn drop(&mut self) {
        put_done(&self.tx, &self.shared, Some(self.until.as_ref()));
    }
}

/// Clears a running flag on exit, including a panic.
struct FlagGuard(Arc<AtomicBool>);

impl Drop for FlagGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

fn play_one(shared: &Shared, play: &PlayFn, audio: &Audio) {
    let mut started: Option<Instant> = None;
    let mut on_start = || {
        started = Some(Instant::now());
        if !shared.sounded.swap(true, Ordering::Relaxed) {
            if let Some(callback) = &shared.on_sound {
                callback();
            }
        }
    };
    if let Err(error) = play(&audio.samples, audio.rate, &mut on_start) {
        tracing::warn!("Reproducción: {error}");
        *shared.error.lock() = Some(error);
        shared.closed.store(true, Ordering::Relaxed);
        return;
    }
    if let (Some(callback), Some(begin)) = (&shared.on_play, started) {
        callback(begin.elapsed().as_secs_f64());
    }
}

/// Encola con backpressure; `false` si el turno se cortó.
fn put<T>(target: &Sender<Option<T>>, item: Option<T>, shared: &Shared) -> bool {
    let mut item = item;
    loop {
        if shared.halted() {
            return false;
        }
        match target.send_timeout(item, POLL_SECS) {
            Ok(()) => return true,
            Err(SendTimeoutError::Timeout(returned)) => item = returned,
            Err(SendTimeoutError::Disconnected(_)) => return false,
        }
    }
}

/// `_put_audio`: igual que `_put` sin mirar `_closed`.
fn put_audio(target: &Sender<Option<Audio>>, audio: Audio, shared: &Shared) -> bool {
    let mut item = Some(audio);
    loop {
        if shared.halted_audio() {
            return false;
        }
        match target.send_timeout(item, POLL_SECS) {
            Ok(()) => return true,
            Err(SendTimeoutError::Timeout(returned)) => item = returned,
            Err(SendTimeoutError::Disconnected(_)) => return false,
        }
    }
}

/// `_put_done`: espera hueco, salvo que el turno se cortó o el worker murió.
fn put_done<T>(target: &Sender<Option<T>>, shared: &Shared, until: Option<&AtomicBool>) {
    loop {
        match target.send_timeout(None, POLL_SECS) {
            Ok(()) => return,
            Err(SendTimeoutError::Timeout(_)) => {
                if shared.aborted.load(Ordering::Relaxed)
                    || until.is_some_and(|flag| !flag.load(Ordering::Relaxed))
                {
                    return;
                }
            }
            Err(SendTimeoutError::Disconnected(_)) => return,
        }
    }
}

fn drain<T>(target: &Receiver<Option<T>>) {
    while target.try_recv().is_ok() {}
}

/// `Thread.join(remaining)`: espera hasta el plazo sin consumir el handle.
fn join_thread(handle: &Mutex<Option<JoinHandle<()>>>, deadline: Option<Instant>) {
    let Some(deadline) = deadline else {
        let joined = handle.lock().take();
        if let Some(joined) = joined {
            let _ = joined.join();
        }
        return;
    };
    loop {
        if handle.lock().as_ref().is_none_or(JoinHandle::is_finished) {
            let joined = handle.lock().take();
            if let Some(joined) = joined {
                let _ = joined.join();
            }
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        thread::sleep((deadline - now).min(Duration::from_millis(5)));
    }
}

fn is_finished(handle: &Mutex<Option<JoinHandle<()>>>) -> bool {
    handle.lock().as_ref().is_none_or(JoinHandle::is_finished)
}

#[cfg(test)]
mod tests;

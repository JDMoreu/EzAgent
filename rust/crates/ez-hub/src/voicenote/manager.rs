//! Dueño de la nota: coordina los hilos sin retener candados durante E/S.

use std::process::Child;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::bounded;
use ez_core::config::VoiceConfig;
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

use crate::texts;

use super::recorder::MicNoteRecorder;
use super::transcriber::WhisperTranscriber;
use super::{insert_at, NoteMachine, SystemClock};

type Machine = NoteMachine<MicNoteRecorder, WhisperTranscriber, SystemClock>;

/// `Idle` espera un clic; `Recording` tiene la máquina viva; `Busy` impide un
/// segundo inicio mientras se transcribe fuera de cualquier candado.
// DECISION: `Busy` also covers the interval while the microphone opens and the
// transcription runs, so a fast double click can never start two notes.
enum Slot {
    Idle,
    Recording(Box<Machine>),
    Busy,
}

/// Estado de la nota compartido por los comandos de la vista.
pub struct VoiceNotes {
    slot: Mutex<Slot>,
    config: VoiceConfig,
    whisper: Arc<Mutex<Option<Child>>>,
}

impl VoiceNotes {
    pub fn new() -> Self {
        let config = VoiceConfig::load(None).unwrap_or_else(|error| {
            tracing::warn!(%error, "no pude leer voice.json; uso los valores por omisión");
            VoiceConfig::default()
        });
        Self {
            slot: Mutex::new(Slot::Idle),
            config,
            whisper: Arc::new(Mutex::new(None)),
        }
    }

    /// Empieza la grabación y programa el tope de cinco minutos.
    pub fn start(self: &Arc<Self>, app: AppHandle) -> Result<(), String> {
        {
            let mut slot = self.slot.lock();
            if !matches!(*slot, Slot::Idle) {
                return Err(texts::VOICE_BUSY.to_string());
            }
            // Marca de reserva: abrir el micrófono y lanzar hilos va sin candado.
            *slot = Slot::Busy;
        }
        let emit = {
            let app = app.clone();
            Arc::new(move |level: f64| {
                let payload = serde_json::json!({ "level": level });
                let _ = app.emit("hub://voice-level", payload);
            })
        };
        let recorder = MicNoteRecorder::new(self.config.clone(), emit);
        let transcriber = WhisperTranscriber::new(self.config.clone(), Arc::clone(&self.whisper));
        let mut machine = Machine::new(recorder, transcriber, SystemClock::new());
        if let Err(error) = machine.start() {
            *self.slot.lock() = Slot::Idle;
            return Err(error.message());
        }
        *self.slot.lock() = Slot::Recording(Box::new(machine));

        let watcher = Arc::clone(self);
        if thread::Builder::new()
            .name("ez-hub-voice-timeout".to_string())
            .spawn(move || watcher.watchdog(app))
            .is_err()
        {
            tracing::warn!("no pude lanzar el vigilante de la nota de voz");
        }
        Ok(())
    }

    /// Para y devuelve el texto transcrito; bloquea solo en el canal, sin candado.
    pub fn stop(&self) -> Result<String, String> {
        self.finish()
    }

    /// Inserta el texto dictado donde estaba el cursor.
    pub fn insert(&self, current: &str, cursor: usize, added: &str) -> (String, usize) {
        insert_at(current, cursor, added)
    }

    /// Vigila el reloj de la máquina; al vencer, transcribe y avisa a la vista.
    // DECISION: se sondea una vez por segundo en vez de dormir 300 s, para que
    // el tope use el mismo reloj que la máquina probada con reloj falso.
    fn watchdog(self: Arc<Self>, app: AppHandle) {
        loop {
            thread::sleep(Duration::from_secs(1));
            let (active, timed_out) = {
                let slot = self.slot.lock();
                match &*slot {
                    Slot::Recording(machine) => (true, machine.is_timed_out()),
                    _ => (false, false),
                }
            };
            if !active {
                return;
            }
            if timed_out {
                let payload = match self.finish() {
                    Ok(text) => serde_json::json!({ "texto": text }),
                    Err(error) => serde_json::json!({ "error": error }),
                };
                let _ = app.emit("hub://voice-note-done", payload);
                return;
            }
        }
    }

    /// Saca la máquina del candado, la termina en `ez-hub-voice-stt` y la suelta.
    fn finish(&self) -> Result<String, String> {
        let machine = {
            let mut slot = self.slot.lock();
            let taken = std::mem::replace(&mut *slot, Slot::Busy);
            match taken {
                Slot::Recording(machine) => machine,
                other => {
                    *slot = other;
                    return Err(texts::VOICE_NOT_RECORDING.to_string());
                }
            }
        };
        let (sender, receiver) = bounded(1);
        let worker = thread::Builder::new()
            .name("ez-hub-voice-stt".to_string())
            .spawn(move || {
                let mut machine = machine;
                let _ = sender.send(machine.stop().map_err(|error| error.message()));
            });
        let result = match worker {
            Ok(worker) => {
                let answer = receiver
                    .recv()
                    .map_err(|_| texts::VOICE_STT_FAILED.to_string());
                let _ = worker.join();
                answer?
            }
            Err(_) => Err(texts::VOICE_STT_FAILED.to_string()),
        };
        *self.slot.lock() = Slot::Idle;
        result
    }
}

impl Default for VoiceNotes {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for VoiceNotes {
    fn drop(&mut self) {
        if let Some(process) = self.whisper.lock().as_mut() {
            let _ = process.kill();
        }
    }
}

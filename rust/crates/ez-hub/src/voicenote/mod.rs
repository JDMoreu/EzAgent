//! Nota de voz de la barra del elefante: máquina de estados pura sobre traits
//! (brief ECO-7). La captura y la transcripción reales viven en submódulos.

use std::time::{Duration, Instant};

use ez_audio::capture::pcm_to_wav;

use crate::texts;

mod manager;
mod recorder;
mod transcriber;

#[cfg(test)]
mod tests;

pub use manager::VoiceNotes;

/// Tope de grabación: cinco minutos, tras los que se transcribe lo capturado.
pub const MAX_SECS: u64 = 300;

/// Audio capturado: PCM mono 16 bits y la tasa del dispositivo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pcm {
    pub samples: Vec<u8>,
    pub rate: u32,
}

/// Fallos de la nota, con el texto que ve la persona en la línea de estado.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoteError {
    NoMicrophone,
    Empty,
    Transcribe(String),
    InvalidState(&'static str),
}

impl NoteError {
    /// Texto para la línea de estado de la barra.
    pub fn message(&self) -> String {
        match self {
            Self::NoMicrophone => texts::VOICE_NO_MICROPHONE.to_string(),
            Self::Empty => texts::VOICE_EMPTY.to_string(),
            Self::Transcribe(reason) => texts::voice_transcribe_failed(reason),
            Self::InvalidState(reason) => (*reason).to_string(),
        }
    }
}

/// Captura de audio; las implementaciones real y de prueba cumplen este contrato.
pub trait NoteRecorder: Send {
    /// Empieza a capturar. Solo falla si no se puede abrir el micrófono.
    fn start(&mut self) -> Result<(), NoteError>;
    /// Para y devuelve todo el audio capturado hasta ahora.
    fn stop(&mut self) -> Result<Pcm, NoteError>;
}

/// Transcripción de un WAV a texto.
pub trait Transcriber: Send {
    fn transcribe(&mut self, wav: &[u8]) -> Result<String, NoteError>;
}

/// Reloj inyectable para probar el tope de cinco minutos sin esperar.
pub trait Clock: Send {
    fn now_ms(&self) -> u64;
}

/// Reloj de sistema, contado desde que se crea la máquina.
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }
}

/// Estados de la nota: inactiva, grabando y transcribiendo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Recording,
    Transcribing,
}

/// Lógica de la nota: transiciones, tope de tiempo y canals de audio a texto.
pub struct NoteMachine<R: NoteRecorder, T: Transcriber, C: Clock> {
    recorder: R,
    transcriber: T,
    clock: C,
    state: State,
    started_ms: u64,
}

impl<R: NoteRecorder, T: Transcriber, C: Clock> NoteMachine<R, T, C> {
    pub fn new(recorder: R, transcriber: T, clock: C) -> Self {
        Self {
            recorder,
            transcriber,
            clock,
            state: State::Idle,
            started_ms: 0,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    /// Inicia la grabación; un segundo clic no la reinicia.
    pub fn start(&mut self) -> Result<(), NoteError> {
        if self.state != State::Idle {
            return Err(NoteError::InvalidState(texts::VOICE_ALREADY_STARTED));
        }
        self.recorder.start()?;
        self.started_ms = self.clock.now_ms();
        self.state = State::Recording;
        Ok(())
    }

    /// Milisegundos transcurridos desde el inicio de la grabación.
    pub fn elapsed_ms(&self) -> u64 {
        if self.state == State::Recording {
            self.clock.now_ms().saturating_sub(self.started_ms)
        } else {
            0
        }
    }

    /// True al llegar al tope de cinco minutos mientras se graba.
    pub fn is_timed_out(&self) -> bool {
        self.state == State::Recording && self.elapsed_ms() >= MAX_SECS * 1000
    }

    /// Para, junta los fragmentos, transcribe y vuelve a inactiva.
    pub fn stop(&mut self) -> Result<String, NoteError> {
        if self.state != State::Recording {
            return Err(NoteError::InvalidState(texts::VOICE_NOT_RECORDING));
        }
        self.state = State::Transcribing;
        let pcm = match self.recorder.stop() {
            Ok(pcm) => pcm,
            Err(error) => {
                self.state = State::Idle;
                return Err(error);
            }
        };
        if pcm.samples.is_empty() {
            self.state = State::Idle;
            return Err(NoteError::Empty);
        }
        let wav = pcm_to_wav(&pcm.samples, pcm.rate);
        let text = match self.transcriber.transcribe(&wav) {
            Ok(text) => text,
            Err(error) => {
                self.state = State::Idle;
                return Err(error);
            }
        };
        self.state = State::Idle;
        Ok(text)
    }
}

/// Concatena los fragmentos que el VAD fue entregando, en orden.
pub fn join_fragments(fragments: &[Vec<u8>]) -> Vec<u8> {
    let total = fragments.iter().map(Vec::len).sum();
    let mut pcm = Vec::with_capacity(total);
    for fragment in fragments {
        pcm.extend_from_slice(fragment);
    }
    pcm
}

/// Inserta `added` en `cursor` y devuelve el texto y el nuevo cursor.
///
/// Añade un espacio a cada lado solo cuando haría falta separar palabras.
// DECISION: el brief pide «un espacio si hace falta»; se añade a izquierda y
// derecha solo cuando hay texto pegado sin espacio, para no alterar saltos.
pub fn insert_at(current: &str, cursor: usize, added: &str) -> (String, usize) {
    let cursor = floor_boundary(current, cursor);
    let before = &current[..cursor];
    let after = &current[cursor..];
    let mut piece = added.to_string();
    if space_before(before, &piece) {
        piece.insert(0, ' ');
    }
    if space_after(&piece, after) {
        piece.push(' ');
    }
    let next = cursor + piece.len();
    let mut out = String::with_capacity(current.len() + piece.len());
    out.push_str(before);
    out.push_str(&piece);
    out.push_str(after);
    (out, next)
}

fn floor_boundary(text: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(text.len());
    while cursor > 0 && !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

fn space_before(before: &str, piece: &str) -> bool {
    match (before.chars().next_back(), piece.chars().next()) {
        (Some(left), Some(right)) => !left.is_whitespace() && !right.is_whitespace(),
        _ => false,
    }
}

fn space_after(piece: &str, after: &str) -> bool {
    match (piece.chars().next_back(), after.chars().next()) {
        (Some(left), Some(right)) => !left.is_whitespace() && !right.is_whitespace(),
        _ => false,
    }
}

/// Longitud del fragmento de nivel: para no saturar la vista, se emite a lo sumo
/// diez veces por segundo.
pub const LEVEL_INTERVAL: Duration = Duration::from_millis(100);

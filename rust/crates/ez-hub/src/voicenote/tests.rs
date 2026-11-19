//! Pruebas de la máquina de estados, el WAV y la inserción en el cursor.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use ez_audio::capture::pcm_to_wav;

use super::*;

struct FakeRecorder {
    pcm: Pcm,
    start_error: Option<NoteError>,
    stop_error: Option<NoteError>,
    starts: usize,
    stops: usize,
}

impl FakeRecorder {
    fn with_samples(samples: &[u8]) -> Self {
        Self {
            pcm: Pcm {
                samples: samples.to_vec(),
                rate: 16_000,
            },
            start_error: None,
            stop_error: None,
            starts: 0,
            stops: 0,
        }
    }
}

impl NoteRecorder for FakeRecorder {
    fn start(&mut self) -> Result<(), NoteError> {
        self.starts += 1;
        match &self.start_error {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }

    fn stop(&mut self) -> Result<Pcm, NoteError> {
        self.stops += 1;
        match &self.stop_error {
            Some(error) => Err(error.clone()),
            None => Ok(self.pcm.clone()),
        }
    }
}

struct FakeTranscriber {
    text: String,
    error: Option<NoteError>,
    calls: usize,
}

impl Transcriber for FakeTranscriber {
    fn transcribe(&mut self, _wav: &[u8]) -> Result<String, NoteError> {
        self.calls += 1;
        match &self.error {
            Some(error) => Err(error.clone()),
            None => Ok(self.text.clone()),
        }
    }
}

#[derive(Clone)]
struct FakeClock {
    now: Arc<AtomicU64>,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            now: Arc::new(AtomicU64::new(0)),
        }
    }

    fn advance(&self, ms: u64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
}

type TestMachine = NoteMachine<FakeRecorder, FakeTranscriber, FakeClock>;

fn machine() -> (TestMachine, FakeClock) {
    let clock = FakeClock::new();
    let recorder = FakeRecorder::with_samples(&[1, 2, 3, 4]);
    let transcriber = FakeTranscriber {
        text: "hola dictado".to_string(),
        error: None,
        calls: 0,
    };
    (
        TestMachine::new(recorder, transcriber, clock.clone()),
        clock,
    )
}

#[test]
fn start_moves_to_recording_and_a_second_click_is_rejected() {
    let (mut note, _) = machine();
    assert_eq!(note.state(), State::Idle);
    note.start().expect("empezar");
    assert_eq!(note.state(), State::Recording);
    assert_eq!(note.recorder.starts, 1);
    assert!(note.start().is_err());
    assert_eq!(note.recorder.starts, 1);
}

#[test]
fn stop_transcribes_and_returns_to_idle() {
    let (mut note, _) = machine();
    note.start().expect("empezar");
    assert_eq!(note.stop().expect("transcribir"), "hola dictado");
    assert_eq!(note.state(), State::Idle);
    assert_eq!(note.recorder.stops, 1);
    assert_eq!(note.transcriber.calls, 1);
}

#[test]
fn stopping_while_idle_is_rejected() {
    let (mut note, _) = machine();
    assert!(note.stop().is_err());
    assert_eq!(note.recorder.stops, 0);
}

#[test]
fn five_minutes_are_the_cap_and_only_count_while_recording() {
    let (mut note, clock) = machine();
    clock.advance(123_000);
    assert_eq!(note.elapsed_ms(), 0);
    assert!(!note.is_timed_out());
    note.start().expect("empezar");
    clock.advance(MAX_SECS * 1000 - 1);
    assert_eq!(note.elapsed_ms(), MAX_SECS * 1000 - 1);
    assert!(!note.is_timed_out());
    clock.advance(1);
    assert!(note.is_timed_out());
    note.stop().expect("transcribir");
    assert_eq!(note.elapsed_ms(), 0);
    assert!(!note.is_timed_out());
}

#[test]
fn a_microphone_failure_keeps_the_machine_idle() {
    let (mut note, _) = machine();
    note.recorder.start_error = Some(NoteError::NoMicrophone);
    let error = note.start().expect_err("debe fallar");
    assert_eq!(error.message(), crate::texts::VOICE_NO_MICROPHONE);
    assert_eq!(note.state(), State::Idle);
}

#[test]
fn an_empty_recording_reports_nothing_was_heard() {
    let (mut note, _) = machine();
    note.recorder.pcm.samples.clear();
    note.start().expect("empezar");
    let error = note.stop().expect_err("debe fallar");
    assert_eq!(error.message(), crate::texts::VOICE_EMPTY);
    assert_eq!(note.state(), State::Idle);
    assert_eq!(note.transcriber.calls, 0);
}

#[test]
fn a_transcription_failure_shows_the_reason_and_recovers() {
    let (mut note, _) = machine();
    note.transcriber.error = Some(NoteError::Transcribe("whisper caído".to_string()));
    note.start().expect("empezar");
    let error = note.stop().expect_err("debe fallar");
    assert_eq!(error.message(), "No pude transcribir: whisper caído");
    assert_eq!(note.state(), State::Idle);
    note.transcriber.error = None;
    note.start().expect("reintentar");
    assert_eq!(note.stop().expect("transcribir"), "hola dictado");
}

#[test]
fn fragments_are_joined_in_order() {
    let fragments = vec![vec![1, 2], vec![], vec![3], vec![4, 5]];
    assert_eq!(join_fragments(&fragments), vec![1, 2, 3, 4, 5]);
    assert!(join_fragments(&[]).is_empty());
}

#[test]
fn the_resulting_wav_has_the_expected_header_and_sizes() {
    let pcm = join_fragments(&[vec![1, 2, 3, 4], vec![5, 6, 7, 8]]);
    let wav = pcm_to_wav(&pcm, 16_000);
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(&wav[36..40], b"data");
    let riff = u32::from_le_bytes(wav[4..8].try_into().expect("4 bytes"));
    let data = u32::from_le_bytes(wav[40..44].try_into().expect("4 bytes"));
    assert_eq!(riff, 36 + pcm.len() as u32);
    assert_eq!(data, pcm.len() as u32);
    assert_eq!(wav.len(), 44 + pcm.len());
    assert_eq!(&wav[44..], pcm.as_slice());
}

#[test]
fn insertion_separates_words_at_the_cursor() {
    let (text, cursor) = insert_at("hola", 4, "mundo");
    assert_eq!(text, "hola mundo");
    assert_eq!(cursor, 10);
    let (text, cursor) = insert_at("hola adios", 4, "mundo");
    assert_eq!(text, "hola mundo adios");
    assert_eq!(cursor, 10);
    let (text, cursor) = insert_at("hola", 0, "mundo");
    assert_eq!(text, "mundo hola");
    assert_eq!(cursor, 6);
}

#[test]
fn insertion_keeps_existing_spaces_and_unicode_boundaries() {
    let (text, _) = insert_at("hola", 4, " mundo");
    assert_eq!(text, "hola mundo");
    let (text, _) = insert_at("hola  ", 4, "mundo");
    assert_eq!(text, "hola mundo  ");
    let (text, cursor) = insert_at("ábc", 1, "x");
    assert_eq!(text, "x ábc");
    assert_eq!(cursor, 2);
    let (text, cursor) = insert_at("abc", 99, "x");
    assert_eq!(text, "abc x");
    assert_eq!(cursor, 5);
}

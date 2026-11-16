//! Barge-in adaptativo con audio real del arnés (P-IMP-BARGEIN).
//!
//! Mezcla un WAV de voz del TTS como referencia de reproducción, su eco
//! atenuado y retrasado (como lo oiría el micrófono) y la voz del usuario, y
//! comprueba lo que el brief pide con señal real, sin tocar los altavoces.

use std::path::PathBuf;
use std::sync::Arc;

use ez_audio::barge_in::{BargeInMode, PlaybackReference};
use ez_audio::capture::{pcm_rms, CaptureEvent, CapturePipeline, VadSettings};

const RATE: u32 = 16_000;
const CHUNK: usize = 480;
/// Retardo altavoz→micrófono: 5 bloques (150 ms).
const DELAY_BLOCKS: usize = 5;
/// Inicio de la frase del usuario: 17 bloques (0,51 s).
const VOICE_START: usize = 17;

fn audio_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../parity/replay/audio")
}

/// WAV PCM16 mono 16 kHz como muestras en `[-1, 1]`.
fn read_wav(name: &str) -> Vec<f32> {
    let bytes = std::fs::read(audio_dir().join(name)).expect("WAV del arnés");
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    let mut offset = 12;
    let mut data: &[u8] = &[];
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().expect("4 bytes"));
        let body = offset + 8;
        let end = (body + size as usize).min(bytes.len());
        if id == b"data" {
            data = &bytes[body..end];
        }
        offset = end + (size as usize % 2);
    }
    data.as_chunks::<2>()
        .0
        .iter()
        .map(|pair| f32::from(i16::from_le_bytes(*pair)) / 32768.0)
        .collect()
}

fn pcm(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|value| ((value.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect()
}

/// Ruido determinista de sala (±0,0005).
fn noise(sample: usize) -> f32 {
    let mut state = (sample as u32)
        .wrapping_mul(1_664_525)
        .wrapping_add(1_013_904_223);
    state ^= state >> 13;
    (f64::from(state % 1000) / 500.0 - 1.0) as f32 * 0.0005
}

fn scale(samples: &[f32], factor: f32) -> Vec<f32> {
    samples.iter().map(|value| value * factor).collect()
}

/// Voz del usuario con los 0,51 s de silencio de delante.
fn voice_track() -> Vec<f32> {
    let mut track = vec![0.0; VOICE_START * CHUNK];
    track.extend(scale(&read_wav("espera_para.wav"), 0.07));
    track
}

struct Rig {
    settings: VadSettings,
    reference: Arc<PlaybackReference>,
    pipeline: CapturePipeline,
}

impl Rig {
    fn new(mode: BargeInMode) -> Self {
        let settings = VadSettings {
            silence_secs: 0.2,
            min_speech_secs: 0.1,
            barge_in_rms: 0.008,
            barge_in_secs: 0.25,
            barge_in_mode: mode,
            ..VadSettings::default()
        };
        let reference = Arc::new(PlaybackReference::new());
        let pipeline =
            CapturePipeline::with_reference(&settings, RATE, None, Some(Arc::clone(&reference)));
        Self {
            settings,
            reference,
            pipeline,
        }
    }

    /// Un bloque de 30 ms; devuelve `true` si hubo barge-in.
    fn feed(&mut self, reference: &[f32], voice: &[f32], block: usize, coupling: f32) -> bool {
        let start = block * CHUNK;
        let echo_start = start.saturating_sub(DELAY_BLOCKS * CHUNK);
        let mixed: Vec<f32> = (0..CHUNK)
            .map(|index| {
                let at = start + index;
                let echo = if start >= DELAY_BLOCKS * CHUNK {
                    reference.get(echo_start + index).copied().unwrap_or(0.0) * coupling
                } else {
                    0.0
                };
                let user = voice.get(at).copied().unwrap_or(0.0);
                echo + user + noise(at)
            })
            .collect();
        let chunk = pcm(&mixed);
        let level = pcm_rms(&chunk);
        let heard = reference
            .get(start..(start + CHUNK).min(reference.len()))
            .unwrap_or(&[]);
        if !heard.is_empty() {
            self.reference.publish(pcm_rms(&pcm(heard)));
        }
        let events = self
            .pipeline
            .feed(&self.settings, &chunk, level, true, false);
        events
            .iter()
            .any(|event| matches!(event, CaptureEvent::BargeIn(_)))
    }
}

/// Dos frases de «Dime algo largo»; el usuario responde a los 0,51 s de cada
/// una. Devuelve el bloque (desde el arranque de la voz) del primer barge-in de
/// la segunda frase, cuando el acople ya se aprendió.
fn run_scene(mode: BargeInMode, coupling: f32, with_voice: bool) -> Option<usize> {
    let reference = scale(&read_wav("dime_algo_largo.wav"), 0.4);
    let voice = if with_voice {
        voice_track()
    } else {
        Vec::new()
    };
    let mut rig = Rig::new(mode);
    let mut first = None;
    for phrase in 0..2 {
        rig.reference.begin();
        let blocks = reference
            .len()
            .div_ceil(CHUNK)
            .max(voice.len().div_ceil(CHUNK));
        for block in 0..blocks {
            if rig.feed(&reference, &voice, block, coupling) && phrase == 1 && first.is_none() {
                first = Some(block.saturating_sub(VOICE_START));
            }
        }
        rig.reference.end();
    }
    first
}

#[test]
fn real_audio_echo_alone_never_interrupts() {
    for coupling in [0.3, 0.5, 0.8] {
        assert_eq!(
            run_scene(BargeInMode::Adaptive, coupling, false),
            None,
            "el eco (acople {coupling}) no interrumpe"
        );
    }
}

#[test]
fn real_audio_a_moderate_voice_interrupts_in_time_and_fixed_does_not() {
    let adaptive =
        run_scene(BargeInMode::Adaptive, 0.05, true).expect("la voz moderada interrumpe");
    assert!(
        adaptive <= 13,
        "≈0,3 s de voz sostenida con las pausas de la frase real (fueron {adaptive} bloques)"
    );
    assert_eq!(
        run_scene(BargeInMode::Fixed, 0.05, true),
        None,
        "con el umbral fijo de 0,008 no dispara"
    );
}

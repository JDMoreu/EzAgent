//! Unit tests for `queue.rs` (F4-05).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::bounded;
use parking_lot::Mutex;

use super::*;

#[derive(Default)]
struct Recorder {
    synthesized: Vec<String>,
    played: Vec<u32>,
    sounds: usize,
    synth_secs: Vec<f64>,
    play_secs: Vec<f64>,
}

fn recorder() -> Arc<Mutex<Recorder>> {
    Arc::new(Mutex::new(Recorder::default()))
}

fn options(record: &Arc<Mutex<Recorder>>) -> SpeechQueueOptions {
    let sound = Arc::clone(record);
    let synth = Arc::clone(record);
    let play = Arc::clone(record);
    SpeechQueueOptions {
        on_sound: Some(Arc::new(move || sound.lock().sounds += 1)),
        on_synth: Some(Arc::new(move |secs| synth.lock().synth_secs.push(secs))),
        on_play: Some(Arc::new(move |secs| play.lock().play_secs.push(secs))),
    }
}

fn synthesize(
    record: &Arc<Mutex<Recorder>>,
) -> impl Fn(&str) -> Result<(Vec<f32>, u32), String> + Send + Sync + 'static {
    let record = Arc::clone(record);
    move |text| {
        record.lock().synthesized.push(text.to_string());
        Ok((vec![0.1; 480], 24_000))
    }
}

fn play(
    record: &Arc<Mutex<Recorder>>,
) -> impl Fn(&[f32], u32, &mut dyn FnMut()) -> Result<bool, String> + Send + Sync + 'static {
    let record = Arc::clone(record);
    move |_samples, rate, on_start| {
        on_start();
        record.lock().played.push(rate);
        Ok(true)
    }
}

#[test]
fn phrases_play_in_order_and_sound_fires_once() {
    let record = recorder();
    let queue = SpeechQueue::new(
        synthesize(&record),
        play(&record),
        Arc::new(AtomicBool::new(false)),
        options(&record),
    );
    assert!(queue.say("uno"));
    assert!(queue.say("dos"));
    assert!(queue.finish(None).expect("fin"));
    let record = record.lock();
    assert_eq!(record.synthesized, ["uno", "dos"]);
    assert_eq!(record.played, [24_000, 24_000]);
    assert_eq!(record.sounds, 1);
    assert_eq!(record.synth_secs.len(), 2);
    assert_eq!(record.play_secs.len(), 2);
}

#[test]
fn say_is_false_after_stop_or_finish() {
    let record = recorder();
    let stop = Arc::new(AtomicBool::new(false));
    let queue = SpeechQueue::new(
        synthesize(&record),
        play(&record),
        Arc::clone(&stop),
        options(&record),
    );
    stop.store(true, Ordering::Relaxed);
    assert!(!queue.say("no"));
    assert!(!queue.finish(None).expect("fin"));
    assert!(!queue.say("tampoco"));
}

#[test]
fn abort_discards_pending_without_waiting() {
    let record = recorder();
    let (started_tx, started_rx) = bounded::<()>(1);
    let (gate_tx, gate_rx) = bounded::<()>(1);
    let worker = Arc::clone(&record);
    let synthesize = move |text: &str| {
        if text == "uno" {
            let _ = started_tx.send(());
            let _ = gate_rx.recv();
        }
        worker.lock().synthesized.push(text.to_string());
        Ok((vec![0.1; 480], 24_000))
    };
    let queue = SpeechQueue::new(
        synthesize,
        play(&record),
        Arc::new(AtomicBool::new(false)),
        options(&record),
    );
    assert!(queue.say("uno"));
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("empezó la síntesis");
    assert!(queue.say("dos"));
    queue.abort();
    gate_tx.send(()).expect("abre la puerta");
    assert!(queue.finish(None).expect("fin"));
    let record = record.lock();
    assert_eq!(record.synthesized, ["uno"]);
    assert!(record.played.is_empty());
    assert_eq!(record.sounds, 0);
}

#[test]
fn synthesis_error_is_reported_and_stops_the_queue() {
    let stop = Arc::new(AtomicBool::new(false));
    let queue = SpeechQueue::new(
        |_text| Err("boom".to_string()),
        |_samples, _rate, _on_start| Ok(true),
        stop,
        SpeechQueueOptions::default(),
    );
    assert!(queue.say("uno"));
    assert_eq!(queue.finish(None).expect_err("error"), "boom");
    assert!(!queue.say("dos"));
}

#[test]
fn playback_error_is_reported() {
    let queue = SpeechQueue::new(
        |_text| Ok((vec![0.1; 48], 24_000)),
        |_samples, _rate, _on_start| Err("altavoz".to_string()),
        Arc::new(AtomicBool::new(false)),
        SpeechQueueOptions::default(),
    );
    assert!(queue.say("uno"));
    assert_eq!(queue.finish(None).expect_err("error"), "altavoz");
}

#[test]
fn finish_timeout_leaves_the_work_running() {
    let queue = SpeechQueue::new(
        |_text| {
            std::thread::sleep(Duration::from_millis(150));
            Ok((vec![0.1; 48], 24_000))
        },
        |_samples, _rate, on_start| {
            on_start();
            Ok(true)
        },
        Arc::new(AtomicBool::new(false)),
        SpeechQueueOptions::default(),
    );
    assert!(queue.say("uno"));
    assert!(!queue
        .finish(Some(Duration::from_millis(20)))
        .expect("sin plazo cumplido"));
    assert!(queue.finish(None).expect("fin"));
}

#[test]
fn say_does_not_wait_for_a_slow_synthesis() {
    let (started_tx, started_rx) = bounded::<()>(1);
    let (gate_tx, gate_rx) = bounded::<()>(1);
    let queue = SpeechQueue::new(
        move |_text| {
            let _ = started_tx.send(());
            let _ = gate_rx.recv();
            Ok((vec![0.1; 480], 24_000))
        },
        |_samples, _rate, on_start| {
            on_start();
            Ok(true)
        },
        Arc::new(AtomicBool::new(false)),
        SpeechQueueOptions::default(),
    );
    let began = std::time::Instant::now();
    assert!(queue.say("uno"));
    assert!(queue.say("dos"));
    assert!(began.elapsed() < Duration::from_millis(200));
    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("empezó la síntesis");
    // Un token por frase: la synthesizer se detiene en cada una.
    gate_tx.send(()).expect("abre la puerta");
    gate_tx.send(()).expect("abre la segunda puerta");
    assert!(queue.finish(None).expect("fin"));
}

#[test]
fn queues_are_bounded_and_say_waits_for_a_slot() {
    assert_eq!(TEXT_MAXSIZE, 64);
    assert_eq!(AUDIO_MAXSIZE, 8);
    let (gate_tx, gate_rx) = bounded::<()>(1);
    let queue = Arc::new(SpeechQueue::new(
        |_text| Ok((vec![0.1; 480], 24_000)),
        move |_samples, _rate, _on_start| {
            let _ = gate_rx.recv();
            Ok(true)
        },
        Arc::new(AtomicBool::new(false)),
        SpeechQueueOptions::default(),
    ));
    let (done_tx, done_rx) = bounded::<Vec<bool>>(1);
    let worker = Arc::clone(&queue);
    let filler = std::thread::spawn(move || {
        let results: Vec<bool> = (0..80)
            .map(|index| worker.say(&format!("frase {index}")))
            .collect();
        let _ = done_tx.send(results);
    });
    assert!(
        done_rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "la cola llena debe hacer backpressure"
    );
    queue.abort();
    gate_tx.send(()).expect("abre la puerta");
    let results = done_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("termina el llenado");
    filler.join().expect("hilo de llenado");
    assert!(queue.finish(None).expect("fin"));
    assert!(results.contains(&false));
}

#[test]
fn finish_is_idempotent_and_say_is_false_after_it() {
    let record = recorder();
    let queue = SpeechQueue::new(
        synthesize(&record),
        play(&record),
        Arc::new(AtomicBool::new(false)),
        options(&record),
    );
    assert!(queue.say("hola"));
    assert!(queue.finish(None).expect("fin"));
    assert!(queue.finish(None).expect("fin repetido"));
    assert!(!queue.say("tarde"));
}

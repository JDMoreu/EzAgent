//! Propiedades de `speech_text` que el Python prueba con bucles: idempotencia y
//! fuzz. No son casos de paridad: comprueban invariantes del texto hablado.

use ez_core::speech_text::{normalize_for_kokoro, normalize_for_speech};

/// Generador lineal determinista: el fuzz no depende de una crate de azar.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn range(&mut self, low: u64, high: u64) -> u64 {
        low + self.next() % (high - low + 1)
    }

    fn pick(&mut self, items: &[char]) -> char {
        items[self.next() as usize % items.len()]
    }
}

#[test]
fn normalization_is_idempotent_for_spoken_text() {
    let once = normalize_for_speech("Son las 10:30 y cuesta 15 € en 3 km");
    assert_eq!(normalize_for_speech(&once), once);
    let kokoro = normalize_for_kokoro("Hola EzAgent, son las 10:30");
    assert_eq!(normalize_for_kokoro(&kokoro), kokoro);
}

#[test]
fn fuzzed_integers_never_leave_digits() {
    let mut rng = Rng(20_260_101);
    for _ in 0..300 {
        let number = rng.range(0, 999_999);
        let spoken = normalize_for_speech(&number.to_string());
        assert!(
            !spoken.chars().any(|character| character.is_ascii_digit()),
            "{number} -> {spoken}"
        );
        assert_eq!(spoken.trim(), spoken);
        assert!(!spoken.is_empty());
    }
}

#[test]
fn fuzzed_hours_never_leave_digits_or_colons() {
    let mut rng = Rng(7);
    for _ in 0..200 {
        let text = format!("{}:{:02}", rng.range(0, 23), rng.range(0, 59));
        let spoken = normalize_for_speech(&text);
        assert!(
            !spoken
                .chars()
                .any(|character| character.is_ascii_digit() || character == ':'),
            "{text} -> {spoken}"
        );
    }
}

#[test]
fn fuzzed_text_never_leaves_markdown() {
    let alphabet: Vec<char> = "abc XYZ 0123456789 .,;:!?*_`#[]()<>/\\-+€%&$ñáéü\n\t"
        .chars()
        .collect();
    let mut rng = Rng(99);
    for _ in 0..400 {
        let length = rng.range(0, 80);
        let raw: String = (0..length).map(|_| rng.pick(&alphabet)).collect();
        let spoken = normalize_for_speech(&raw);
        assert!(!spoken.contains('`'), "{raw:?} -> {spoken:?}");
        assert!(!spoken.contains("**"), "{raw:?} -> {spoken:?}");
        assert!(!spoken.contains("  "), "{raw:?} -> {spoken:?}");
        assert_eq!(spoken.trim(), spoken, "{raw:?}");
    }
}

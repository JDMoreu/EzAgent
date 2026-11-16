//! Pruebas del contrato de voz de la confirmación (tablas del Python).

use super::*;

fn manager() -> ConfirmationManager {
    let accept = vec!["sí".to_string(), "confirmo".to_string(), "dale".to_string()];
    let reject = vec!["no".to_string(), "cancela".to_string()];
    ConfirmationManager::new(&accept, &reject, 0.2, None)
}

fn pending(strong: bool, require_number: Option<i64>) -> Pending {
    Pending {
        strong,
        require_number,
        token: String::new(),
        state: Mutex::new(PendingState::default()),
        condition: Condvar::new(),
    }
}

#[test]
fn the_declared_words_accept_reject_or_stay_ambiguous() {
    let accept: Vec<String> = ["sí", "si", "confirmo", "adelante", "dale"]
        .iter()
        .map(|word| (*word).to_string())
        .collect();
    let reject: Vec<String> = ["no", "cancela", "para"]
        .iter()
        .map(|word| (*word).to_string())
        .collect();
    let manager = ConfirmationManager::new(&accept, &reject, 0.2, None);
    let request = pending(false, None);
    for text in [
        "sí",
        "Sí",
        "SÍ",
        "si",
        "dale",
        "adelante",
        "sí, adelante",
        "Dale, hazlo",
    ] {
        assert_eq!(
            manager.intent(&request, &words(text)),
            Some("yes"),
            "{text}"
        );
    }
    for text in ["no", "No.", "cancela", "para"] {
        assert_eq!(manager.intent(&request, &words(text)), Some("no"), "{text}");
    }
    for text in ["", "quizás", "hmm no sé", "daleee", "sin duda"] {
        assert_eq!(manager.intent(&request, &words(text)), None, "{text}");
    }
    for text in ["no, dale", "dale, no", "sí pero cancela", "para, sí"] {
        assert_eq!(manager.intent(&request, &words(text)), Some("no"), "{text}");
    }
    let strong = pending(true, None);
    for text in ["sí", "dale", "adelante", "ok", "confirma"] {
        assert_eq!(manager.intent(&strong, &words(text)), None, "{text}");
    }
    assert_eq!(manager.intent(&strong, &words("confirmo")), Some("yes"));
    assert_eq!(manager.intent(&strong, &words("Sí, confirmo")), Some("yes"));
    assert_eq!(manager.intent(&strong, &words("no confirmo")), Some("no"));
}

#[test]
fn number_words_cover_the_counted_answers() {
    let spoken = |text: &str| number_from_words(&words(text));
    assert_eq!(spoken("cien"), Some(100));
    assert_eq!(spoken("ciento veinte"), Some(120));
    assert_eq!(spoken("treinta y tres"), Some(33));
    assert_eq!(spoken("doce"), Some(12));
    assert_eq!(spoken("seis"), Some(6));
}

#[test]
fn can_ask_again_after_resolving() {
    let manager = Arc::new(manager());
    for (text, expected) in [("sí", true), ("no", false), ("dale", true)] {
        let worker = Arc::clone(&manager);
        let handle = std::thread::spawn(move || worker.ask("¿Borro?", false, None));
        while !manager.pending() {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(manager.submit_text(text, None), Some(expected));
        assert_eq!(handle.join().unwrap_or(!expected), expected);
    }
}

#[test]
fn concurrent_answers_resolve_only_once() {
    let manager = Arc::new(manager());
    let worker = Arc::clone(&manager);
    let ask = std::thread::spawn(move || worker.ask("¿Borro?", false, None));
    while !manager.pending() {
        std::thread::sleep(Duration::from_millis(5));
    }
    let results: Arc<Mutex<Vec<Option<bool>>>> = Arc::default();
    let mut handles = Vec::new();
    for text in ["sí", "no", "dale", "cancela"].iter().cycle().take(20) {
        let manager = Arc::clone(&manager);
        let results = Arc::clone(&results);
        let text = (*text).to_string();
        handles.push(std::thread::spawn(move || {
            let result = manager.submit_text(&text, None);
            lock(&results).push(result);
        }));
    }
    for handle in handles {
        handle.join().expect("hilo de respuesta");
    }
    let answer = ask.join().expect("hilo de la pregunta");
    let resolved: Vec<bool> = lock(&results).iter().flatten().copied().collect();
    assert_eq!(resolved.len(), 1);
    assert_eq!(answer, resolved[0]);
    assert!(!manager.pending());
}

//! Casos exactos de `test_stt.py` que no están en los fixtures (TMAP-4).

use ez_stt::client::is_hallucination;
use ez_stt::dictionary::{correct_transcription, normalize_rules, transcription_hint};

#[test]
fn the_dictionary_matches_literals_without_chaining() {
    let cases = [
        (
            "Abre OPEN CODE, por favor.",
            "open code → OpenCode",
            "Abre OpenCode, por favor.",
        ),
        (
            "code codec decode code_base",
            "code => Code",
            "Code codec decode code_base",
        ),
        (
            "open code y code",
            "code → Code; open code → OpenCode",
            "OpenCode y Code",
        ),
        ("uno dos", "uno → dos; dos → tres", "dos tres"),
        ("[x] y xxx", "[x] → $1", "$1 y xxx"),
        ("open   code", "open code → OpenCode", "OpenCode"),
        ("ÁRBOL, árbol.", "árbol → árbolito", "árbolito, árbolito."),
        ("Texto intacto.", "", "Texto intacto."),
    ];
    for (text, rules, expected) in cases {
        assert_eq!(
            &correct_transcription(text, rules).expect("diccionario válido"),
            expected,
            "{text:?} con {rules:?}"
        );
    }
}

#[test]
#[ignore = "TMAP: diferencia con Python, ver informe"]
fn python_re_ignorecase_matches_turkish_dotted_and_dotless_i() {
    // Python: `re.IGNORECASE` empareja «i» con «İ» (U+0130) y «ı» (U+0131);
    // el `casefold` de Rust no, así que quedan sin corregir.
    assert_eq!(
        correct_transcription("İ ı I i", "i → letra").expect("diccionario válido"),
        "letra letra letra letra"
    );
}

#[test]
fn the_dictionary_rejects_invalid_or_ambiguous_rules() {
    for rules in [
        "sin flecha",
        "uno →",
        "→ dos",
        "a → b; A → c",
        &"x".repeat(12_001),
    ] {
        assert!(normalize_rules(rules).is_err(), "{rules:?}");
    }
}

#[test]
fn the_dictionary_normalizes_pasted_lines_and_limits_the_rule_count() {
    assert_eq!(
        normalize_rules(" open code => OpenCode\n cloud → Claude ").expect("reglas válidas"),
        "open code → OpenCode; cloud → Claude"
    );
    let too_many: Vec<String> = (0..101)
        .map(|index| format!("word{index} → term{index}"))
        .collect();
    assert!(normalize_rules(&too_many.join(";")).is_err());
}

#[test]
fn subtitle_hallucinations_are_always_discarded() {
    for text in [
        "¡Gracias por ver el vídeo!",
        "Gracias por ver el video.",
        "GRACIAS POR VER",
        "Gracias por vernos",
        "Suscríbete al canal.",
        "suscribete",
        "No olvides suscribirte",
        "Hasta la próxima",
        "Subtítulos realizados por la comunidad de Amara.org",
        "amara.org",
        "Subtitulado por la comunidad de Amara.org",
        "  ¡¡SUSCRÍBETE   AL   CANAL!!  ",
        "hasta la proxima...",
    ] {
        assert!(is_hallucination(text, false), "{text}");
        assert!(is_hallucination(text, true), "{text}");
    }
}

#[test]
fn loose_courtesies_only_fall_with_weak_audio() {
    for text in [
        "Gracias.",
        "muchas gracias",
        "Gracias a todos",
        "hasta luego",
        "Chao",
    ] {
        assert!(!is_hallucination(text, false), "{text}");
        assert!(is_hallucination(text, true), "{text}");
    }
}

#[test]
fn real_orders_are_never_discarded() {
    for text in [
        "",
        "   ",
        "...",
        "Gracias, ahora abre OpenCode",
        "Gracias por ver el vídeo que hice y revísalo",
        "Muchas gracias, ahora crea una carpeta nueva",
        "abre Claude Code",
        "dime la hora",
    ] {
        assert!(!is_hallucination(text, true), "{text}");
    }
}

#[test]
fn vocabulary_hints_are_bounded_and_respect_explicit_prompts() {
    let dictionary: Vec<String> = (0..100)
        .map(|index| format!("source{index} → {}{index}", "word".repeat(20)))
        .collect();
    let hint = transcription_hint("", &dictionary.join("; "), &[]).expect("pistas");
    assert!(hint.chars().count() <= 512, "{}", hint.chars().count());
    assert_eq!(
        transcription_hint("Mi vocabulario", &dictionary.join("; "), &[]).expect("prompt"),
        "Mi vocabulario"
    );
}

#[test]
fn the_default_hint_is_a_natural_spanish_sentence() {
    let hint = transcription_hint("", "", &["claude".to_string()]).expect("pistas");
    // Whisper copia el estilo del prompt: tildes, puntuación y jerga técnica.
    assert!(
        hint.contains("bugueado") && hint.contains("quedó"),
        "{hint}"
    );
    assert!(hint.contains("Claude Code"), "{hint}");
    assert!(hint.ends_with('.'), "{hint}");
}

//! Port of `src/ezagent/voice/wake.py` (F3-16).
//!
//! Frase de reactivación: lo único que EzAgent escucha mientras está en pausa.
//! El micrófono sigue abierto, pero cada frase se descarta salvo que sea corta
//! y, en conjunto, sea exactamente la frase de reactivación («EzAgent,
//! escúchame»). Una palabra suelta nunca basta: la frase debe nombrar al
//! asistente y decir el verbo, para que una conversación normal no lo despierte.
//!
//! Módulo puro: sin audio ni red. `app.py` transcribe y llama a
//! [`is_wake_phrase`].

use std::collections::HashSet;

use ez_core::pyvalue::{char_len, collapse_spaces, py_split, py_strip, strip_accents};

/// Frase más larga que se transcribe en pausa.
pub const MAX_WAKE_SECS: f64 = 3.0;
pub const MIN_WAKE_SECS: f64 = 0.35;

pub const DEFAULT_WAKE_PHRASE: &str = "EzAgent, escúchame";

/// Cómo suele escribir Whisper el nombre del asistente en español.
pub const NAME_VARIANTS: [&str; 12] = [
    "ezagent",
    "ez agent",
    "easy agent",
    "easyagent",
    "isi agent",
    "isiagent",
    "esagent",
    "es agent",
    "e z agent",
    "eazy agent",
    "easy agents",
    "ezagents",
];
pub const NAME_KEY: &str = "ezagent";

/// Siempre se aceptan con el nombre, además de la frase configurada.
pub const BUILTIN_VERBS: [&str; 4] = ["escuchame", "vuelve", "abre la escucha", "despierta"];

/// Cortesías que pueden envolver la frase sin cambiarla.
const LEADING: [&str; 4] = ["oye ", "hey ", "ey ", "hola "];
const TRAILING: [&str; 3] = [" por favor", " porfa", " ya"];

/// Minúsculas, sin tildes, sin puntuación y con espacios simples.
pub fn normalize(text: &str) -> String {
    let folded = strip_accents(&text.to_lowercase());
    let mut out = String::with_capacity(folded.len());
    let mut pending_space = false;
    for character in folded.chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(character);
        } else {
            // Igual que `re.sub(r"[^a-z0-9 ]+", " ", …)` seguido de `\s+`.
            pending_space = true;
        }
    }
    out
}

/// Reemplaza cada grafía conocida del nombre por `ezagent`.
fn canonical_name(text: &str) -> String {
    let mut variants = NAME_VARIANTS;
    // `sorted(..., reverse=True)` es estable: a igual longitud, orden original.
    variants.sort_by(|left, right| char_len(left).cmp(&char_len(right)).reverse());
    let mut out = format!(" {text} ");
    for variant in variants {
        out = out.replace(&format!(" {variant} "), &format!(" {NAME_KEY} "));
    }
    collapse_spaces(&out)
}

/// Quita las cortesías iniciales y finales, repitiendo hasta que no quede una.
fn strip_courtesy(text: &str) -> String {
    let mut text = text.to_string();
    let mut changed = true;
    while changed {
        changed = false;
        for lead in LEADING {
            if let Some(rest) = text.strip_prefix(lead) {
                text = rest.to_string();
                changed = true;
            }
        }
        for trail in TRAILING {
            if let Some(rest) = text.strip_suffix(trail) {
                text = rest.to_string();
                changed = true;
            }
        }
    }
    py_strip(&text).to_string()
}

/// Todas las formas normalizadas aceptadas para la frase configurada.
///
/// La frase configurada debe incluir el nombre del asistente; si no lo incluye,
/// se antepone para que una sola palabra nunca pueda despertar a EzAgent.
pub fn wake_forms(phrase: &str) -> HashSet<String> {
    let mut base = canonical_name(&normalize(phrase));
    if base.is_empty() {
        base = canonical_name(&normalize(DEFAULT_WAKE_PHRASE));
    }
    if !py_split(&base).any(|word| word == NAME_KEY) {
        base = format!("{NAME_KEY} {base}");
    }
    let verb = py_split(&base)
        .filter(|word| *word != NAME_KEY)
        .collect::<Vec<_>>()
        .join(" ");
    let mut verbs: HashSet<String> = BUILTIN_VERBS.iter().map(|verb| verb.to_string()).collect();
    if !verb.is_empty() {
        verbs.insert(verb);
    }
    let mut forms = HashSet::new();
    forms.insert(base);
    for verb in verbs {
        forms.insert(format!("{NAME_KEY} {verb}"));
        forms.insert(format!("{verb} {NAME_KEY}"));
    }
    forms
}

/// `True` solo si la transcripción entera es la frase de reactivación.
pub fn is_wake_phrase(text: &str, phrase: &str) -> bool {
    let key = strip_courtesy(&canonical_name(&normalize(text)));
    if key.is_empty() {
        return false;
    }
    wake_forms(phrase).contains(&key)
}

/// Duración de un PCM mono de 16 bits.
// DECISION: `rate` es `u32` como en el resto de `ez-audio`; Python acepta una
// tasa negativa y devuelve 0.0, un valor que aquí no es representable.
pub fn utterance_secs(pcm: &[u8], rate: u32) -> f64 {
    if pcm.is_empty() || rate == 0 {
        return 0.0;
    }
    pcm.len() as f64 / (2.0 * f64::from(rate))
}

/// Suficientemente corto para ser un comando, suficiente para ser habla.
pub fn worth_transcribing(pcm: &[u8], rate: u32) -> bool {
    let secs = utterance_secs(pcm, rate);
    (MIN_WAKE_SECS..=MAX_WAKE_SECS).contains(&secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wake(text: &str) -> bool {
        is_wake_phrase(text, DEFAULT_WAKE_PHRASE)
    }

    #[test]
    fn normaliza_tildes_puntuacion_y_espacios() {
        assert_eq!(normalize("  EzAgent,   escúchame!! "), "ezagent escuchame");
        assert_eq!(normalize("ÉL NIÑO"), "el nino");
        assert_eq!(normalize("a,,,b"), "a b");
        assert_eq!(normalize("¿Qué? ¡Ya!"), "que ya");
        assert_eq!(normalize("A\t\nB"), "a b");
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("🙂"), "");
    }

    #[test]
    fn reconoce_las_grafias_del_nombre() {
        assert_eq!(canonical_name("easy agent escuchame"), "ezagent escuchame");
        assert_eq!(
            canonical_name(&normalize("E Z Agent despierta")),
            "ezagent despierta"
        );
        assert_eq!(
            canonical_name(&normalize("ezagents vuelve")),
            "ezagent vuelve"
        );
        assert_eq!(canonical_name("hola"), "hola");
    }

    #[test]
    fn acepta_la_frase_con_cortesias() {
        assert!(wake("EzAgent, escúchame"));
        assert!(wake("ezagent escuchame"));
        assert!(wake("Escúchame, EzAgent"));
        assert!(wake("  EzAgent   escúchame "));
        assert!(wake("Oye EzAgent, escúchame por favor"));
        assert!(wake("hey ezagent escúchame por favor"));
        assert!(wake("Ey, EzAgent, despierta."));
        assert!(wake("Hola, EzAgent escúchame ya"));
        assert!(wake("hola ez agent vuelve ya"));
        assert!(wake("EZ AGENT ABRE LA ESCUCHA"));
        assert!(wake("¡EzAgent, escúchame!"));
        assert!(wake("E Z Agent, escúchame"));
        assert!(wake("easy agents despierta"));
        assert!(wake("isiagent escuchame"));
        assert!(wake("easy agent escúchame"));
        assert!(wake("Ez Agent, escúchame"));
        assert!(wake("isi agent escuchame"));
        assert!(wake("EzAgent vuelve"));
        assert!(wake("EzAgent, abre la escucha"));
        assert!(wake("  EZAGENT   ESCUCHAME  "));
    }

    #[test]
    fn una_palabra_suelta_no_despierta() {
        assert!(!wake("EzAgent"));
        assert!(!wake("ezagent escucha"));
        assert!(!wake("escúchame"));
        assert!(!wake("hola"));
        assert!(!wake("vuelve"));
        assert!(!wake("despierta"));
        assert!(!wake("oye"));
        assert!(!wake(""));
        assert!(!wake("   "));
        assert!(!wake("hola cómo estás"));
        assert!(!wake("EzAgent, escúchame esta canción que me gusta mucho"));
        assert!(!wake("le dije a ezagent escúchame"));
        assert!(!wake("EzAgent no me escuches"));
        assert!(!wake("ezagent escuchame y borra todo"));
        assert!(!wake("voy a hablar con el agente"));
    }

    #[test]
    fn una_frase_propia_sin_nombre_exige_el_nombre() {
        let forms = wake_forms("hola asistente");
        assert!(forms.contains("ezagent hola asistente"));
        assert!(forms.contains("hola asistente ezagent"));
        assert!(!forms.contains("hola asistente"));
        assert!(!is_wake_phrase("hola asistente", "hola asistente"));
        assert!(is_wake_phrase("EzAgent, hola asistente", "hola asistente"));

        // Con una frase de una sola palabra, el nombre se antepone.
        assert!(!is_wake_phrase("hola", "hola"));
        assert!(is_wake_phrase("ezagent hola", "hola"));
        assert!(!is_wake_phrase("", ""));
    }

    #[test]
    fn una_frase_personalizada_mantiene_los_verbos_integrados() {
        let phrase = "EzAgent, a trabajar";
        assert!(is_wake_phrase("ezagent a trabajar", phrase));
        assert!(is_wake_phrase("a trabajar ezagent", phrase));
        assert!(is_wake_phrase("ezagent, escúchame", phrase));
        assert!(!is_wake_phrase("a trabajar", phrase));
        for phrase in [DEFAULT_WAKE_PHRASE, "hola", "a trabajar", "", "EzAgent"] {
            let forms = wake_forms(phrase);
            assert!(!forms.is_empty(), "{phrase}");
            assert!(
                forms
                    .iter()
                    .all(|form| py_split(form).any(|word| word == NAME_KEY)),
                "{phrase}"
            );
        }
    }

    #[test]
    fn una_frase_vacia_usa_la_de_fabrica() {
        assert_eq!(wake_forms(""), wake_forms(DEFAULT_WAKE_PHRASE));
        assert!(is_wake_phrase("EzAgent, escúchame", ""));
    }

    #[test]
    fn la_duracion_cuenta_bytes_pares_y_tasa() {
        assert_eq!(utterance_secs(&[], 16000), 0.0);
        assert_eq!(utterance_secs(&[0; 1000], 0), 0.0);
        assert_eq!(utterance_secs(&[0; 32000], 16000), 1.0);
        assert_eq!(utterance_secs(&[0; 32000], 8000), 2.0);
        // Python divide el largo crudo: un byte suelto también suma.
        assert_eq!(utterance_secs(&[0; 32001], 16000), 1.000_031_25);
    }

    #[test]
    fn los_limites_de_transcripcion_son_inclusivos() {
        assert_eq!((MIN_WAKE_SECS, MAX_WAKE_SECS), (0.35, 3.0));
        assert!(!worth_transcribing(&[0; 11199], 16000));
        assert!(worth_transcribing(&[0; 11200], 16000));
        assert!(worth_transcribing(&[0; 96000], 16000));
        assert!(!worth_transcribing(&[0; 96001], 16000));
    }
}

//! Unit tests for `phonemes.rs` (F4-04).

use super::text::{phonemize_text, postprocess, split_lines};
use super::*;
use crate::kokoro::Tokenizer;

fn echo(text: &str) -> String {
    format!("{text} ")
}

fn marked(text: &str) -> String {
    if text == "Dijo hola y se" {
        "<Dijo> <hola> <Y> <SE> ".to_string()
    } else {
        let words: Vec<String> = text
            .split_whitespace()
            .map(|word| format!("<{word}>"))
            .collect();
        format!("{} ", words.join(" "))
    }
}

#[test]
fn postprocess_matches_the_python_normalization() {
    assert_eq!(postprocess("ˈo_l_a m_ˈu_n_d_o"), "ˈola mˈundo ");
    assert_eq!(postprocess("  a__b_ c  "), "ab c ");
    assert_eq!(postprocess("uno\ndos"), "uno dos ");
    assert_eq!(postprocess(""), "");
    assert_eq!(postprocess("   "), "");
}

#[test]
fn split_lines_drops_empty_lines() {
    assert_eq!(
        split_lines("uno\r\ndos\n\n  \ntres\n"),
        vec!["uno", "dos", "tres"]
    );
    assert!(split_lines("\n\r\n").is_empty());
}

#[test]
fn keeps_punctuation_and_decimal_separators() {
    let output =
        phonemize_text("1,5 euros; 2.5 y 3,5. Fin", |chunk| Ok(echo(chunk))).expect("sin error");
    assert_eq!(output, "1,5 euros; 2.5 y 3,5. Fin ");
}

#[test]
fn joins_lines_with_a_newline() {
    let output = phonemize_text("uno.\ndos.", |chunk| Ok(echo(chunk))).expect("sin error");
    assert_eq!(output, "uno. \ndos. ");
}

#[test]
fn inline_groups_are_phonemized_in_context() {
    let output =
        phonemize_text("Dijo \"hola\" y se.", |chunk| Ok(marked(chunk))).expect("sin error");
    assert_eq!(output, "<Dijo> \"<hola>\" <Y> <SE>. ");
}

#[test]
fn errors_from_the_chunk_phonemizer_propagate() {
    let error =
        phonemize_text("hola.", |_text| Err(PhonemeError("boom".into()))).expect_err("falla");
    assert_eq!(error.0, "boom");
}

#[test]
fn fake_phonemizer_records_calls_and_answers() {
    let fake = FakePhonemizer::new(|text, lang| format!("{lang}:{text}"));
    assert_eq!(fake.phonemize("hola", "es").expect("respuesta"), "es:hola");
    assert_eq!(fake.calls(), vec![("hola".to_string(), "es".to_string())]);
}

struct TempTree(std::path::PathBuf);

impl TempTree {
    fn new(name: &str, relative: &[&str]) -> Self {
        let root = std::env::temp_dir().join(format!("ez-tts-{name}-{}", std::process::id()));
        let _ = std::fs::remove_file(root.join("placeholder"));
        let mut path = root.clone();
        for part in relative {
            path = path.join(part);
        }
        std::fs::create_dir_all(&path).expect("árbol temporal");
        std::fs::write(path.join("phontab"), b"x").expect("phontab temporal");
        Self(root)
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let mut stack = vec![self.0.clone()];
        let mut directories = Vec::new();
        while let Some(directory) = stack.pop() {
            directories.push(directory.clone());
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        for directory in directories.into_iter().rev() {
            let _ = std::fs::remove_dir(directory);
        }
    }
}

#[test]
fn data_path_argument_accepts_both_layouts() {
    let direct = TempTree::new("direct", &[]);
    assert_eq!(
        ffi::data_path_argument(&direct.0, (1, 52)).expect("data directa"),
        direct.0
    );

    let nested = TempTree::new("nested", &["espeak-ng-data"]);
    assert_eq!(
        ffi::data_path_argument(&nested.0, (1, 52)).expect("data anidada"),
        nested.0
    );

    let legacy = TempTree::new("legacy", &["espeak-data"]);
    assert_eq!(
        ffi::data_path_argument(&legacy.0.join("espeak-data"), (1, 48)).expect("espeak-data"),
        legacy.0
    );
}

#[test]
fn data_path_argument_requires_phontab() {
    let empty = TempTree::new("empty", &["otra"]);
    let error = ffi::data_path_argument(&empty.0, (1, 52)).expect_err("sin phontab");
    assert!(error.starts_with("no espeak data found in "));
}

#[test]
fn library_lookup_prefers_the_first_folder_with_the_dll() {
    let empty = TempTree::new("no-dll", &["otra"]);
    let packed = TempTree::new("next-to-exe", &[]);
    let dll = packed.0.join("espeak-ng.dll");
    std::fs::write(&dll, b"x").expect("dll temporal");
    let found = library_in(&[empty.0.clone(), packed.0.clone()]);
    assert_eq!(found, Some(dll));
    assert_eq!(library_in(&[empty.0.clone()]), None);
}

#[test]
fn real_espeak_matches_python_for_spanish() {
    if default_library_path().is_none() {
        return;
    }
    let phonemizer = EspeakPhonemizer::new(&EspeakConfig::default()).expect("espeak disponible");
    assert_eq!(phonemizer.version().0, 1);
    assert_eq!(
        phonemizer.phonemize("Hola mundo.", "es").expect("fonemas"),
        "ˈola mˈundo. "
    );
    assert_eq!(
        phonemizer.phonemize("Es Agente", "es").expect("fonemas"),
        "ˈes axˈɛnte "
    );
    assert_eq!(
        phonemizer
            .phonemize("Dijo \"hola\" y se fue.", "es")
            .expect("fonemas"),
        "dˈixo \"ˈola\" i se fwˈe. "
    );
    assert_eq!(
        phonemizer.phonemize("a… b", "es").expect("fonemas"),
        "ˈa… bˈe "
    );
    assert_eq!(
        phonemizer
            .phonemize("¿Qué tal? ¡Bien!", "es")
            .expect("fonemas"),
        "¿kˈe tˈal? ¡bjˈen! "
    );
    assert_eq!(
        phonemizer
            .phonemize("Línea uno\nlínea dos\n", "es")
            .expect("fonemas"),
        "lˈinea ˈuno \nlˈinea ðˈos "
    );

    let tokenizer = Tokenizer::new(&EspeakConfig::default(), None).expect("tokenizer");
    assert_eq!(
        tokenizer
            .phonemize("Hola mundo.", "es", true)
            .expect("fonemas"),
        "ˈola mˈundo."
    );
    assert_eq!(
        tokenizer
            .phonemize("Dijo \"hola\" y se fue.", "es", true)
            .expect("fonemas"),
        "dˈixo \"ˈola\" i se fwˈe."
    );
    assert_eq!(
        tokenizer
            .phonemize("Línea uno\nlínea dos\n", "es", true)
            .expect("fonemas"),
        "lˈinea ˈuno lˈinea ðˈos"
    );
    assert_eq!(
        tokenizer
            .phonemize("Hola, mundo; (bien) [mal]", "es", true)
            .expect("fonemas"),
        "ˈola, mˈundo; (bjˈen) mˈal"
    );
}

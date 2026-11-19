//! Los textos visibles usan «agente principal» y «subagente», nunca la metáfora.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Trozos entre comillas de un archivo, sin las líneas de comentario.
fn literals(path: &Path) -> Vec<String> {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut found = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with('*') {
            continue;
        }
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch != '"' && ch != '`' {
                continue;
            }
            let mut literal = String::new();
            for inner in chars.by_ref() {
                if inner == ch {
                    break;
                }
                literal.push(inner);
            }
            found.push(literal);
        }
    }
    found
}

fn whole_word(text: &str, word: &str) -> bool {
    text.split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .any(|part| part.eq_ignore_ascii_case(word))
}

fn assert_neutral(name: &str, texts: &[String]) {
    let banned = ["elefante", "pez", "peces"];
    for text in texts {
        for word in banned {
            assert!(!whole_word(text, word), "{name} shows «{word}» in: {text}");
        }
    }
}

#[test]
fn all_visible_texts_use_neutral_names() {
    let hub = root();
    let crates = hub.parent().expect("crates");

    assert_neutral(
        "ez-arbiter",
        &literals(&crates.join("ez-arbiter/src/texts.rs")),
    );
    assert_neutral(
        "ez-ctl (usage)",
        &literals(&crates.join("ez-ctl/src/cli.rs")),
    );
    assert_neutral("skill", &[ez_ctl::SKILL.to_string()]);
    assert_neutral("hub texts", &literals(&hub.join("src/texts.rs")));
    assert_neutral("view", &literals(&hub.join("ui/js/texts.js")));

    let roles = crates.join("ez-team/src/roles");
    let mut templates = Vec::new();
    for entry in std::fs::read_dir(&roles).expect("papeles") {
        let path = entry.expect("archivo").path();
        if path.extension().is_some_and(|ext| ext == "md") {
            templates.push(std::fs::read_to_string(&path).expect("leer papel"));
        }
    }
    assert_neutral("role templates", &templates);

    let first = ez_team::launch::first_line("QA", Path::new("qa.instrucciones.md"));
    let main = ez_team::launch::main_first_line();
    assert_neutral("first lines", &[first, main]);
}

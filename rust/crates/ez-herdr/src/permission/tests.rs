//! Pruebas unitarias de las piezas internas de `permission`.

use super::*;

#[test]
fn trailing_key_reads_the_shortcut_after_the_label() {
    assert_eq!("y", trailing_key("Yes, proceed (y)").expect("tecla"));
    assert_eq!(
        "esc",
        trailing_key("No, and tell Codex what to do differently (esc)").expect("tecla")
    );
    assert_eq!(None, trailing_key("Yes"));
    assert_eq!(None, trailing_key("Yes, and don't ask again (pq)"));
}

#[test]
fn parse_option_reads_numbers_arrows_and_plain_prompts() {
    assert_eq!(
        Some(("1".to_string(), "Yes".to_string())),
        parse_option("❯ 1. Yes")
    );
    assert_eq!(
        Some(("2".to_string(), "No, exit".to_string())),
        parse_option("  2. No, exit")
    );
    assert_eq!(
        Some((String::new(), "No, exit".to_string())),
        parse_option("❯ No, exit")
    );
    assert_eq!(
        Some(("y".to_string(), "Yes, proceed (y)".to_string())),
        parse_option("› 1. Yes, proceed (y)")
    );
    assert_eq!(None, parse_option("Ask anything"));
    assert_eq!(None, parse_option("┃  $ cargo test"));
}

#[test]
fn approve_once_rejects_always_and_needs_a_unique_option() {
    let options = |rows: &[(&str, &str)]| -> Vec<(String, String)> {
        rows.iter()
            .map(|(key, text)| ((*key).to_string(), (*text).to_string()))
            .collect()
    };
    assert_eq!(
        Some("1".to_string()),
        approve_once(&options(&[
            ("1", "Yes"),
            ("2", "Yes, and don't ask again for npm test"),
            ("3", "No"),
        ]))
    );
    assert_eq!(
        Some("1".to_string()),
        approve_once(&options(&[
            ("1", "Allow once"),
            ("2", "Always allow"),
            ("3", "Reject"),
        ]))
    );
    assert_eq!(
        None,
        approve_once(&options(&[("1", "Yes, and don't ask again"), ("2", "No")]))
    );
    assert_eq!(
        None,
        approve_once(&options(&[("1", "Always allow"), ("2", "Reject")]))
    );
    assert_eq!(
        None,
        approve_once(&options(&[("1", "Yes, and switch to auto mode")]))
    );
    assert_eq!(
        None,
        approve_once(&options(&[("1", "Yes"), ("2", "Allow once")]))
    );
}

#[test]
fn folder_within_compares_components() {
    assert!(folder_within("/home/me/proj", "/home/me/proj"));
    assert!(folder_within("/home/me/proj/sub", "/home/me/proj"));
    assert!(!folder_within("/home/me/project", "/home/me/proj"));
    assert!(!folder_within("/home/me", "/home/me/proj"));
    assert!(!folder_within("/home/me/proj", ""));
}

#[test]
fn fingerprint_uses_the_last_twenty_non_empty_lines() {
    let screen: String = (1..=25)
        .map(|n| format!("línea {n}\n\n"))
        .collect::<String>();
    let fingerprint = screen_fingerprint(&screen);
    assert!(fingerprint.starts_with("línea 6"));
    assert!(fingerprint.ends_with("línea 25"));
    assert_eq!(FINGERPRINT_LINES, fingerprint.lines().count());
}

#[test]
fn excerpt_is_one_short_line_without_secrets() {
    let screen = format!("clave sk-{}\nsegunda   línea", "a".repeat(40));
    let excerpt = screen_excerpt(&screen);
    assert!(!excerpt.contains("sk-"));
    assert!(excerpt.contains("segunda línea"));
    assert!(!excerpt.contains('\n'));
    let long = screen_excerpt(&"á".repeat(500));
    assert_eq!(SCREEN_EXCERPT_CHARS, long.chars().count());
}

#[test]
fn idle_screens_are_not_dialogs() {
    let claude = "▐▛███▛█ Claude Code v2.1.319\n❯\n⏵⏵ auto mode on";
    assert_eq!(DialogKind::None, classify(claude).kind);
    let codex = ">_ OpenAI Codex (v0.167.0)\n› Ask Codex to do anything\n← for agents · ? for shortcuts ⚠ 1 warning · f2 to view";
    assert_eq!(DialogKind::None, classify(codex).kind);
    let opencode = "┃  Ask anything… \"Fix a bug in the codebase\"\n┃  Build · Big Pickle";
    assert_eq!(DialogKind::None, classify(opencode).kind);
}

#[test]
fn denied_rejects_catastrophic_commands_and_allows_normal_ones() {
    assert!(denied("rm -rf /").is_some());
    assert!(denied("rm -rf ./target").is_none());
    assert!(denied("git push -f origin main").is_some());
    assert!(denied("git push origin feature").is_none());
    assert!(denied("cargo test").is_none());
}

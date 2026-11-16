//! Pruebas unitarias de las piezas internas de `permission`.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::audit::sha256_hex;
use super::deny::{deletes_outside, path_allowed, within};
use super::dialogs::{
    approve_sequence, parse_option, parse_options, permanent_filter, positive_option, trailing_key,
    ParsedOption,
};
use super::*;

fn option(number: &str, text: &str) -> ParsedOption {
    ParsedOption {
        number: Some(number.to_string()),
        shortcut: trailing_key(text),
        text: text.to_string(),
        selected: false,
    }
}

fn plain(text: &str) -> ParsedOption {
    ParsedOption {
        number: None,
        shortcut: None,
        text: text.to_string(),
        selected: false,
    }
}

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
    let numbered = parse_option("❯ 1. Yes").expect("opción");
    assert_eq!(Some("1"), numbered.number.as_deref());
    assert_eq!("Yes", numbered.text);
    assert!(numbered.selected);

    let second = parse_option("  2. No, exit").expect("opción");
    assert_eq!("2", second.key());
    assert!(!second.selected);

    let arrow = parse_option("❯ No, exit").expect("opción");
    assert_eq!(None, arrow.number);
    assert!(arrow.selected);

    let shortcut = parse_option("› 1. Yes, proceed (y)").expect("opción");
    assert_eq!("y", shortcut.key());

    assert_eq!(None, parse_option("Ask anything"));
    assert_eq!(None, parse_option("┃  $ cargo test"));
}

#[test]
fn arrow_lists_absorb_the_unmarked_options_after_the_marker() {
    let lines = [
        " Accessing workspace:",
        "",
        " Quick safety check: Is this a project you trust?",
        "",
        " ❯ No, exit",
        "   Yes, I trust this folder",
    ];
    let options = parse_options(&lines);
    assert_eq!(2, options.len());
    assert!(options[0].selected);
    assert_eq!("No, exit", options[0].text);
    assert_eq!("Yes, I trust this folder", options[1].text);
}

#[test]
fn approve_once_rejects_permanent_options_by_whole_word() {
    assert!(!permanent_filter("Allow"));
    assert!(!permanent_filter("Allow once"));
    assert!(!permanent_filter("Yes"));
    assert!(permanent_filter("Always allow"));
    assert!(permanent_filter("Allow all"));
    assert!(permanent_filter("Yes, and don't ask again for npm test"));
    assert!(permanent_filter("for this session"));
    assert!(permanent_filter("Permitir siempre"));
    assert!(permanent_filter("Permitir todo"));
    assert!(permanent_filter("Todas las veces"));
    assert!(permanent_filter("No volver a preguntar"));
    assert!(permanent_filter("Yes, and switch to auto mode"));
    assert!(positive_option("Allow once"));
    assert!(positive_option("Sí, permitir una vez"));
    assert!(!positive_option("Reject"));
}

#[test]
fn numbered_approval_is_just_the_number() {
    let options = [
        option("1", "Yes"),
        option("2", "Yes, and don't ask again for npm test"),
        option("3", "No"),
    ];
    assert_eq!(
        Some(vec!["1".to_string()]),
        approve_sequence(&options, "Do you want to proceed?", false)
    );
}

#[test]
fn dont_ask_first_then_yes_approves_the_second_option() {
    let options = [
        option("1", "Yes, and don't ask again for rm commands"),
        option("2", "Yes"),
    ];
    assert_eq!(
        Some(vec!["2".to_string()]),
        approve_sequence(&options, " Do you want to proceed?", false)
    );
}

#[test]
fn arrow_approval_walks_from_the_marked_option_and_confirms() {
    let options = [
        ParsedOption {
            selected: true,
            ..plain("No, exit")
        },
        plain("Yes, I trust this folder"),
    ];
    assert_eq!(
        Some(vec!["down".to_string(), "enter".to_string()]),
        approve_sequence(&options, " Do you trust this folder?", true)
    );
    let upwards = [
        plain("Yes, I trust this folder"),
        ParsedOption {
            selected: true,
            ..plain("No, exit")
        },
    ];
    assert_eq!(
        Some(vec!["up".to_string(), "enter".to_string()]),
        approve_sequence(&upwards, "", true)
    );
}

#[test]
fn numbered_options_add_enter_when_the_screen_asks_for_it() {
    let options = [option("1", "Yes, proceed"), option("2", "No")];
    assert_eq!(
        Some(vec!["1".to_string(), "enter".to_string()]),
        approve_sequence(&options, " Press enter to confirm or esc to cancel", false)
    );
    assert_eq!(
        Some(vec!["1".to_string()]),
        approve_sequence(&options, " Esc to cancel · Tab to amend", false)
    );
}

#[test]
fn ambiguous_or_only_permanent_menus_have_no_approval() {
    let two = [option("1", "Yes"), option("2", "Allow once")];
    assert_eq!(None, approve_sequence(&two, "", false));
    let permanent = [option("1", "Always allow"), option("2", "Reject")];
    assert_eq!(None, approve_sequence(&permanent, "", false));
}

#[test]
fn folders_compare_by_components() {
    assert!(within(
        Path::new("/home/me/proj"),
        Path::new("/home/me/proj")
    ));
    assert!(within(
        Path::new("/home/me/proj/sub"),
        Path::new("/home/me/proj")
    ));
    assert!(!within(
        Path::new("/home/me/project"),
        Path::new("/home/me/proj")
    ));
    assert!(!within(Path::new("/home/me"), Path::new("/home/me/proj")));
}

#[test]
fn edit_paths_resolve_against_the_workdir() {
    let workdir = PathBuf::from("/repo");
    let dirs = [workdir.clone()];
    assert!(path_allowed("src/main.rs", &dirs, Some(&workdir)));
    assert!(path_allowed("./src/../src/main.rs", &dirs, Some(&workdir)));
    assert!(!path_allowed("../fuera.txt", &dirs, Some(&workdir)));
    assert!(!path_allowed("src/main.rs", &dirs, None));
    assert!(!path_allowed("/elsewhere/main.rs", &dirs, Some(&workdir)));
}

#[test]
fn recursive_deletes_outside_the_workdir_are_denied() {
    let workdir = PathBuf::from("/repo");
    let dirs = [workdir.clone()];
    assert_eq!(
        None,
        deletes_outside("rm -rf ./target", &dirs, Some(&workdir))
    );
    assert_eq!(
        None,
        deletes_outside("rm -rf target", &dirs, Some(&workdir))
    );
    assert!(deletes_outside("rm -rf ../otro", &dirs, Some(&workdir)).is_some());
    assert!(deletes_outside("rm -rf \"../otro\"", &dirs, Some(&workdir)).is_some());
    assert!(deletes_outside("rm -rf /", &dirs, Some(&workdir)).is_some());
    assert!(deletes_outside("Remove-Item -Recurse -Force C:\\x", &dirs, Some(&workdir)).is_some());
    assert!(deletes_outside("rm -rf /repo", &dirs, Some(&workdir)).is_some());
    // Las rutas con variables no se pueden comprobar sin shell.
    assert!(deletes_outside("rm -rf \"$HOME/projects\"", &dirs, Some(&workdir)).is_some());
    assert_eq!(None, deletes_outside("rm -f target", &dirs, Some(&workdir)));
    assert_eq!(None, deletes_outside("cargo test", &dirs, Some(&workdir)));
}

#[test]
fn quoted_and_prefixed_git_commands_are_denied() {
    assert!(denied("rm -rf \"$HOME\"").is_some());
    assert!(denied("rm -rf \"C:\\\"").is_some());
    assert!(denied("sudo git -C /repo reset --hard").is_some());
    assert!(denied("git -C /repo clean -fdx").is_some());
    assert!(denied("git stash list").is_none());
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
fn s1_and_s2_must_match_and_s3_must_be_clean() {
    let before = " Bash command\n  npm test\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. No\n";
    assert!(same_dialog(before, before));
    assert!(!same_dialog(before, " Bash command\n  npm test\n"));
    let idle = "▐▛███▛█ Claude Code v2.1.319\n❯\n⏵⏵ auto mode on";
    assert_eq!(Ok(()), approval_granted(idle));
    assert!(approval_granted(before).is_err());
    assert!(approval_granted("✓ Always allow npm test from now on").is_err());
    assert!(approval_granted("✓ Allowed once\n❯").is_ok());
}

#[test]
fn audit_has_no_screen_text_and_no_secrets() {
    let screen = " Bash command\n  export OPENAI_API_KEY=sk-live-1234567890abcdef\n  Do you want to proceed?\n  ❯ 1. Yes\n";
    let dialog = classify(screen);
    let audit = audit_summary(&dialog, &Decision::Approve(vec!["1".to_string()]));
    let row: Value = serde_json::from_str(&audit).expect("json");
    assert_eq!(row["kind"], "exec");
    assert_eq!(row["option"], "Yes");
    assert_eq!(row["command"], "export");
    assert_eq!(16, row["command_hash"].as_str().expect("hash").len());
    assert!(row["path"].is_null());
    assert_eq!(row["decision"], "approve:1");
    assert!(!audit.contains("sk-live"));
    assert!(!audit.contains("OPENAI_API_KEY"));
}

#[test]
fn audit_redacts_and_truncates_the_chosen_option() {
    let dialog = Dialog {
        kind: DialogKind::Exec,
        command: Some("npm test".to_string()),
        options: vec![(
            "1".to_string(),
            format!("Yes, allow sk-live-1234567890abcdef {}", "x".repeat(80)),
        )],
        approve_once: Some(vec!["1".to_string()]),
        ..Dialog::default()
    };
    let audit = audit_summary(&dialog, &Decision::Approve(vec!["1".to_string()]));
    let row: Value = serde_json::from_str(&audit).expect("json");
    let option = row["option"].as_str().expect("opción");
    assert!(option.chars().count() <= 60);
    assert!(!option.contains("sk-live"));
    assert_eq!("npm", row["command"]);
}

#[test]
fn audit_redacts_the_edited_path() {
    let dialog = Dialog {
        kind: DialogKind::Edit,
        path: Some("sk-live-1234567890abcdef/.env".to_string()),
        ..Dialog::default()
    };
    let audit = audit_summary(&dialog, &Decision::Ask("x".to_string()));
    let row: Value = serde_json::from_str(&audit).expect("json");
    assert!(!audit.contains("sk-live"));
    assert!(row["path"].as_str().expect("ruta").contains("[REDACTED]"));
}

#[test]
fn sha256_matches_the_standard_vectors() {
    assert_eq!(
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        sha256_hex("")
    );
    assert_eq!(
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        sha256_hex("abc")
    );
    assert_eq!(
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        sha256_hex("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
    );
}

#[test]
fn denied_rejects_catastrophic_commands_and_allows_normal_ones() {
    assert!(denied("rm -rf /").is_some());
    assert!(denied("rm -rf ./target").is_none());
    assert!(denied("git push -f origin main").is_some());
    assert!(denied("git branch -d feature").is_none());
    assert!(denied("cargo test").is_none());
    assert!(denied_path(".env").is_some());
    assert!(denied_path("src/app.tsx").is_none());
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

#![recursion_limit = "512"]
//! Paridad de `hud_chat` contra `parity/fixtures/hud_chat.jsonl` (P-HUD-C).
//!
//! Un solo `#[test]`: los casos comparten el directorio temporal, la paleta
//! global de tokens y el reloj de la vista, así que se ejecutan en serie.

#[path = "hud_chat_parity/logic.rs"]
mod logic;
#[path = "hud_chat_parity/markdown/mod.rs"]
mod markdown;
#[path = "hud_chat_parity/morph.rs"]
mod morph;
#[path = "hud_chat_parity/panel.rs"]
mod panel;
#[path = "hud_chat_parity/render.rs"]
mod render;
#[path = "hud_chat_parity/support.rs"]
mod support;
#[path = "hud_chat_parity/view.rs"]
mod view;
#[path = "hud_chat_parity/view_input.rs"]
mod view_input;

use std::path::Path;

use ez_parity::fixtures;

const FIXTURES: &str = "../../../parity/fixtures/hud_chat.jsonl";
const CASE_COUNT: usize = 221;

/// `<tmp>` del oráculo. Solo se usan los nombres: no se escribe nada.
///
/// El oráculo guarda números que dependen de la longitud de su `HOME` (el
/// cursor de `view-add-files`), así que la raíz se rellena hasta la longitud
/// con la que se generó el fixture (42 caracteres) para reproducirlos.
fn temp_root() -> String {
    let base = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| {
            std::env::temp_dir()
                .join(format!("ezagent-chat-{}", std::process::id()))
                .to_string_lossy()
                .into_owned()
        });
    const ORACLE_LEN: usize = 53;
    let padding = ORACLE_LEN.saturating_sub(base.chars().count() + 1);
    if padding == 0 {
        base
    } else {
        format!("{base}\\{}", "e".repeat(padding))
    }
}

static PNG_FAILURES: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> =
    std::sync::OnceLock::new();

fn png_failures() -> &'static std::sync::Mutex<Vec<String>> {
    PNG_FAILURES.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

#[test]
fn every_chat_case_matches_python() {
    let root = temp_root();
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de chat legibles");
    let mut cases_run = 0;
    for case in &cases {
        if case.function == "ChatPanel._compose" {
            morph::morph(case);
        } else if case.function == "ChatView.render" {
            panel::panel(case);
        } else if case.function == "render_message" {
            render::render(case, &root);
        } else if case.function.starts_with("chat_view.ChatView.") {
            view::run(case, &case.function, &root);
        } else {
            logic::run(case, &case.function);
        }
        cases_run += 1;
    }
    let cases_in_file = cases.len();
    assert_eq!(
        cases_run, cases_in_file,
        "faltan casos por ejecutar en {FIXTURES}"
    );
    assert_eq!(cases_in_file, CASE_COUNT, "faltan casos en {FIXTURES}");
    let failures = png_failures().lock().expect("sin envenenar");
    assert!(
        failures.is_empty(),
        "cuadros:
{}",
        failures.join(
            "
"
        )
    );
}

//! Uso incorrecto, entorno ausente y códigos de salida de `ez`.

mod support;

use std::sync::Arc;

use ez_ctl::{Caller, CtlServer, Handler, Reply, Request, Tokens};
use support::{run_at, run_without_env, Harness, TempDir, TOKEN};

const MISSING_ENV: &str =
    "Este comando solo funciona dentro de una terminal abierta por EzAgent (faltan EZ_URL y EZ_TOKEN).";

#[test]
fn skill_help_and_version_work_without_a_server() {
    let dir = TempDir::new();

    let (code, out, _) = run_without_env(&["--skill"], dir.path());
    assert_eq!(code, 0);
    assert!(
        out.contains("# Manual del elefante"),
        "{}",
        &out[..80.min(out.len())]
    );

    let (code, out, _) = run_without_env(&["--help"], dir.path());
    assert_eq!(code, 0);
    assert!(out.contains("Uso:"));
    assert!(out.contains("ez report --verdict"));

    let (code, out, _) = run_without_env(&["--version"], dir.path());
    assert_eq!(code, 0);
    assert_eq!(out.trim(), env!("CARGO_PKG_VERSION"));
}

#[test]
fn missing_environment_is_exit_3() {
    let dir = TempDir::new();
    let (code, out, err) = run_without_env(&["status"], dir.path());
    assert_eq!(code, 3);
    assert!(out.is_empty());
    assert_eq!(err.trim(), MISSING_ENV);
}

#[test]
fn a_stopped_server_is_exit_3() {
    let mut harness = Harness::new();
    let url = harness.url.clone();
    harness.stop();
    let (code, _, err) = run_at(&url, TOKEN, harness.dir.path(), &["status"]);
    assert_eq!(code, 3);
    assert_eq!(
        err.trim(),
        format!("No pude conectar con EzAgent en {url}. ¿Está abierta la ventana de EzAgent?")
    );
}

#[test]
fn an_unknown_token_is_exit_3() {
    let mut harness = Harness::new();
    let (code, _, err) = harness.run_as("token-que-no-existe", &["status"]);
    assert_eq!(code, 3);
    assert_eq!(err.trim(), "Token inválido o ausente.");
    harness.stop();
}

#[test]
fn a_rejected_request_is_exit_1_and_prints_its_message() {
    struct Rejecting;

    impl Handler for Rejecting {
        fn handle(&self, _caller: &Caller, _request: &Request) -> Reply {
            Reply::failure("Todavía no: hacen falta dos rondas limpias seguidas.")
        }
    }

    let tokens = Arc::new(Tokens::new());
    tokens.register(
        "token-prueba",
        Caller::Elephant {
            space: "espacio".to_string(),
        },
    );
    let server = CtlServer::start(Arc::new(Rejecting), tokens).expect("servidor");
    let dir = TempDir::new();
    let (code, out, err) = run_at(&server.url(), "token-prueba", dir.path(), &["close"]);
    assert_eq!(code, 1);
    assert_eq!(
        out.trim(),
        "Todavía no: hacen falta dos rondas limpias seguidas."
    );
    assert!(err.is_empty());
    server.stop();
}

#[test]
fn misuse_is_exit_2_with_usage() {
    let dir = TempDir::new();
    let brief = dir.file("encargo.md");
    let brief = brief.to_string_lossy().into_owned();
    let cases: [&[&str]; 9] = [
        &["bailar"],
        &["task", "new", "--title", "T", "--brief", &brief],
        &["fish", "", "--brief", &brief],
        &["fish", "developer"],
        &["report", "--verdict", "quizás", "--file", &brief],
        &["report", "--verdict", "pass", "--file", &brief],
        &[
            "report",
            "--verdict",
            "pass",
            "--file",
            &brief,
            "--blocking",
            "0",
            "--minor",
            "0",
        ],
        &["status", "de-más"],
        &["status", "--verbose", "x"],
    ];
    for args in cases {
        let (code, out, err) = run_without_env(args, dir.path());
        assert_eq!(code, 2, "{}", args.join(" "));
        assert!(out.is_empty(), "{}", args.join(" "));
        assert!(err.contains("Uso:"), "{}: {err}", args.join(" "));
    }

    let (code, _, err) = run_without_env(
        &[
            "report",
            "--verdict",
            "pass",
            "--file",
            &brief,
            "--blocking",
            "0",
            "--minor",
            "0",
        ],
        dir.path(),
    );
    assert_eq!(code, 2);
    assert!(err.contains("Falta --major."), "{err}");

    let (code, _, err) = run_without_env(&["pause", "--reason"], dir.path());
    assert_eq!(code, 2);
    assert!(err.contains("Falta el valor de --reason."), "{err}");
}

#[test]
fn a_missing_file_is_exit_2_with_its_path() {
    let dir = TempDir::new();
    let missing = dir.path().join("no-existe.md");
    let criteria = dir.file("criterios.toml");
    let criteria = criteria.to_string_lossy().into_owned();
    let (code, out, err) = run_without_env(
        &[
            "task",
            "new",
            "--title",
            "T",
            "--brief",
            "no-existe.md",
            "--criteria",
            &criteria,
        ],
        dir.path(),
    );
    assert_eq!(code, 2);
    assert!(out.is_empty());
    assert_eq!(
        err.trim(),
        format!("No existe el archivo: {}", missing.display())
    );
}

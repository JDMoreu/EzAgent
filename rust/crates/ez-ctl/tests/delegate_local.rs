//! `ez delegate`: el análisis de las opciones, el modo árbitro y el modo local
//! de Herdr.

mod support;

use std::path::Path;

use ez_ctl::parse_delegate_options;
use support::{program, run, run_at, Harness, TempDir, TOKEN};

const ARBITRATOR_OPTIONS: &str = "En una tarea del árbitro el tipo sale del equipo aprobado (llega con la integración del árbitro).";
const MISSING_TERMINAL: &str = "Ejecútalo desde la terminal de un agente en Herdr.";

/// Ejecuta `ez` con solo `HC_TERMINAL_ID` en el entorno (modo local).
fn run_local(argv: &[&str], cwd: &Path, terminal: Option<&str>) -> (i32, String, String) {
    let terminal = terminal.map(str::to_string);
    let env = move |name: &str| match name {
        "HC_TERMINAL_ID" => terminal.clone(),
        _ => None,
    };
    run(&program(argv), &env, cwd)
}

fn options(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn el_analisis_toma_todas_las_opciones_del_modo_local() {
    let dir = TempDir::new();
    let brief = dir.file("encargo.md");
    let brief = brief.to_string_lossy().into_owned();
    let parsed = parse_delegate_options(
        &options(&[
            "developer",
            "--agent",
            "opencode",
            "--model",
            "deepseek-v4.1-flash",
            "--effort",
            "high",
            "--brief",
            &brief,
            "--name",
            "dev1-opencode",
            "--show",
            "tab",
            "--cwd",
            "C:\\ws",
            "--outlive-parent",
        ]),
        dir.path(),
    )
    .expect("análisis");
    assert_eq!(parsed.role, "developer");
    assert_eq!(parsed.agent.as_deref(), Some("opencode"));
    assert_eq!(parsed.model.as_deref(), Some("deepseek-v4.1-flash"));
    assert_eq!(parsed.effort.as_deref(), Some("high"));
    assert_eq!(parsed.brief, brief);
    assert_eq!(parsed.name.as_deref(), Some("dev1-opencode"));
    assert_eq!(parsed.show.as_str(), "tab");
    assert_eq!(parsed.cwd.as_deref(), Some("C:\\ws"));
    assert!(parsed.outlive_parent);
    assert!(!parsed.notify);
    assert!(parsed.local_options);
}

#[test]
fn el_analisis_rechaza_el_nombre_y_la_ubicacion_invalidos() {
    let dir = TempDir::new();
    let brief = dir.file("encargo.md");
    let brief = brief.to_string_lossy().into_owned();
    let bad_name = parse_delegate_options(
        &options(&[
            "developer",
            "--agent",
            "opencode",
            "--brief",
            &brief,
            "--name",
            "Malo",
        ]),
        dir.path(),
    );
    assert!(bad_name.is_err(), "{bad_name:?}");
    let bad_show = parse_delegate_options(
        &options(&[
            "developer",
            "--agent",
            "opencode",
            "--brief",
            &brief,
            "--show",
            "panel",
        ]),
        dir.path(),
    );
    assert!(bad_show.is_err(), "{bad_show:?}");
    let no_agent = parse_delegate_options(&options(&["developer", "--brief", &brief]), dir.path())
        .expect("sin --agent vale: lo elige la recomendación");
    assert!(no_agent.agent.unwrap_or_default().is_empty());
    let unknown = parse_delegate_options(
        &options(&[
            "developer",
            "--agent",
            "opencode",
            "--brief",
            &brief,
            "--otro",
            "x",
        ]),
        dir.path(),
    );
    assert!(unknown.is_err(), "{unknown:?}");
}

#[test]
fn en_una_tarea_del_arbitro_las_opciones_locales_son_un_uso_incorrecto() {
    let mut harness = Harness::new();
    let dir = TempDir::new();
    let brief = dir.file("encargo.md");
    let brief = brief.to_string_lossy().into_owned();
    for extra in [
        vec!["--agent", "opencode"],
        vec!["--model", "sonnet"],
        vec!["--effort", "high"],
        vec!["--name", "dev1"],
        vec!["--show", "tab"],
        vec!["--cwd", "C:\\ws"],
        vec!["--no-notify"],
        vec!["--outlive-parent"],
    ] {
        let mut argv = vec!["delegate", "developer", "--brief", brief.as_str()];
        argv.extend(extra.iter().copied());
        let (code, out, err) = run_at(&harness.url, TOKEN, dir.path(), &argv);
        assert_eq!(code, 2, "{}", argv.join(" "));
        assert!(out.is_empty(), "{}", argv.join(" "));
        assert!(err.contains(ARBITRATOR_OPTIONS), "{err}");
    }
    harness.stop();
}

#[test]
fn en_una_tarea_del_arbitro_el_delegate_sigue_siendo_el_de_hoy() {
    let mut harness = Harness::new();
    let dir = TempDir::new();
    let brief = dir.file("encargo.md");
    let brief = brief.to_string_lossy().into_owned();
    let (code, _, err) = run_at(
        &harness.url,
        TOKEN,
        dir.path(),
        &["delegate", "developer", "--brief", &brief],
    );
    assert_eq!(code, 0, "{err}");
    let calls = harness.handler.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].1.command, "agent.delegate");
    assert_eq!(calls[0].1.args["role"], "developer");
    assert_eq!(calls[0].1.args["brief"], brief);
    harness.stop();
}

#[test]
fn el_analisis_acepta_sin_agente_y_con_force() {
    let dir = TempDir::new();
    let brief = dir.file("encargo.md");
    let brief = brief.to_string_lossy().into_owned();
    let parsed = parse_delegate_options(
        &options(&["qa_validator", "--brief", &brief, "--force"]),
        dir.path(),
    )
    .expect("análisis");
    assert!(parsed.agent.is_none());
    assert!(parsed.force);
    assert!(parsed.local_options);

    let empty = parse_delegate_options(
        &options(&["developer", "--brief", &brief, "--agent", ""]),
        dir.path(),
    );
    assert!(empty.is_err(), "{empty:?}");
}

#[test]
fn sin_hc_terminal_id_el_modo_local_es_codigo_3() {
    let dir = TempDir::new();
    let brief = dir.file("encargo.md");
    let brief = brief.to_string_lossy().into_owned();
    let (code, out, err) = run_local(
        &[
            "delegate",
            "developer",
            "--agent",
            "opencode",
            "--brief",
            &brief,
        ],
        dir.path(),
        None,
    );
    assert_eq!(code, 3);
    assert!(out.is_empty());
    assert!(err.contains(MISSING_TERMINAL), "{err}");

    // Sin `--agent` el análisis también pasa; el modo local solo exige Herdr.
    let (code, out, err) = run_local(
        &["delegate", "developer", "--brief", &brief],
        dir.path(),
        None,
    );
    assert_eq!(code, 3);
    assert!(out.is_empty());
    assert!(err.contains(MISSING_TERMINAL), "{err}");
}

#[test]
fn un_encargo_demasiado_grande_es_codigo_2() {
    let dir = TempDir::new();
    let brief = dir.path().join("enorme.md");
    std::fs::write(&brief, "x".repeat(200_001)).expect("archivo");
    let brief = brief.to_string_lossy().into_owned();
    let (code, out, err) = run_local(
        &[
            "delegate",
            "developer",
            "--agent",
            "opencode",
            "--brief",
            &brief,
        ],
        dir.path(),
        Some("t6h"),
    );
    assert_eq!(code, 2);
    assert!(out.is_empty());
    assert!(err.contains("demasiado grande"), "{err}");
}

#[test]
fn un_encargo_que_no_es_utf8_es_codigo_2() {
    let dir = TempDir::new();
    let brief = dir.path().join("binario.md");
    std::fs::write(&brief, [0xff, 0xfe, 0x00]).expect("archivo");
    let brief = brief.to_string_lossy().into_owned();
    let (code, _, err) = run_local(
        &[
            "delegate",
            "developer",
            "--agent",
            "opencode",
            "--brief",
            &brief,
        ],
        dir.path(),
        Some("t6h"),
    );
    assert_eq!(code, 2);
    assert!(err.contains("UTF-8"), "{err}");
}

#[test]
fn el_uso_incorrecto_del_delegate_es_codigo_2() {
    let dir = TempDir::new();
    let cases: [&[&str]; 3] = [
        &["delegate"],
        &["delegate", "developer"],
        &["delegate", "developer", "--agent", "opencode"],
    ];
    for args in cases {
        let (code, out, err) = run_local(args, dir.path(), Some("t6h"));
        assert_eq!(code, 2, "{}", args.join(" "));
        assert!(out.is_empty(), "{}", args.join(" "));
        assert!(err.contains("Uso:"), "{}: {err}", args.join(" "));
    }
    let (code, out, err) = run_local(
        &[
            "delegate",
            "developer",
            "--agent",
            "opencode",
            "--brief",
            "no-existe.md",
        ],
        dir.path(),
        Some("t6h"),
    );
    assert_eq!(code, 2);
    assert!(out.is_empty());
    assert!(err.contains("No existe el archivo"), "{err}");
}

#[test]
fn la_ayuda_muestra_la_sintaxis_del_modo_local() {
    let dir = TempDir::new();
    let (code, out, _) = run_local(&["--help"], dir.path(), None);
    assert_eq!(code, 0);
    assert!(
        out.contains("ez delegate <papel> [--agent <tipo>]"),
        "{out}"
    );
    assert!(out.contains("--show tree|tab|right|down"), "{out}");
}

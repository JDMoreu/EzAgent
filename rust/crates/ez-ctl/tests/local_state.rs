//! `ez report`, `ez validate`, `ez tasks` y `ez watch`: el análisis de las
//! opciones y los códigos de salida del modo local.

mod support;

use std::path::Path;

use ez_herdr::subagents::{Record, Registry, TaskState};
use support::{program, run, Harness, TempDir};

/// Ejecuta `ez` en el modo local: sin árbitro, con una terminal y un registro.
fn run_local(
    argv: &[&str],
    cwd: &Path,
    terminal: Option<&str>,
    root: &Path,
) -> (i32, String, String) {
    run_local_env(argv, cwd, terminal, root, &|_| None)
}

/// Igual que `run_local`, pero con variables extra (los tiempos del vigilante).
fn run_local_env(
    argv: &[&str],
    cwd: &Path,
    terminal: Option<&str>,
    root: &Path,
    extra: &dyn Fn(&str) -> Option<String>,
) -> (i32, String, String) {
    let terminal = terminal.map(str::to_string);
    let root = root.to_string_lossy().into_owned();
    let env = move |name: &str| match name {
        "HC_TERMINAL_ID" => terminal.clone(),
        "EZ_SUBAGENTS_DIR" => Some(root.clone()),
        // DECISION: un `hc` que no existe: las pruebas no tocan Herdr.
        "EZ_HC_PATH" => Some("hc-que-no-existe".to_string()),
        other => extra(other),
    };
    run(&program(argv), &env, cwd)
}

/// Un registro de subagente en proceso.
fn record(name: &str, terminal: &str, parent: &str) -> Record {
    Record {
        name: name.to_string(),
        terminal: terminal.to_string(),
        parent: parent.to_string(),
        agent: "opencode".to_string(),
        label: "OpenCode".to_string(),
        model: Some("deepseek-v4.1-flash".to_string()),
        effort: None,
        role: "developer".to_string(),
        cwd: "C:\\ws".to_string(),
        brief_head: "arregla el parser".to_string(),
        state: TaskState::InProgress,
        since_ms: 0,
        history: Vec::new(),
        summary: None,
        report: None,
        validation: None,
        notified: None,
    }
}

#[test]
fn el_report_sin_registro_es_codigo_1() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let (code, out, err) = run_local(
        &["report", "--state", "terminado"],
        dir.path(),
        Some("t12"),
        &root,
    );
    assert_eq!(code, 1);
    assert!(out.is_empty());
    assert!(
        err.contains("Esta terminal no tiene una tarea de EzAgent."),
        "{err}"
    );
}

#[test]
fn el_report_sin_terminal_es_codigo_3() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let (code, out, err) = run_local(&["report", "--state", "terminado"], dir.path(), None, &root);
    assert_eq!(code, 3);
    assert!(out.is_empty());
    assert!(
        err.contains("Ejecútalo desde la terminal de un agente en Herdr."),
        "{err}"
    );
}

#[test]
fn el_report_con_un_archivo_que_no_existe_es_codigo_2() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let (code, out, err) = run_local(
        &["report", "--state", "terminado", "--file", "no-existe.md"],
        dir.path(),
        Some("t12"),
        &root,
    );
    assert_eq!(code, 2);
    assert!(out.is_empty());
    assert!(err.contains("No existe el archivo"), "{err}");
}

#[test]
fn el_report_con_un_estado_desconocido_es_codigo_2() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let (code, out, err) = run_local(
        &["report", "--state", "casi"],
        dir.path(),
        Some("t12"),
        &root,
    );
    assert_eq!(code, 2);
    assert!(out.is_empty());
    assert!(
        err.contains("--state debe ser terminado o fallido"),
        "{err}"
    );
}

#[test]
fn el_report_de_un_subagente_registra_el_estado() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12", "t6h"))
        .expect("registro");
    let informe = dir.file("informe.md");
    let informe = informe.to_string_lossy().into_owned();
    let (code, out, err) = run_local(
        &[
            "report",
            "--state",
            "terminado",
            "--summary",
            "listo",
            "--file",
            &informe,
        ],
        dir.path(),
        Some("t12"),
        &root,
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("Estado registrado: terminado (dev1-opencode)."),
        "{out}"
    );
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::Done);
    assert_eq!(saved.summary.as_deref(), Some("listo"));
    assert_eq!(saved.report.as_deref(), Some(informe.as_str()));
}

#[test]
fn el_validate_desde_un_desconocido_es_codigo_1() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12", "t6h"))
        .expect("registro");
    let (code, out, err) = run_local(
        &["validate", "dev1-opencode", "--pass"],
        dir.path(),
        Some("t99"),
        &root,
    );
    assert_eq!(code, 1);
    assert!(out.is_empty());
    assert!(
        err.contains("Solo el padre o un hermano puede validar"),
        "{err}"
    );
}

#[test]
fn el_validate_sin_veredicto_es_codigo_2() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let (code, out, err) = run_local(
        &["validate", "dev1-opencode"],
        dir.path(),
        Some("t6h"),
        &root,
    );
    assert_eq!(code, 2);
    assert!(out.is_empty());
    assert!(err.contains("Uso:"), "{err}");
}

#[test]
fn el_validate_una_tarea_que_no_termino_es_codigo_1() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12", "t6h"))
        .expect("registro");
    let (code, out, err) = run_local(
        &["validate", "dev1-opencode", "--pass"],
        dir.path(),
        Some("t6h"),
        &root,
    );
    assert_eq!(code, 1);
    assert!(out.is_empty());
    assert!(err.contains("solo se validan las terminadas"), "{err}");
}

#[test]
fn el_validate_del_padre_pasa_la_tarea() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12", "t6h"))
        .expect("registro");
    registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::Done,
            "t12",
            "el subagente terminó",
            |_| {},
        )
        .expect("cambio");
    let (code, out, err) = run_local(
        &[
            "validate",
            "dev1-opencode",
            "--pass",
            "--summary",
            "todo bien",
        ],
        dir.path(),
        Some("t6h"),
        &root,
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("Validación registrada: validado (dev1-opencode)."),
        "{out}"
    );
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::Validated);
    let validation = saved.validation.expect("validación");
    assert!(validation.verdict);
    assert_eq!(validation.by, "t6h");
    assert_eq!(validation.summary, "todo bien");
}

#[test]
fn el_validate_que_falla_deja_la_tarea_fallida() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12", "t6h"))
        .expect("registro");
    registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::Done,
            "t12",
            "el subagente terminó",
            |_| {},
        )
        .expect("cambio");
    let (code, _, err) = run_local(
        &[
            "validate",
            "dev1-opencode",
            "--fail",
            "--summary",
            "falta el informe",
        ],
        dir.path(),
        Some("t6h"),
        &root,
    );
    assert_eq!(code, 0, "{err}");
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::Failed);
    let detail = saved.history.last().expect("cambio").detail.clone();
    assert!(detail.contains("no pasó la validación de t6h"), "{detail}");
}

#[test]
fn el_tasks_sin_padre_es_codigo_3() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let (code, out, err) = run_local(&["tasks"], dir.path(), None, &root);
    assert_eq!(code, 3);
    assert!(out.is_empty());
    assert!(err.contains("--parent"), "{err}");
}

#[test]
fn el_tasks_muestra_los_subagentes_del_padre() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12", "t6h"))
        .expect("registro");
    let (code, out, err) = run_local(&["tasks"], dir.path(), Some("t6h"), &root);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("dev1-opencode"), "{out}");
    assert!(out.contains("OpenCode · deepseek-v4.1-flash"), "{out}");
    assert!(out.contains("en_proceso"), "{out}");
}

#[test]
fn el_tasks_con_json_muestra_los_registros() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12", "t6h"))
        .expect("registro");
    let (code, out, err) = run_local(&["tasks", "--json"], dir.path(), Some("t6h"), &root);
    assert_eq!(code, 0, "{err}");
    let value: serde_json::Value = serde_json::from_str(out.trim()).expect("json");
    let rows = value.as_array().expect("lista");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "dev1-opencode");
    assert_eq!(rows[0]["state"], "en_proceso");
}

#[test]
fn en_una_tarea_del_arbitro_el_report_local_es_un_uso_incorrecto() {
    let mut harness = Harness::new();
    let (code, out, err) = harness.run(&["report", "--state", "terminado"]);
    assert_eq!(code, 2);
    assert!(out.is_empty());
    assert!(
        err.contains("En una tarea del árbitro el estado de los subagentes"),
        "{err}"
    );
    harness.stop();
}

#[test]
fn el_watch_sin_padre_ni_carpeta_es_codigo_2() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let (code, out, err) = run_local(&["watch"], dir.path(), Some("t6h"), &root);
    assert_eq!(code, 2);
    assert!(out.is_empty());
    assert!(err.contains("Uso:"), "{err}");
}

#[test]
fn el_watch_analiza_sus_opciones() {
    let dir = TempDir::new();
    let root = dir.path().join("subagentes");
    let root_text = root.to_string_lossy().into_owned();
    // El vigilante termina de inmediato: no hay nada pendiente.
    let (code, out, err) = run_local_env(
        &[
            "watch",
            "--parent",
            "t6h",
            "--dir",
            &root_text,
            "--hc",
            "hc-que-no-existe",
        ],
        dir.path(),
        Some("t6h"),
        &root,
        &|name| match name {
            "EZ_WATCH_INTERVAL_MS" => Some("10".to_string()),
            "EZ_WATCH_IDLE_MS" => Some("50".to_string()),
            _ => None,
        },
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Vigilante terminado"), "{out}");
}

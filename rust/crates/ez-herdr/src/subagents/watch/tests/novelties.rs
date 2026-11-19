//! Pruebas de las novedades del vigilante: límites, diálogos y sin avance.

use chrono::TimeZone;

use super::*;

const NOW: u64 = 1_700_000_000_000;

/// Un registro del agente y modelo pedidos; el resto como [`record`].
fn record_of(name: &str, terminal: &str, agent: &str, label: &str, model: &str) -> Record {
    let mut saved = record(name, terminal);
    saved.agent = agent.to_string();
    saved.label = label.to_string();
    saved.model = Some(model.to_string());
    saved
}

#[test]
fn un_limite_de_opencode_lo_bloquea_y_anota_la_hora() {
    let root = temp_root("limite-opencode");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen(
            "t12",
            "Free limit reached. Resets at 7:00 p.m.\nUse /models to change.",
        );
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    let tick = watcher.tick();

    assert_eq!(tick.blocked, vec!["dev1-opencode".to_string()]);
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::Blocked);
    let change = saved.history.last().expect("cambio");
    assert_eq!(change.by, "vigilante");
    assert!(change.detail.contains("plan gratuito"), "{}", change.detail);

    let sent = fake.sent();
    assert!(
        sent.iter().any(|text| text.contains("está bloqueado")),
        "{sent:?}"
    );
    assert!(
        sent.iter()
            .any(|text| text.contains("ez retry dev1-opencode --auto")),
        "{sent:?}"
    );

    let usage = usage(&root);
    assert_eq!(usage[0]["agente"], "opencode");
    assert_eq!(usage[0]["modelo"], "deepseek-v4.1-flash");
    assert_eq!(usage[0]["visto_ms"], NOW);
    let until = usage[0]["hasta"].as_u64().expect("hora de reinicio");
    let local = chrono::Local
        .timestamp_millis_opt(i64::try_from(until).expect("milisegundos"))
        .single()
        .expect("fecha local");
    assert_eq!(local.format("%H:%M").to_string(), "19:00");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_limite_de_codex_sin_hora_queda_sin_hasta() {
    let root = temp_root("limite-codex");
    let registry = Registry::at(&root);
    registry
        .create(&record_of(
            "dev1-codex",
            "t12",
            "codex",
            "Codex",
            "gpt-5.1-codex",
        ))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", "You've hit your usage limit.");
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    let tick = watcher.tick();

    assert_eq!(tick.blocked, vec!["dev1-codex".to_string()]);
    let usage = usage(&root);
    assert_eq!(usage[0]["agente"], "codex");
    assert!(usage[0]["hasta"].is_null());
    assert!(fake
        .sent()
        .iter()
        .any(|text| text.contains("límite de uso")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_limite_de_claude_con_horas_de_espera_trae_la_hora() {
    let root = temp_root("limite-claude");
    let registry = Registry::at(&root);
    registry
        .create(&record_of(
            "dev1-claude",
            "t12",
            "claude",
            "Claude Code",
            "claude-sonnet-4-5",
        ))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", "5 hour usage limit reached. Resets in 2h.");
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    let tick = watcher.tick();

    assert_eq!(tick.blocked, vec!["dev1-claude".to_string()]);
    let usage = usage(&root);
    assert_eq!(usage[0]["hasta"], NOW + 2 * 3_600_000);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_dialogo_de_hc_lo_bloquea_sin_tocar_uso() {
    let root = temp_root("dialogo");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("blocked"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", "Do you trust the files in this folder?");
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    let tick = watcher.tick();

    assert_eq!(tick.blocked, vec!["dev1-opencode".to_string()]);
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::Blocked);
    assert!(saved
        .history
        .last()
        .expect("cambio")
        .detail
        .contains("diálogo"));
    assert!(!root.join("uso.json").exists());
    assert!(fake.sent()[0].contains("diálogo o permiso pendiente"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_dialogo_conocido_en_la_pantalla_tambien_bloquea() {
    let root = temp_root("dialogo-pantalla");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen(
            "t12",
            " Accessing workspace:\n\n Quick safety check: Is this a project you trust?\n\n ❯ No, exit\n   Yes, I trust this folder\n",
        );
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    let tick = watcher.tick();

    assert_eq!(tick.blocked, vec!["dev1-opencode".to_string()]);
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::Blocked);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sin_avance_avisa_a_los_diez_minutos_una_sola_vez() {
    let root = temp_root("sin-avance");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", "❯ pensando…");
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();

    fake.advance(9 * 60_000);
    let tick = watcher.tick();
    assert!(tick.stalled.is_empty());

    fake.advance(60_000);
    let tick = watcher.tick();
    assert_eq!(tick.stalled, vec!["dev1-opencode".to_string()]);
    let sent = fake.sent();
    assert!(sent
        .iter()
        .any(|text| text.contains("lleva 10 min sin avance")));
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(
        saved.state,
        TaskState::InProgress,
        "sin avance no cambia estado"
    );
    let before = fake.sent().len();

    watcher.tick();
    assert_eq!(fake.sent().len(), before, "una vez por episodio");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn al_volver_a_trabajar_sigue_en_proceso_y_lo_avisa() {
    let root = temp_root("vuelve");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", "Free limit reached. Resets at 7:00 p.m.");
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    watcher.tick();
    assert_eq!(
        registry
            .get("t6h", "dev1-opencode")
            .expect("leer")
            .expect("registro")
            .state,
        TaskState::Blocked
    );

    fake.advance(5_000);
    fake.with_screen("t12", "❯ trabajando otra vez")
        .with_ls(&live_children("working"));
    let tick = watcher.tick();
    assert_eq!(tick.resumed, vec!["dev1-opencode".to_string()]);
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::InProgress);
    assert_eq!(
        saved.history.last().expect("cambio").detail,
        "siguió trabajando"
    );
    assert!(fake
        .sent()
        .iter()
        .any(|text| text.contains("siguió trabajando")));

    let before = fake.sent().len();
    watcher.tick();
    assert_eq!(fake.sent().len(), before, "el aviso no se repite");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn cada_episodio_de_bloqueo_se_avisa_una_vez() {
    let root = temp_root("episodios");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", "Free limit reached. Resets at 7:00 p.m.");
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    watcher.tick();
    watcher.tick();
    let blocked = fake
        .sent()
        .iter()
        .filter(|text| text.contains("está bloqueado"))
        .count();
    assert_eq!(blocked, 1, "el bloqueo se avisa una vez");
    let before = fake.sent().len();
    watcher.tick();
    assert_eq!(fake.sent().len(), before, "el aviso no se repite");

    fake.with_screen("t12", "❯ otra vez")
        .with_ls(&live_children("working"));
    watcher.tick();
    fake.with_screen("t12", "Free limit reached. Resets at 8:00 p.m.")
        .with_ls(&live_children("idle"));
    // §3: la pantalla tiene que quedarse quieta una vuelta antes de contar.
    watcher.tick();
    watcher.tick();
    let blocked = fake
        .sent()
        .iter()
        .filter(|text| text.contains("está bloqueado"))
        .count();
    assert_eq!(blocked, 2, "un aviso por episodio");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_limite_mientras_trabaja_no_bloquea_ni_anota() {
    let root = temp_root("working-limite");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let screen = "Escribo el detector del encargo IR-2:\n\
                  buscar «Free limit reached», «quota exceeded» y «rate limit reached»\n\
                  en las últimas líneas de la pantalla.\n\
                  if screen.contains(\"quota\") { anotar }\n\
                  Free limit reached";
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("working"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", screen);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    watcher.tick();

    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(
        saved.state,
        TaskState::InProgress,
        "un agente que trabaja no está bloqueado"
    );
    assert!(!root.join("uso.json").exists(), "no se anota uso.json");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn las_palabras_sueltas_no_bloquean() {
    let root = temp_root("palabras-sueltas");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", "quota\nrate limit\nusage limit\nresets at 7:00 p.m.");
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    watcher.tick();

    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::InProgress);
    assert!(!root.join("uso.json").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn el_limite_fuera_de_las_ultimas_lineas_no_cuenta() {
    let root = temp_root("limite-arriba");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let screen = format!(
        "Free limit reached. Resets at 7:00 p.m.\n{}",
        "procesando archivo\n".repeat(20)
    );
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", &screen);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();
    watcher.tick();

    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::InProgress);
    assert!(!root.join("uso.json").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn el_limite_al_final_con_pantalla_estable_si_bloquea() {
    let root = temp_root("limite-abajo");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", "❯ trabajando");
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();

    fake.advance(3_000);
    fake.with_screen(
        "t12",
        "❯ trabajando\nFree limit reached. Resets at 7:00 p.m.",
    );
    let tick = watcher.tick();
    assert!(
        tick.blocked.is_empty(),
        "la pantalla cambió: hace falta una vuelta más"
    );
    assert_eq!(
        registry
            .get("t6h", "dev1-opencode")
            .expect("leer")
            .expect("registro")
            .state,
        TaskState::InProgress
    );

    fake.advance(3_000);
    let tick = watcher.tick();
    assert_eq!(tick.blocked, vec!["dev1-opencode".to_string()]);
    let usage = usage(&root);
    let until = usage[0]["hasta"].as_u64().expect("hora de reinicio");
    let local = chrono::Local
        .timestamp_millis_opt(i64::try_from(until).expect("milisegundos"))
        .single()
        .expect("fecha local");
    assert_eq!(local.format("%H:%M").to_string(), "19:00");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn el_vigilante_lee_la_pantalla_sin_historial() {
    let root = temp_root("lee");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(&live_children("idle"))
        .with_get(r#"{"address":"t6h","state":"live"}"#)
        .with_screen("t12", "sin novedad");
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    watcher.tick();

    let calls = fake.calls();
    let read = calls
        .iter()
        .find(|call| call.first().map(String::as_str) == Some("read"))
        .expect("hc read");
    assert_eq!(read, &["read", "t12", "--lines", "0"]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn un_bloqueado_que_muere_sin_informe_queda_fallido() {
    let root = temp_root("bloqueado-muere");
    let registry = Registry::at(&root);
    registry
        .create(&record("dev1-opencode", "t12"))
        .expect("registro");
    registry
        .transition(
            "t6h",
            "dev1-opencode",
            TaskState::Blocked,
            "vigilante",
            "diálogo o permiso pendiente",
            |_| {},
        )
        .expect("bloquear");
    let fake = Fake::new()
        .at(NOW)
        .with_ls(r#"{"terminals":[{"address":"t12","state":"finished","exit_code":1}]}"#)
        .with_get(r#"{"address":"t6h","state":"live"}"#);
    let mut watcher = make_watcher(&fake, Registry::at(&root));
    let tick = watcher.tick();

    assert_eq!(tick.failed, vec!["dev1-opencode".to_string()]);
    let saved = registry
        .get("t6h", "dev1-opencode")
        .expect("leer")
        .expect("registro");
    assert_eq!(saved.state, TaskState::Failed);
    let _ = std::fs::remove_dir_all(&root);
}

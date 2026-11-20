//! Pruebas de `ez retry`: análisis, elección, encargo guardado y política.

use std::path::PathBuf;

use ez_core::config::VoiceConfig;
use ez_herdr::subagents::{Change, Record, TaskState};
use ez_scan::recommend::{Catalog, Cost, Level, ModelEntry, Pressure};
use ez_scan::usage::{AgentUsage, ModelUsage, Sources, UsageReport, UsageState};

use crate::cli::Parsed;

use super::super::choice::Inputs;
use super::*;

fn parsed(values: &[&str]) -> Parsed {
    Parsed::parse(
        &values
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    )
    .expect("análisis")
}

fn command(values: &[&str]) -> LocalRetry {
    match build_retry(&mut parsed(values)) {
        Ok(command) => command,
        Err(_) => panic!("el análisis debía aceptar {values:?}"),
    }
}

fn record() -> Record {
    let now = ez_herdr::subagents::epoch_ms();
    Record {
        name: "dev1-opencode".to_string(),
        terminal: "t12".to_string(),
        parent: "t6h".to_string(),
        agent: "opencode".to_string(),
        label: "OpenCode".to_string(),
        model: Some("opencode/glm-5.3".to_string()),
        effort: Some("medium".to_string()),
        role: "developer".to_string(),
        cwd: "C:\\ws".to_string(),
        brief_head: "arregla el parser".to_string(),
        brief_path: None,
        retry_of: None,
        state: TaskState::Blocked,
        since_ms: now,
        history: vec![Change {
            state: TaskState::Blocked,
            at_ms: now,
            by: "vigilante".to_string(),
            detail: "límite de uso del plan gratuito".to_string(),
        }],
        summary: None,
        report: None,
        validation: None,
        notified: None,
    }
}

fn temp_folder(tag: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "ez-retry-ctl-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(&folder).expect("carpeta temporal");
    folder
}

fn agent(key: &str, label: &str) -> AgentUsage {
    AgentUsage {
        key: key.to_string(),
        label: label.to_string(),
        installed: true,
        window_hours: 5.0,
        used_percent: None,
        observed_window_hours: None,
        tokens: None,
        messages: None,
        cost_usd: None,
        models: Vec::new(),
        limit: None,
        resets_ms: None,
        reset_text: None,
        plan: None,
        priority: None,
        state: UsageState::Libre,
        observed_ms: None,
        sources: Sources::default(),
        warnings: Vec::new(),
    }
}

fn entry(agent: &str, model: &str, cost: Cost) -> ModelEntry {
    ModelEntry {
        agent: agent.to_string(),
        model: model.to_string(),
        level: Level::Medio,
        cost,
        family: "prueba".to_string(),
    }
}

/// Un informe con el modelo gratuito agotado y el de plan libre.
fn exhausted_inputs() -> Inputs {
    let mut opencode = agent("opencode", "OpenCode");
    opencode.models = vec![
        ModelUsage {
            model: "opencode/glm-5.3".to_string(),
            tokens: None,
            messages: None,
            cost_usd: None,
            state: UsageState::Agotado,
        },
        ModelUsage {
            model: "opencode-go/deepseek-v4.1-flash".to_string(),
            tokens: None,
            messages: None,
            cost_usd: None,
            state: UsageState::Libre,
        },
    ];
    Inputs {
        cfg: VoiceConfig::new(),
        report: UsageReport {
            generated_ms: 0,
            agents: vec![opencode],
            warnings: Vec::new(),
        },
        catalog: Catalog {
            entries: vec![
                entry("opencode", "glm-5.3", Cost::Gratis),
                entry("opencode", "deepseek-v4.1-flash", Cost::Plan),
            ],
            warnings: Vec::new(),
        },
        pressure: Pressure::default(),
        last_developer_model: None,
    }
}

#[test]
fn el_analisis_toma_las_opciones_y_rechaza_las_mezclas() {
    let ok = command(&["dev1-opencode", "--auto", "--keep"]);
    assert_eq!(ok.name, "dev1-opencode");
    assert!(ok.auto);
    assert!(ok.keep);
    assert!(!ok.same);

    let same = command(&["dev1-opencode", "--same"]);
    assert!(same.same);

    for bad in [
        &["dev1-opencode", "--auto", "--same"][..],
        &["dev1-opencode", "--auto", "--agent", "claude"][..],
        &["dev1-opencode", "--same", "--model", "sonnet"][..],
        &["MALO"][..],
    ] {
        assert!(build_retry(&mut parsed(bad)).is_err(), "{bad:?}");
    }
}

#[test]
fn sin_opciones_conserva_tipo_modelo_y_esfuerzo() {
    let prepared = prepare_with_cfg(
        &command(&["dev1-opencode"]),
        &record(),
        "t6h",
        &|_| None,
        VoiceConfig::new(),
    )
    .expect("preparado");
    assert_eq!(prepared.fill.agent, "opencode");
    assert_eq!(prepared.fill.model.as_deref(), Some("opencode/glm-5.3"));
    assert_eq!(prepared.fill.effort.as_deref(), Some("medium"));
    assert!(prepared.fill.chosen.is_none());
}

#[test]
fn same_conserva_aunque_el_modelo_este_agotado() {
    let prepared = prepare_with_cfg(
        &command(&["dev1-opencode", "--same"]),
        &record(),
        "t6h",
        &|_| None,
        VoiceConfig::new(),
    )
    .expect("preparado");
    assert_eq!(prepared.fill.model.as_deref(), Some("opencode/glm-5.3"));
}

#[test]
fn auto_evita_el_modelo_agotado() {
    let inputs = exhausted_inputs();
    let selected =
        select(&command(&["dev1-opencode", "--auto"]), &record(), &inputs).expect("elección");
    assert_eq!(selected.agent, "opencode");
    assert_eq!(
        selected.model.as_deref(),
        Some("opencode-go/deepseek-v4.1-flash")
    );
    let chosen = selected.chosen.expect("línea");
    assert!(chosen.starts_with("Elegí OpenCode"), "{chosen}");
}

#[test]
fn manual_con_agente_y_modelo_no_cambia_la_eleccion() {
    let selected = select(
        &command(&["dev1-opencode", "--agent", "claude", "--model", "sonnet"]),
        &record(),
        &exhausted_inputs(),
    )
    .expect("elección");
    assert_eq!(selected.agent, "claude");
    assert_eq!(selected.model.as_deref(), Some("sonnet"));
}

#[test]
fn el_encargo_sale_del_archivo_y_si_falta_usa_las_primeras_letras() {
    let folder = temp_folder("encargo");
    let path = folder.join("encargo.md");
    std::fs::write(&path, "el encargo completo").expect("encargo");
    let mut saved = record();
    saved.brief_path = Some(path.to_string_lossy().into_owned());
    let (brief, used, warnings) = brief_for(&saved).expect("encargo");
    assert_eq!(brief, "el encargo completo");
    assert_eq!(used.as_deref(), Some(path.to_string_lossy().as_ref()));
    assert!(warnings.is_empty());
    let _ = std::fs::remove_dir_all(&folder);

    let (brief, used, warnings) = brief_for(&record()).expect("respaldo");
    assert_eq!(brief, "arregla el parser");
    assert!(used.is_none());
    assert_eq!(warnings.len(), 1);
}

#[test]
fn la_politica_automatica_se_lee_de_planes_toml() {
    let folder = temp_folder("planes");
    assert!(!automatic_retry_enabled(&folder));
    std::fs::write(
        folder.join("planes.toml"),
        "[novedades]\nreintento_automatico = true\n",
    )
    .expect("planes");
    assert!(automatic_retry_enabled(&folder));
    std::fs::write(
        folder.join("planes.toml"),
        "[novedades]\nreintento_automatico = false\n",
    )
    .expect("planes");
    assert!(!automatic_retry_enabled(&folder));
    let _ = std::fs::remove_dir_all(&folder);
}

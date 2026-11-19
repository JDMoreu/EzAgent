//! Pruebas de `planes.toml`: lectura tolerante y propuesta que no sobrescribe.

use super::*;
use crate::usage::{AgentUsage, Sources, UsageState};

fn temp_root(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("ez-scan-planes-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("carpeta");
    root
}

#[test]
fn lee_todo_lo_que_la_persona_declara() {
    let planes = parse(
        "[claude]\n\
         plan = \"claude_pro\"\n\
         ventana_horas = 10\n\
         limite = 1000\n\
         unidad = \"mensajes\"\n\
         prioridad = \"primero\"\n\
         \n[novedades]\n\
         reintento_automatico = true\n",
    );
    let plane = planes.agents.get("claude").expect("claude");
    assert_eq!(plane.plan.as_deref(), Some("claude_pro"));
    assert_eq!(plane.window_hours, Some(10.0));
    assert_eq!(plane.limit, Some(UsageLimit::messages(1000.0)));
    assert_eq!(plane.priority, Some(Priority::Primero));
    assert!(planes.novedades.automatic_retry);
    assert!(planes.warnings.is_empty(), "{:?}", planes.warnings);
}

#[test]
fn lo_que_no_se_entiende_se_avisa_y_se_ignora() {
    let planes = parse("[codex]\nprioridad = \"siempre\"\nlimite = -5\nventana_horas = 0\n");
    let plane = planes.agents.get("codex").expect("codex");
    assert!(plane.priority.is_none());
    assert!(plane.limit.is_none());
    assert!(plane.window_hours.is_none());
    assert!(planes.warnings.len() >= 3, "{:?}", planes.warnings);
}

#[test]
fn sin_archivo_no_hay_nada_que_declarar() {
    let planes = load(std::path::Path::new("no-existe-planes-inventado.toml"));
    assert!(planes.agents.is_empty());
    assert!(planes.warnings.is_empty());
}

#[test]
fn un_toml_roto_no_rompe_el_comando() {
    let planes = parse("[codex\nplan = ");
    assert!(planes.agents.is_empty());
    assert!(planes.warnings[0].contains("planes.toml"));
}

#[test]
fn el_alias_minimax_sirve_para_mcode() {
    let planes = parse("[minimax]\nplan = \"oauth\"\n");
    let plane = for_agent(&planes, "mcode").expect("mcode por alias");
    assert_eq!(plane.plan.as_deref(), Some("oauth"));
}

#[test]
fn la_propuesta_no_sobrescribe_lo_que_ya_existe() {
    let root = temp_root("propuesta");
    let path = root.join("planes.toml");
    let report = UsageReport {
        generated_ms: 0,
        warnings: Vec::new(),
        agents: vec![AgentUsage {
            key: "codex".into(),
            label: "Codex".into(),
            installed: true,
            window_hours: 5.0,
            used_percent: None,
            observed_window_hours: Some(5.0),
            tokens: None,
            messages: None,
            cost_usd: None,
            limit: None,
            resets_ms: None,
            reset_text: None,
            plan: Some("plus".into()),
            priority: None,
            state: UsageState::Desconocido,
            observed_ms: None,
            sources: Sources::default(),
            warnings: Vec::new(),
        }],
    };
    assert!(propose(&path, &report).expect("proponer"));
    let text = std::fs::read_to_string(&path).expect("archivo");
    assert!(text.contains("[codex]"));
    assert!(text.contains("plan = \"plus\""));
    assert!(text.contains("[novedades]"));
    std::fs::write(&path, "planes escritos por la persona\n").expect("marcar");
    assert!(!propose(&path, &report).expect("no sobrescribe"));
    assert_eq!(
        std::fs::read_to_string(&path).expect("archivo"),
        "planes escritos por la persona\n"
    );
    let _ = std::fs::remove_dir_all(&root);
}

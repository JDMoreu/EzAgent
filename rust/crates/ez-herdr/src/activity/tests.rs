use std::collections::HashMap;

use super::*;

fn item(key: &str, state: &str, since: f64) -> ActivityItem {
    ActivityItem {
        key: key.to_string(),
        title: format!("Tarea {key}"),
        agent: "codex".to_string(),
        state: state.to_string(),
        since,
        unseen: false,
        live: false,
    }
}

fn live(mut item: ActivityItem) -> ActivityItem {
    item.live = true;
    item
}

fn unseen(mut item: ActivityItem) -> ActivityItem {
    item.unseen = true;
    item
}

#[test]
fn el_orden_pone_primero_lo_que_te_necesita_y_al_final_lo_terminado() {
    let items = vec![
        unseen(item("a", DONE, 50.0)),
        live(item("b", WORKING, 10.0)),
        unseen(item("c", NEEDS_YOU, 5.0)),
        live(item("d", WAITING, 1.0)),
        unseen(item("e", FAILED, 2.0)),
    ];
    let keys: Vec<String> = order(items).into_iter().map(|item| item.key).collect();
    assert_eq!(keys, ["c", "e", "b", "d", "a"]);
}

#[test]
fn lo_vivo_gana_a_un_desenlace_con_la_misma_clave() {
    let merged = merge([live(item("t1", WORKING, 0.0))], [item("t1", DONE, 0.0)]);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].key, "t1");
    assert_eq!(merged[0].state, WORKING);
}

#[test]
fn la_orbita_solo_marca_lo_vivo_y_lo_no_visto_con_tope() {
    let mut items: Vec<ActivityItem> = (0..7)
        .map(|index| live(item(&index.to_string(), WORKING, 0.0)))
        .collect();
    items.push(item("seen", DONE, 0.0));
    assert_eq!(orbit(items, ORBIT_LIMIT), vec![WORKING; 5]);
    assert!(orbit([item("x", DONE, 0.0)], ORBIT_LIMIT).is_empty());
    assert_eq!(
        orbit([unseen(item("x", DONE, 0.0))], ORBIT_LIMIT),
        vec![DONE.to_string()]
    );
}

#[test]
fn el_registro_traduce_motivos_y_caduca_lo_visto() {
    let now = std::sync::Arc::new(Mutex::new(1000.0));
    let clock = {
        let now = std::sync::Arc::clone(&now);
        move || *now.lock().expect("reloj")
    };
    let log = ActivityLog::new(clock);
    assert_eq!(
        log.record("t1", "codex", "  Revisar\n permisos ", "lost")
            .state,
        STALE
    );
    assert_eq!(log.record("t2", "claude", "", "error").title, "Tarea");
    let states: Vec<String> = log.items().into_iter().map(|item| item.state).collect();
    assert!(states.contains(&STALE.to_string()) && states.contains(&FAILED.to_string()));
    *now.lock().expect("reloj") += KEEP_SECS + 1.0;
    assert_eq!(log.items().len(), 2); // sin ver: no caduca
    assert!(log.seen_all() && !log.seen_all());
    assert!(log.items().is_empty()); // visto y viejo: se va
}

#[test]
fn el_registro_tiene_tope_y_olvida_por_clave() {
    let log = ActivityLog::default();
    for index in 0..LOG_LIMIT + 3 {
        log.record(&format!("t{index}"), "", "x", "done");
    }
    assert_eq!(log.items().len(), LOG_LIMIT);
    log.forget("t5");
    assert!(!log.items().iter().any(|item| item.key == "t5"));
}

#[derive(Default)]
struct FakeSource {
    watched: Vec<String>,
    tasks: HashMap<String, Map<String, Value>>,
    states: HashMap<String, String>,
    labels: HashMap<String, String>,
    chains: Vec<Map<String, Value>>,
    swarms: Vec<(String, String)>,
}

impl ActivitySource for FakeSource {
    fn watched(&self) -> Vec<String> {
        self.watched.clone()
    }

    fn task(&self, address: &str) -> Option<Map<String, Value>> {
        self.tasks.get(address).cloned()
    }

    fn state_of(&self, address: &str) -> Option<String> {
        self.states.get(address).cloned()
    }

    fn label_for(&self, address: &str) -> Option<String> {
        self.labels.get(address).cloned()
    }

    fn chains(&self) -> Vec<Map<String, Value>> {
        self.chains.clone()
    }

    fn active_swarms(&self) -> Vec<(String, String)> {
        self.swarms.clone()
    }
}

#[test]
fn las_cadenas_quedan_en_espera() {
    let mut source = FakeSource::default();
    let mut spec = Map::new();
    spec.insert("source".into(), Value::from("t1"));
    spec.insert("agent".into(), Value::from("opencode"));
    spec.insert("name".into(), Value::from("QA"));
    spec.insert("created_at".into(), Value::from(3.0));
    source.chains.push(spec);
    let items = live_items(&source, Some(10.0));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "QA");
    assert_eq!(items[0].state, WAITING);
    assert_eq!(items[0].since, 3.0);
    assert_eq!(items[0].key, "chain:t1:opencode:3.0");
}

#[test]
fn los_titulos_largos_se_recortan() {
    let title = clean_title(&"x".repeat(500), "Tarea");
    assert_eq!(title.chars().count(), TITLE_LIMIT);
    assert!(title.ends_with('…'));
}

#[test]
fn un_enjambre_en_marcha_no_se_puede_quitar() {
    assert!(dismissible(&item("swarm:ask", NEEDS_YOU, 0.0)));
    assert!(!dismissible(&live(item("swarm:s1", WORKING, 0.0))));
    assert!(dismissible(&live(item("t1", WORKING, 0.0))));
}

#[test]
fn lo_vivo_usa_el_estado_del_watcher_y_la_etiqueta_visible() {
    let mut source = FakeSource {
        watched: vec!["t1".into(), "t2".into(), "t3".into()],
        ..FakeSource::default()
    };
    source.states.insert("t1".into(), "blocked".into());
    source.states.insert("t2".into(), "idle".into());
    source.labels.insert("t1".into(), "Arreglar el HUD".into());
    let mut info = Map::new();
    info.insert("agent".into(), Value::from("claude"));
    info.insert("created_at".into(), Value::from(5.0));
    source.tasks.insert("t2".into(), info);
    let items = live_items(&source, Some(10.0));
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].key, "t1");
    assert_eq!(items[0].title, "Arreglar el HUD");
    assert_eq!(items[0].agent, "");
    assert_eq!(items[0].state, NEEDS_YOU);
    assert_eq!(items[0].since, 10.0);
    assert_eq!(items[1].key, "t3");
    assert_eq!(items[1].title, "el agente");
    assert_eq!(items[1].state, STALE);
}

struct OnlyWatched(Vec<String>);

impl ActivitySource for OnlyWatched {
    fn watched(&self) -> Vec<String> {
        self.0.clone()
    }
}

#[test]
fn sin_watcher_el_estado_es_en_curso() {
    let source = OnlyWatched(vec!["t1".into()]);
    let items = live_items(&source, Some(10.0));
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].state, WORKING);
    assert_eq!(items[0].since, 10.0);
}

#[test]
fn visible_name_nombra_pestanas_espacios_y_agentes() {
    let mut row = Map::new();
    row.insert("where".into(), Value::from("EzAgent / 2"));
    assert_eq!(visible_name(&row), "pestaña 2 de EzAgent");
    row.insert("where".into(), Value::from("Claude Code"));
    assert_eq!(visible_name(&row), "Claude Code");
    row.clear();
    row.insert("agent".into(), Value::from("codex"));
    row.insert("cwd".into(), Value::from("C:/proyectos/demo"));
    assert_eq!(visible_name(&row), "Codex en demo");
    row.clear();
    assert_eq!(visible_name(&row), "el agente");
}

#[test]
fn los_enjambres_activos_quedan_en_curso() {
    let mut source = FakeSource::default();
    source.swarms.push(("s1".into(), "Migrar a Rust".into()));
    source.swarms.push(("s2".into(), String::new()));
    let items = live_items(&source, Some(7.0));
    assert_eq!(items[0].key, "swarm:s1");
    assert_eq!(items[0].title, "Migrar a Rust");
    assert_eq!(items[0].agent, "enjambre");
    assert_eq!(items[0].state, WORKING);
    assert_eq!(items[0].since, 7.0);
    assert_eq!(items[1].title, "Enjambre");
}

#[test]
fn snapshot_junta_lo_vivo_y_lo_reciente() {
    let now = std::sync::Arc::new(Mutex::new(100.0));
    let clock = {
        let now = std::sync::Arc::clone(&now);
        move || *now.lock().expect("reloj")
    };
    let log = ActivityLog::new(clock);
    log.record("t9", "codex", "Listo", "done");
    let source = FakeSource {
        watched: vec!["t1".into()],
        ..FakeSource::default()
    };
    let items = snapshot(&source, Some(&log));
    let keys: Vec<String> = items.into_iter().map(|item| item.key).collect();
    assert_eq!(keys, ["t1", "t9"]);
}

#[test]
fn estados_desconocidos_no_tienen_etiqueta() {
    assert_eq!(state_label("raro"), "");
    assert_eq!(state_label(NEEDS_YOU), "Necesita tu respuesta");
    assert_eq!(item("x", "raro", 0.0).label(), "");
}

//! Port of `src/ezagent/voice/activity.py` (F3-15).
//!
//! Actividad de los agentes delegados: un solo modelo para la órbita y la vista
//! rápida. Se juntan dos fuentes en una instantánea ordenada:
//!
//! * **Lo vivo**, leído de lo que ya calcularon otros hilos (terminales
//!   vigiladas y su último estado del watcher, cadenas esperando, enjambres
//!   activos). Construir la instantánea nunca llama a Herdr ni al disco.
//! * **Los desenlaces recientes** que guarda [`ActivityLog`] (terminado,
//!   necesita respuesta, falló, perdido). Siguen a la vista un rato y quedan
//!   *sin ver* hasta que el usuario los atiende.
//!
//! Los estados son honestos: nada se marca terminado solo porque la salida se
//! detuvo, y no hay porcentajes. Una terminal que desapareció es «Sin
//! actualización reciente».

use std::path::Path;
use std::sync::Mutex;

use ez_core::pytime::timestamp;
use ez_core::pyvalue::{
    collapse_spaces, head_chars, py_rstrip, py_strip, python_str, text_or_empty, truthy,
};
use serde_json::{Map, Value};

use crate::tasks::number_or;

pub const NEEDS_YOU: &str = "needs_you";
pub const FAILED: &str = "failed";
pub const WORKING: &str = "working";
pub const WAITING: &str = "waiting";
pub const STALE: &str = "stale";
pub const DONE: &str = "done";

/// Prioridad: lo que pide atención va primero; lo terminado, al final.
pub const STATE_ORDER: &[(&str, usize)] = &[
    (NEEDS_YOU, 0),
    (FAILED, 1),
    (WORKING, 2),
    (WAITING, 3),
    (STALE, 4),
    (DONE, 5),
];
pub const STATE_LABEL: &[(&str, &str)] = &[
    (NEEDS_YOU, "Necesita tu respuesta"),
    (FAILED, "Falló"),
    (WORKING, "En curso"),
    (WAITING, "En espera"),
    (STALE, "Sin actualización reciente"),
    (DONE, "Completada"),
];

pub const TITLE_LIMIT: usize = 120;
/// Cuánto sigue a la vista un desenlace ya visto.
pub const KEEP_SECS: f64 = 30.0 * 60.0;
pub const LOG_LIMIT: usize = 8;
pub const ORBIT_LIMIT: usize = 5;

pub fn state_label(state: &str) -> &'static str {
    STATE_LABEL
        .iter()
        .find(|(key, _)| *key == state)
        .map(|(_, label)| *label)
        .unwrap_or("")
}

fn state_rank(state: &str) -> usize {
    STATE_ORDER
        .iter()
        .find(|(key, _)| *key == state)
        .map(|(_, rank)| *rank)
        .unwrap_or(9)
}

fn reason_state(reason: &str) -> &'static str {
    match reason {
        "done" => DONE,
        "idle" => DONE,
        "blocked" => NEEDS_YOU,
        "error" => FAILED,
        "lost" => STALE,
        _ => NEEDS_YOU,
    }
}

fn live_state(raw: &str) -> &'static str {
    match raw {
        "working" => WORKING,
        "blocked" => NEEDS_YOU,
        "error" => FAILED,
        _ => STALE,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityItem {
    pub key: String,
    pub title: String,
    pub agent: String,
    pub state: String,
    pub since: f64,
    pub unseen: bool,
    pub live: bool,
}

impl ActivityItem {
    pub fn label(&self) -> &'static str {
        state_label(&self.state)
    }

    /// ¿Se dibuja en la órbita? Lo vivo, y lo terminado que aún no se ha visto.
    pub fn marked(&self) -> bool {
        self.live || self.unseen
    }
}

/// Clave de una etapa de cadena en espera (la app la usa para encontrarla).
pub fn chain_key(spec: &Map<String, Value>) -> String {
    format!(
        "chain:{}:{}:{}",
        python_str(spec.get("source").unwrap_or(&Value::Null)),
        python_str(spec.get("agent").unwrap_or(&Value::Null)),
        python_str(spec.get("created_at").unwrap_or(&Value::Null))
    )
}

/// ¿Se puede quitar de la vista? Todo menos un enjambre en marcha.
pub fn dismissible(item: &ActivityItem) -> bool {
    !(item.live && item.key.starts_with("swarm:"))
}

pub fn clean_title(text: &str, fallback: &str) -> String {
    let title = collapse_spaces(text);
    let title = if title.chars().count() > TITLE_LIMIT {
        format!("{}…", py_rstrip(head_chars(&title, TITLE_LIMIT - 1)))
    } else {
        title
    };
    if title.is_empty() {
        fallback.to_string()
    } else {
        title
    }
}

/// Desenlaces recientes; seguro entre hilos (la UI lee, los avisos escriben).
pub struct ActivityLog {
    clock: Box<dyn Fn() -> f64 + Send + Sync>,
    items: Mutex<Vec<(String, ActivityItem)>>,
}

impl Default for ActivityLog {
    fn default() -> Self {
        Self::new(timestamp)
    }
}

impl ActivityLog {
    pub fn new(clock: impl Fn() -> f64 + Send + Sync + 'static) -> Self {
        Self {
            clock: Box::new(clock),
            items: Mutex::new(Vec::new()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(String, ActivityItem)>> {
        self.items
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn record(&self, key: &str, agent: &str, title: &str, reason: &str) -> ActivityItem {
        let item = ActivityItem {
            key: key.to_string(),
            title: clean_title(title, "Tarea"),
            agent: agent.to_string(),
            state: reason_state(reason).to_string(),
            since: (self.clock)(),
            unseen: true,
            live: false,
        };
        let mut items = self.lock();
        if let Some(index) = items.iter().position(|(stored, _)| stored == &item.key) {
            items.remove(index);
        }
        items.push((item.key.clone(), item.clone()));
        while items.len() > LOG_LIMIT {
            items.remove(0);
        }
        item
    }

    /// Marca todo como visto; `true` si algo cambió.
    pub fn seen_all(&self) -> bool {
        let mut items = self.lock();
        let changed = items.iter().any(|(_, item)| item.unseen);
        for (_, item) in items.iter_mut() {
            item.unseen = false;
        }
        changed
    }

    pub fn forget(&self, key: &str) {
        self.lock().retain(|(stored, _)| stored != key);
    }

    pub fn items(&self) -> Vec<ActivityItem> {
        let now = (self.clock)();
        let mut items = self.lock();
        items.retain(|(_, item)| {
            item.unseen
                || !matches!(
                    (now - item.since).partial_cmp(&KEEP_SECS),
                    Some(std::cmp::Ordering::Greater)
                )
        });
        items.iter().map(|(_, item)| item.clone()).collect()
    }
}

/// Prioridad: estado, luego lo no visto, luego lo más reciente.
pub fn order(items: impl IntoIterator<Item = ActivityItem>) -> Vec<ActivityItem> {
    let mut ordered: Vec<ActivityItem> = items.into_iter().collect();
    ordered.sort_by(|left, right| {
        state_rank(&left.state)
            .cmp(&state_rank(&right.state))
            .then_with(|| left.unseen.cmp(&right.unseen))
            .then_with(|| (-right.since).total_cmp(&(-left.since)))
    });
    ordered
}

fn upsert(out: &mut Vec<ActivityItem>, item: ActivityItem) {
    if let Some(index) = out.iter().position(|stored| stored.key == item.key) {
        out[index] = item;
    } else {
        out.push(item);
    }
}

/// Une lo vivo y lo reciente; lo vivo gana si comparten clave.
pub fn merge(
    live: impl IntoIterator<Item = ActivityItem>,
    recent: impl IntoIterator<Item = ActivityItem>,
) -> Vec<ActivityItem> {
    let mut out = Vec::new();
    for item in recent {
        upsert(&mut out, item);
    }
    for item in live {
        upsert(&mut out, item);
    }
    order(out)
}

/// Estados de las marcas de la órbita, en orden de prioridad.
pub fn orbit(items: impl IntoIterator<Item = ActivityItem>, limit: usize) -> Vec<String> {
    order(items.into_iter().filter(ActivityItem::marked))
        .into_iter()
        .take(limit)
        .map(|item| item.state)
        .collect()
}

/// Lo que `live_items` necesita de la app.
///
/// Cada método corresponde a un atributo del objeto Python; el valor por
/// defecto replica el `getattr(..., None)` de Python: sin watcher el estado es
/// «working» y sin Herdr no hay etiqueta visible.
pub trait ActivitySource {
    fn watched(&self) -> Vec<String> {
        Vec::new()
    }

    fn task(&self, _address: &str) -> Option<Map<String, Value>> {
        None
    }

    fn state_of(&self, _address: &str) -> Option<String> {
        // Sin watcher, Python usa «working»; con watcher, `None` es un estado
        // desconocido y `live_items` lo pinta como «Sin actualización reciente».
        Some("working".to_string())
    }

    fn label_for(&self, _address: &str) -> Option<String> {
        None
    }

    fn chains(&self) -> Vec<Map<String, Value>> {
        Vec::new()
    }

    fn active_swarms(&self) -> Vec<(String, String)> {
        Vec::new()
    }
}

/// `herdr.visible_name` sobre el único dato que usa `live_items`: el agente.
// DECISION: `herdr.py` aún no está portado (`herdr.rs` es esqueleto); se
// reproduce aquí la función completa para no tocar archivos fuera del brief.
fn visible_name(item: &Map<String, Value>) -> String {
    let where_text = collapse_spaces(&text_or_empty(item.get("where")));
    let (mut space, mut tab) = (String::new(), String::new());
    if let Some((left, right)) = where_text.split_once(" / ") {
        space = py_strip(left).to_string();
        tab = py_strip(right).to_string();
    } else if !where_text.is_empty() {
        tab = where_text;
    }
    if space.is_empty() {
        space = collapse_spaces(&text_or_empty(item.get("space")));
    }
    if tab.is_empty() {
        tab = collapse_spaces(&text_or_empty(item.get("name")));
    }
    if !tab.is_empty() && tab.chars().all(char::is_numeric) {
        tab = format!("pestaña {tab}");
    }
    if !tab.is_empty() && !space.is_empty() {
        return format!("{tab} de {space}");
    }
    if !tab.is_empty() {
        return tab;
    }
    let raw_agent = text_or_empty(item.get("agent"));
    let raw_agent = py_strip(&raw_agent);
    let agent = match raw_agent.to_lowercase().as_str() {
        "claude-code" | "claude" => "Claude Code",
        "opencode" => "OpenCode",
        "codex" => "Codex",
        "gemini" => "Gemini",
        _ => raw_agent,
    };
    let cwd = text_or_empty(item.get("cwd"));
    let folder = Path::new(&cwd)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !agent.is_empty() && !folder.is_empty() {
        return format!("{agent} en {folder}");
    }
    if agent.is_empty() {
        "el agente".to_string()
    } else {
        agent.to_string()
    }
}

fn title_with_agent(info: Option<&Map<String, Value>>) -> String {
    let mut item = Map::new();
    if let Some(agent) = info.and_then(|info| info.get("agent")) {
        item.insert("agent".to_string(), agent.clone());
    }
    visible_name(&item)
}

fn chain_title(spec: &Map<String, Value>) -> String {
    let chosen = ["name", "task", "agent"]
        .iter()
        .find_map(|key| spec.get(*key).filter(|value| truthy(value)));
    chosen.map(python_str).unwrap_or_else(|| "None".to_string())
}

/// Tareas vivas de la app, sin llamar a Herdr (solo estados ya calculados).
pub fn live_items(source: &dyn ActivitySource, now: Option<f64>) -> Vec<ActivityItem> {
    let now = now.unwrap_or_else(timestamp);
    let mut items = Vec::new();
    for address in source.watched() {
        let info = source.task(&address);
        let raw = source
            .state_of(&address)
            .unwrap_or_else(|| "None".to_string());
        if raw == "idle" || raw == "done" {
            continue; // el aviso de fin está en camino; no se adelanta
        }
        // Title = the tab the user sees, never their own request read back to them.
        let shown = source.label_for(&address).unwrap_or_default();
        let title = if shown.is_empty() {
            title_with_agent(info.as_ref())
        } else {
            shown
        };
        items.push(ActivityItem {
            key: address,
            title: clean_title(&title, "Tarea"),
            agent: String::new(),
            state: live_state(&raw).to_string(),
            since: number_or(info.as_ref().and_then(|info| info.get("created_at")), now),
            unseen: false,
            live: true,
        });
    }
    for spec in source.chains() {
        items.push(ActivityItem {
            key: chain_key(&spec),
            title: clean_title(&chain_title(&spec), "Tarea"),
            agent: text_or_empty(spec.get("agent")),
            state: WAITING.to_string(),
            since: number_or(spec.get("created_at"), now),
            unseen: false,
            live: true,
        });
    }
    for (swarm_id, goal) in source.active_swarms() {
        items.push(ActivityItem {
            key: format!("swarm:{swarm_id}"),
            title: clean_title(&goal, "Enjambre"),
            agent: "enjambre".to_string(),
            state: WORKING.to_string(),
            since: now,
            unseen: false,
            live: true,
        });
    }
    items
}

pub fn snapshot(source: &dyn ActivitySource, log: Option<&ActivityLog>) -> Vec<ActivityItem> {
    let recent = log.map(ActivityLog::items).unwrap_or_default();
    merge(live_items(source, None), recent)
}

#[cfg(test)]
mod tests;

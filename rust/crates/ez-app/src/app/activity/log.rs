//! Registro de actividad perezoso y puente entre `ez-herdr` y el HUD.
//!
//! `_activity_log(app)` crea el `ActivityLog` la primera vez y lo guarda en la
//! app. `VoiceApp` no tiene un campo libre para él (el estado es de F6-01), así
//! que se guarda en un registro por instancia con `Weak<Inner>`: la entrada
//! desaparece cuando la app se suelta.

use std::sync::{Arc, OnceLock, Weak};

use ez_herdr::activity::{chain_key, snapshot, ActivityItem, ActivityLog, ActivitySource};
use ez_hud::win::drop::ActivityItem as PeekItem;
use parking_lot::Mutex;
use serde_json::{Map, Value};

use super::super::core::state::Inner;
use super::super::core::{log, VoiceApp};

type ActivityRegistry = Mutex<Vec<(Weak<Inner>, Arc<ActivityLog>)>>;

static LOGS: OnceLock<ActivityRegistry> = OnceLock::new();

/// `_activity_log(app)`: el registro de esta app, creado al primer uso.
pub(super) fn activity_log(app: &VoiceApp) -> Arc<ActivityLog> {
    let logs = LOGS.get_or_init(|| Mutex::new(Vec::new()));
    let weak = Arc::downgrade(&app.inner);
    let mut logs = logs.lock();
    logs.retain(|(candidate, _)| candidate.strong_count() > 0);
    if let Some((_, log)) = logs
        .iter()
        .find(|(candidate, _)| Weak::ptr_eq(candidate, &weak))
    {
        return Arc::clone(log);
    }
    let log = Arc::new(ActivityLog::default());
    logs.push((weak, Arc::clone(&log)));
    log
}

/// `ActivityItem` de `ez-herdr` al ítem que pinta el HUD.
fn peek_item(item: &ActivityItem) -> PeekItem {
    PeekItem {
        key: item.key.clone(),
        title: item.title.clone(),
        agent: item.agent.clone(),
        state: item.state.clone(),
        since: item.since,
        unseen: item.unseen,
        live: item.live,
    }
}

impl ActivitySource for VoiceApp {
    fn watched(&self) -> Vec<String> {
        self.inner.agents.watched.lock().iter().cloned().collect()
    }

    fn task(&self, address: &str) -> Option<Map<String, Value>> {
        self.inner.agents.tasks.get(address)
    }

    fn state_of(&self, address: &str) -> Option<String> {
        self.inner.agents.watcher.state_of(address)
    }

    fn label_for(&self, address: &str) -> Option<String> {
        let label = self.inner.agents.herdr.label_for(address);
        (!label.is_empty()).then_some(label)
    }

    fn chains(&self) -> Vec<Map<String, Value>> {
        self.inner.agents.tasks.chains()
    }

    fn active_swarms(&self) -> Vec<(String, String)> {
        self.inner
            .agents
            .swarm
            .lock()
            .clone()
            .map(|stack| stack.supervisor.active_view())
            .unwrap_or_default()
    }
}

/// `_refresh_activity(app)`: publica la foto de la actividad en la barra.
pub fn refresh_activity(app: &VoiceApp) {
    let log = activity_log(app);
    let items = snapshot(app, Some(&log));
    app.hud()
        .set_activity(items.iter().map(peek_item).collect());
}

/// `_news_arrived(app, key, agent, title, reason)`: desenlace sin ver + aviso.
pub fn news_arrived(app: &VoiceApp, key: &str, agent: &str, title: &str, reason: &str) {
    let item = activity_log(app).record(key, agent, title, reason);
    refresh_activity(app);
    app.hud().announce_activity(&peek_item(&item));
}

/// `_news_seen(app)`: marca todo como visto y repinta si algo cambió.
pub fn news_seen(app: &VoiceApp) {
    if activity_log(app).seen_all() {
        refresh_activity(app);
    }
}

/// `_open_activity(app, key)`: fila pulsada en la vista rápida.
pub fn open_activity(app: &VoiceApp, key: &str) {
    news_seen(app);
    if key.is_empty() || key.starts_with("chain:") || key.starts_with("swarm:") {
        return;
    }
    let worker = app.clone();
    let address = key.to_string();
    app.start_worker("ezagent-focus", move || focus_quietly(&worker, &address));
}

/// `_dismiss_activity(app, key)`: la fila sale de la actividad y de pendientes.
pub fn dismiss_activity(app: &VoiceApp, key: &str) {
    activity_log(app).forget(key);
    if key.starts_with("chain:") {
        for spec in app.inner.agents.tasks.chains() {
            if chain_key(&spec) == key {
                if let Err(error) = app.inner.agents.tasks.remove_chain(&spec) {
                    tracing::debug!("No pude quitar la cadena {key}: {error}");
                }
            }
        }
    } else if app.inner.agents.watched.lock().contains(key) {
        app.inner.agents.watched.lock().remove(key);
        if let Err(error) = app.inner.agents.tasks.remove_chains_for(key) {
            tracing::debug!("No pude quitar las cadenas de {key}: {error}");
        }
        if let Err(error) = app.inner.agents.tasks.remove(key) {
            tracing::debug!("No pude quitar la tarea de {key}: {error}");
        }
    }
    log(format!("Actividad: quité «{key}» de la vista."));
    refresh_activity(app);
}

/// `_focus_quietly(focus, address)`: traer la terminal no puede tumbar la app.
fn focus_quietly(app: &VoiceApp, address: &str) {
    if let Err(error) = app.inner.agents.herdr.focus_agent(address) {
        log(format!("No pude traer la terminal {address}: {error}"));
    }
}

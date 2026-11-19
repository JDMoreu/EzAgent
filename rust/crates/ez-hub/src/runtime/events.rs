//! Hilo de eventos de herdr-core: traduce estados a señales del árbitro (§6 y §9).

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use ez_arbiter::{Arbiter, FishKey};
use ez_term::{EventStream, TermEvent, Terminal};
use tauri::{AppHandle, Emitter};

use crate::runtime::launcher::RuntimeLauncher;

const ALL_EZ: &str = "ez";
const EZ_LABEL: &str = "1";

/// Señal que el hilo envía al árbitro por un cambio de un pez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FishSignal {
    /// El pez pidió permiso o hizo una pregunta.
    Blocked(FishKey, Option<String>),
    /// El pez falló o se perdió antes de informar.
    Failed(FishKey, String),
    /// El pez terminó su terminal.
    Exited(FishKey),
}

/// Suscribe un hilo a los eventos de hc y los entrega al árbitro.
pub fn spawn(
    hc: &Path,
    arbiter: Arc<Arbiter>,
    launcher: Arc<RuntimeLauncher>,
    app: Arc<parking_lot::Mutex<Option<AppHandle>>>,
) -> Result<(), String> {
    let labels = BTreeMap::from([(ALL_EZ.to_string(), EZ_LABEL.to_string())]);
    let stream = EventStream::subscribe(hc, &labels).map_err(|error| error.to_string())?;
    thread::Builder::new()
        .name("ez-hub-hc-events".to_string())
        .spawn(move || run(stream, arbiter, launcher, app))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn run(
    stream: EventStream,
    arbiter: Arc<Arbiter>,
    launcher: Arc<RuntimeLauncher>,
    app: Arc<parking_lot::Mutex<Option<AppHandle>>>,
) {
    let mut keys: HashMap<String, FishKey> = HashMap::new();
    for terminal in stream.snapshot() {
        if let Some(key) = fish_key(&terminal.labels) {
            keys.insert(terminal.address.clone(), key);
        }
    }
    while let Ok(event) = stream.events().recv() {
        if matches!(event, TermEvent::Missed { snapshot: None, .. }) {
            let labels = BTreeMap::from([(ALL_EZ.to_string(), EZ_LABEL.to_string())]);
            if let Ok(terminals) = launcher.list(&labels) {
                keys = keys_from(&terminals);
            }
            notify(&app);
            continue;
        }
        if let Some(signal) = translate(&event, &mut keys) {
            match signal {
                FishSignal::Blocked(key, kind) => arbiter.fish_blocked(&key, kind.as_deref()),
                FishSignal::Failed(key, motive) => arbiter.fish_failed(&key, &motive),
                FishSignal::Exited(key) => arbiter.fish_exited(&key),
            }
            notify(&app);
        }
    }
}

/// Traduce un evento a una señal y actualiza el mapa de terminales conocidas.
pub(crate) fn translate(
    event: &TermEvent,
    keys: &mut HashMap<String, FishKey>,
) -> Option<FishSignal> {
    match event {
        TermEvent::Created { terminal, .. } => {
            if let Some(key) = fish_key(&terminal.labels) {
                keys.insert(terminal.address.clone(), key);
            }
            None
        }
        TermEvent::Status {
            terminal, status, ..
        } => {
            let key = keys.get(terminal)?.clone();
            match status.state.as_str() {
                "blocked" => Some(FishSignal::Blocked(key, status.kind.clone())),
                "error" => Some(FishSignal::Failed(key, status.msg.clone())),
                _ => None,
            }
        }
        TermEvent::Finished {
            terminal,
            reason: _,
            ..
        } => {
            let key = keys.remove(terminal)?;
            Some(FishSignal::Exited(key))
        }
        TermEvent::Lost { terminal, .. } => {
            let key = keys.remove(terminal)?;
            Some(FishSignal::Failed(
                key,
                "La terminal del pez se perdió.".to_string(),
            ))
        }
        TermEvent::Missed {
            snapshot: Some(terminals),
            ..
        } => {
            *keys = keys_from(terminals);
            None
        }
        _ => None,
    }
}

fn keys_from(terminals: &[Terminal]) -> HashMap<String, FishKey> {
    terminals
        .iter()
        .filter_map(|terminal| {
            fish_key(&terminal.labels).map(|key| (terminal.address.clone(), key))
        })
        .collect()
}

fn fish_key(labels: &BTreeMap<String, String>) -> Option<FishKey> {
    if labels.get("ez_role").map(String::as_str) != Some("fish") {
        return None;
    }
    Some(FishKey {
        space: labels.get("ez_space")?.clone(),
        task: labels.get("ez_task")?.clone(),
        fish: labels.get("ez_fish")?.clone(),
        attempt: labels
            .get("ez_attempt")
            .and_then(|value| value.parse().ok())
            .unwrap_or(1),
    })
}

fn notify(app: &Arc<parking_lot::Mutex<Option<AppHandle>>>) {
    let handle = { app.lock().clone() };
    if let Some(handle) = handle {
        if let Err(error) = handle.emit("hub://changed", ()) {
            tracing::debug!(%error, "la vista ya no escucha cambios");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ez_term::{Status, Terminal};

    fn labels() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("ez".to_string(), "1".to_string()),
            ("ez_role".to_string(), "fish".to_string()),
            ("ez_space".to_string(), "s".to_string()),
            ("ez_task".to_string(), "t".to_string()),
            ("ez_fish".to_string(), "dev-r1-1".to_string()),
            ("ez_attempt".to_string(), "1".to_string()),
        ])
    }

    fn key() -> FishKey {
        FishKey {
            space: "s".to_string(),
            task: "t".to_string(),
            fish: "dev-r1-1".to_string(),
            attempt: 1,
        }
    }

    fn created() -> TermEvent {
        let mut terminal = Terminal::default();
        terminal.address = "t7".to_string();
        terminal.labels = labels();
        TermEvent::Created {
            terminal,
            seq: 1,
        }
    }

    #[test]
    fn translates_blocked_with_its_kind() {
        let mut keys = HashMap::new();
        assert!(translate(&created(), &mut keys).is_none());
        let status = Status {
            state: "blocked".to_string(),
            kind: Some("permission".to_string()),
            ..Status::default()
        };
        let event = TermEvent::Status {
            terminal: "t7".to_string(),
            status,
            seq: 2,
        };
        assert_eq!(
            translate(&event, &mut keys),
            Some(FishSignal::Blocked(key(), Some("permission".to_string())))
        );
    }

    #[test]
    fn translates_error_finished_and_lost() {
        let mut keys = HashMap::new();
        translate(&created(), &mut keys);
        let event = TermEvent::Status {
            terminal: "t7".to_string(),
            status: Status {
                state: "error".to_string(),
                msg: "se cayó".to_string(),
                ..Status::default()
            },
            seq: 2,
        };
        assert_eq!(
            translate(&event, &mut keys),
            Some(FishSignal::Failed(key(), "se cayó".to_string()))
        );
        let event = TermEvent::Finished {
            terminal: "t7".to_string(),
            exit_code: Some(0),
            reason: None,
            seq: 3,
        };
        assert_eq!(translate(&event, &mut keys), Some(FishSignal::Exited(key())));
        translate(&created(), &mut keys);
        let event = TermEvent::Lost {
            terminal: "t7".to_string(),
            seq: 4,
        };
        assert!(matches!(translate(&event, &mut keys), Some(FishSignal::Failed(..))));
    }

    #[test]
    fn missed_with_a_snapshot_resynchronizes_the_keys() {
        let mut terminal = Terminal::default();
        terminal.address = "t9".to_string();
        terminal.labels = labels();
        let event = TermEvent::Missed {
            snapshot: Some(vec![terminal]),
            seq: Some(7),
        };
        let mut keys = HashMap::new();
        assert!(translate(&event, &mut keys).is_none());
        assert_eq!(keys.get("t9"), Some(&key()));
    }

    #[test]
    fn ignores_terminals_that_are_not_fish() {
        let mut terminal = Terminal::default();
        terminal.address = "e1".to_string();
        terminal.labels = BTreeMap::from([
            ("ez".to_string(), "1".to_string()),
            ("ez_role".to_string(), "elephant".to_string()),
        ]);
        let event = TermEvent::Created {
            terminal,
            seq: 1,
        };
        let mut keys = HashMap::new();
        assert!(translate(&event, &mut keys).is_none());
        assert!(keys.is_empty());
        assert!(fish_key(&BTreeMap::from([(
            "ez_fish".to_string(),
            "dev".to_string()
        )]))
        .is_none());
    }
}

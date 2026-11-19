//! Arranque (reinicio) y tarjetas del arbitro (diseno 14.5 y 14.8).

use std::sync::atomic::Ordering;

use crate::events::{ArbiterEvent, Card, CardKind, EventKind};
use crate::state::{Task, WorkState};
use crate::texts;
use crate::types::FishKey;

use super::{emit, ArbiterShared};

impl ArbiterShared {
    /// Pausa las tareas no finales al reiniciar y falla su trabajo pendiente
    /// con sus eventos (diseño §14.5).
    pub(crate) fn restart(&self) {
        let tasks: Vec<Task> = self.inner.lock().tasks.values().cloned().collect();
        let now = self.now();
        for mut task in tasks {
            let mut effects = emit::Effects::default();
            let failed: Vec<FishKey> = task
                .work
                .iter_mut()
                .filter(|item| item.state == WorkState::Pending)
                .map(|item| {
                    item.state = WorkState::Failed;
                    item.key.clone()
                })
                .collect();
            for key in failed {
                let data = serde_json::json!({
                    "fish": key.fish, "attempt": key.attempt, "reason": "reinicio",
                });
                effects.both(
                    self.event(&task, "fish_failed", data.clone()),
                    self.elephant_event(
                        &task,
                        "fish_failed",
                        texts::fish_failed(&key.fish, "reinicio"),
                        data,
                    ),
                );
            }
            task.pause("reinicio", texts::RESTART.to_string(), now);
            let card = self.open_card(
                &task.space.id,
                Some(&task.id),
                CardKind::Pause,
                "Tarea en pausa".to_string(),
                texts::RESTART,
            );
            effects.event(self.card_opened_event(&card));
            effects.both(
                self.event(
                    &task,
                    "paused",
                    serde_json::json!({ "reason": "reinicio", "message": texts::RESTART }),
                ),
                self.elephant_event(
                    &task,
                    "paused",
                    texts::RESTART.to_string(),
                    serde_json::json!({ "reason": "reinicio" }),
                ),
            );
            self.commit(&task, effects);
        }
    }

    // --------------------------------------------------------------- tarjetas

    /// Siguiente número de secuencia del proceso (§14.8).
    pub(crate) fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Abre una tarjeta, la guarda y devuelve su copia para el evento.
    pub(crate) fn open_card(
        &self,
        space: &str,
        task: Option<&str>,
        kind: CardKind,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Card {
        let id = format!("c{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        let card = Card {
            id,
            space: super::normalize_scope(space),
            task: task.map(str::to_string),
            kind,
            title: title.into(),
            body: body.into(),
        };
        self.inner
            .lock()
            .cards
            .insert(card.id.clone(), card.clone());
        card
    }

    /// Cierra (y devuelve) la primera tarjeta abierta de un tipo en un espacio.
    pub(crate) fn take_card(&self, space: &str, kind: CardKind) -> Option<Card> {
        let wanted = super::normalize_scope(space);
        let mut inner = self.inner.lock();
        let id = inner
            .cards
            .values()
            .find(|card| card.space == wanted && card.kind == kind)
            .map(|card| card.id.clone());
        id.and_then(|id| inner.cards.remove(&id))
    }

    /// Elimina una tarjeta por id (tras decidir desde `card_action`).
    pub(crate) fn close_card(&self, id: &str) {
        self.inner.lock().cards.remove(id);
    }

    /// Cambia el cuerpo de una tarjeta abierta (fallo de integración, §15.1).
    pub(crate) fn set_card_body(&self, id: &str, body: String) {
        if let Some(card) = self.inner.lock().cards.get_mut(id) {
            card.body = body;
        }
    }

    /// Evento de apertura de una tarjeta para la traza.
    pub(crate) fn card_opened_event(&self, card: &Card) -> ArbiterEvent {
        ArbiterEvent {
            seq: self.next_seq(),
            kind: EventKind::CardOpened(card.clone()),
            space: super::scope_space(&card.space),
            elephant: super::scope_elephant(&card.space),
            task: card.task.clone(),
            ts: self.now(),
        }
    }

    /// Evento de cierre de una tarjeta para la traza.
    pub(crate) fn card_closed_event(
        &self,
        space: &str,
        task: Option<&str>,
        id: &str,
    ) -> ArbiterEvent {
        ArbiterEvent {
            seq: self.next_seq(),
            kind: EventKind::CardClosed { id: id.to_string() },
            space: super::scope_space(space),
            elephant: super::scope_elephant(space),
            task: task.map(str::to_string),
            ts: self.now(),
        }
    }
}

//! Equipo propuesto y su aprobación de la persona.

use ez_team::Seat;

use crate::events::CardKind;
use crate::state::{Status, TeamStatus};
use crate::texts;
use crate::types::{Outcome, Rejection};

use super::emit::Effects;
use super::ArbiterShared;

impl ArbiterShared {
    /// Guarda el equipo propuesto (ya validado por `ez-team`) y emite la tarjeta.
    pub(crate) fn team_propose(
        &self,
        space: &str,
        seats: Vec<Seat>,
        card: String,
    ) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.status == Status::InRound {
            return Err(Rejection::new("R5", texts::ROUND_OPEN));
        }
        if task.status.is_terminal() {
            return Err(Rejection::new("R2", texts::TASK_FINISHED));
        }
        task.team = seats;
        task.team_status = TeamStatus::Proposed;
        task.team_card = card.clone();
        let mut effects = Effects::default();
        effects.event(self.event(
            &task,
            "team_proposed",
            serde_json::json!({ "card": card.clone(), "seats": task.team }),
        ));
        let opened = self.open_card(
            space,
            Some(&task.id),
            CardKind::Team,
            "Equipo propuesto".to_string(),
            card.clone(),
        );
        effects.event(self.card_opened_event(&opened));
        self.commit(&task, effects);
        Ok(Outcome {
            message: card,
            data: serde_json::json!({ "team": "proposed" }),
        })
    }

    /// La persona aprueba el equipo.
    pub(crate) fn approve_team_impl(&self, space: &str) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.team_status != TeamStatus::Proposed {
            return Err(Rejection::new("R5", texts::TEAM_NOT_PROPOSED));
        }
        task.team_status = TeamStatus::Approved;
        let sha = self.freeze_team(&task)?;
        task.team_sha = Some(sha);
        let mut effects = Effects::default();
        effects.both(
            self.event(&task, "team_approved", serde_json::json!({})),
            self.elephant_event(
                &task,
                "team_approved",
                texts::TEAM_APPROVED.to_string(),
                serde_json::json!({ "team": "approved" }),
            ),
        );
        if let Some(card) = self.take_card(space, CardKind::Team) {
            effects.event(self.card_closed_event(space, Some(&task.id), &card.id));
        }
        self.commit(&task, effects);
        Ok(Outcome::new(texts::TEAM_APPROVED))
    }

    /// La persona rechaza el equipo con un comentario para el elefante.
    pub(crate) fn reject_team_impl(
        &self,
        space: &str,
        comment: &str,
    ) -> Result<Outcome, Rejection> {
        let mut task = self.task_for_elephant(space)?;
        if task.team_status != TeamStatus::Proposed {
            return Err(Rejection::new("R5", texts::TEAM_NOT_PROPOSED));
        }
        task.team_status = TeamStatus::Rejected;
        self.revoke_task_tokens(&task);
        let message = texts::team_rejected(comment);
        let mut events = Effects::default();
        events.both(
            self.event(
                &task,
                "team_rejected",
                serde_json::json!({ "comment": comment }),
            ),
            self.elephant_event(
                &task,
                "team_rejected",
                message.clone(),
                serde_json::json!({ "comment": comment }),
            ),
        );
        if let Some(card) = self.take_card(space, CardKind::Team) {
            events.event(self.card_closed_event(space, Some(&task.id), &card.id));
        }
        self.commit(&task, events);
        Ok(Outcome::new(message))
    }
}

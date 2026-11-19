//! Estado que pinta la columna derecha: peces, traza y tarjetas de aprobación (§10).

use serde::{Deserialize, Serialize};

use crate::space::{Elephant, Space};
use crate::texts;

/// Estado de un pez, con los mismos nombres que usa herdr-core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FishStatus {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
}

/// Un pez visible en su subterminal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fish {
    pub id: String,
    /// Nombre del papel para mostrar («Desarrollador», «QA»…).
    pub role: String,
    pub agent: String,
    pub model: String,
    pub status: FishStatus,
    /// Programa que ejecuta su vista; en la ola 2 será `hc attach <t>`.
    pub argv: Vec<String>,
    pub cwd: String,
}

/// Un evento de la traza.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub time: String,
    pub kind: String,
    pub text: String,
}

/// Una tarjeta de aprobación de la traza.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Card {
    pub id: String,
    pub title: String,
    pub body: String,
}

/// Respuesta de `hub_state`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubState {
    pub fish: Vec<Fish>,
    pub timeline: Vec<TimelineEvent>,
    pub cards: Vec<Card>,
}

/// Una fila de la tarjeta de equipo (§6).
pub struct SeatLine<'a> {
    pub role: &'a str,
    pub agent: &'a str,
    pub model: &'a str,
    pub effort: Option<&'a str>,
    pub count: u32,
}

/// Arma la tarjeta de equipo con el formato fijado en §6.
// DECISION: el encabezado va en `title` y las filas en `body`; la vista ya pinta el
// título aparte y repetirlo en el cuerpo se vería duplicado.
pub fn team_card(task: &str, seats: &[SeatLine<'_>]) -> Card {
    let body = seats
        .iter()
        .map(|seat| {
            let mut line = format!("- {}: {} · {}", seat.role, seat.agent, seat.model);
            if let Some(effort) = seat.effort {
                line.push_str(&format!(" · esfuerzo {effort}"));
            }
            if seat.count > 1 {
                line.push_str(&format!(" · ×{}", seat.count));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n");
    Card {
        id: "equipo".to_string(),
        title: format!("Equipo propuesto para «{task}»"),
        body,
    }
}

/// Id del espacio en memoria del modo demostración.
pub const DEMO_SPACE_ID: &str = "demo";

/// Espacio «Demostración»: no se guarda en `espacios.json`.
pub fn demo_space(folder: &str) -> Space {
    Space {
        id: DEMO_SPACE_ID.to_string(),
        name: "Demostración".to_string(),
        folder: folder.to_string(),
        elephant: Elephant {
            agent: "powershell".to_string(),
            argv: vec!["powershell.exe".to_string(), "-NoLogo".to_string()],
        },
    }
}

/// Estado de demostración: dos peces, cinco eventos y una tarjeta de equipo.
pub fn demo_state(folder: &str) -> HubState {
    let shell = || vec!["powershell.exe".to_string(), "-NoLogo".to_string()];
    let fish = vec![
        Fish {
            id: "dev-r1-1".to_string(),
            role: texts::role_name("developer").to_string(),
            agent: texts::agent_name("opencode").to_string(),
            model: "deepseek-v4.1-flash".to_string(),
            status: FishStatus::Working,
            argv: shell(),
            cwd: folder.to_string(),
        },
        Fish {
            id: "qa-r1".to_string(),
            role: texts::role_name("qa").to_string(),
            agent: texts::agent_name("codex").to_string(),
            model: "gpt-6-sol".to_string(),
            status: FishStatus::Idle,
            argv: shell(),
            cwd: folder.to_string(),
        },
    ];
    let timeline = vec![
        event(
            "09:41",
            "task_approved",
            "La persona aprobó el encargo y los criterios.",
        ),
        event(
            "09:42",
            "team_approved",
            "Equipo aprobado: planificador, desarrollador, QA y validador de QA.",
        ),
        event("09:43", "round_started", "Ronda 1 abierta."),
        event(
            "09:44",
            "fish_started",
            "dev-r1-1 empezó a trabajar en la ronda 1.",
        ),
        event(
            "10:02",
            "fish_reported",
            "qa-r1 informó: pass (0 bloqueantes, 0 mayores, 0 menores).",
        ),
    ];
    let card = team_card(
        "Transcriptor con whisper.cpp",
        &[
            SeatLine {
                role: texts::role_name("planner"),
                agent: texts::agent_name("opencode"),
                model: "opencode-go/deepseek-v4.1-flash",
                effort: None,
                count: 1,
            },
            SeatLine {
                role: texts::role_name("developer"),
                agent: texts::agent_name("opencode"),
                model: "opencode-go/deepseek-v4.1-flash",
                effort: Some("max"),
                count: 1,
            },
            SeatLine {
                role: texts::role_name("qa"),
                agent: texts::agent_name("codex"),
                model: "gpt-6-sol",
                effort: None,
                count: 1,
            },
            SeatLine {
                role: texts::role_name("qa_validator"),
                agent: texts::agent_name("claude"),
                model: "opus",
                effort: None,
                count: 1,
            },
        ],
    );
    HubState {
        fish,
        timeline,
        cards: vec![card],
    }
}

fn event(time: &str, kind: &str, text: &str) -> TimelineEvent {
    TimelineEvent {
        time: time.to_string(),
        kind: kind.to_string(),
        text: text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_has_nothing() {
        let state = HubState::default();
        assert!(state.fish.is_empty());
        assert!(state.timeline.is_empty());
        assert!(state.cards.is_empty());
    }

    #[test]
    fn demo_has_two_fish_with_the_agreed_states() {
        let state = demo_state("C:\\proyecto");
        assert_eq!(state.fish.len(), 2);
        assert_eq!(state.fish[0].id, "dev-r1-1");
        assert_eq!(state.fish[0].status, FishStatus::Working);
        assert_eq!(state.fish[0].role, "Desarrollador");
        assert_eq!(state.fish[0].agent, "OpenCode");
        assert_eq!(state.fish[1].id, "qa-r1");
        assert_eq!(state.fish[1].status, FishStatus::Idle);
        assert_eq!(state.fish[1].agent, "Codex");
    }

    #[test]
    fn demo_has_five_events_and_one_card() {
        let state = demo_state("C:\\proyecto");
        assert_eq!(state.timeline.len(), 5);
        assert_eq!(state.cards.len(), 1);
        assert_eq!(
            state.cards[0].title,
            "Equipo propuesto para «Transcriptor con whisper.cpp»"
        );
        assert!(state.cards[0].body.contains(
            "- Desarrollador: OpenCode · opencode-go/deepseek-v4.1-flash · esfuerzo max"
        ));
        assert!(state.cards[0].body.contains("- QA: Codex · gpt-6-sol"));
        assert!(state.cards[0]
            .body
            .contains("- Validador de QA: Claude Code · opus"));
    }

    #[test]
    fn card_shows_count_only_above_one() {
        let card = team_card(
            "T",
            &[SeatLine {
                role: "Desarrollador",
                agent: "OpenCode",
                model: "m",
                effort: None,
                count: 2,
            }],
        );
        assert!(card.body.contains("×2"));
    }

    #[test]
    fn status_serializes_in_lowercase() {
        let json = serde_json::to_string(&FishStatus::Blocked).expect("json");
        assert_eq!(json, "\"blocked\"");
    }

    #[test]
    fn demo_space_is_marked_as_demo() {
        let space = demo_space("C:\\proyecto");
        assert_eq!(space.id, DEMO_SPACE_ID);
        assert_eq!(space.elephant.argv, vec!["powershell.exe", "-NoLogo"]);
    }
}

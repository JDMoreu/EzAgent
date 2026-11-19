//! `equipo.toml`: parse, validate against the inventory and build the approval card
//! (design §6).

use serde::Deserialize;

use crate::contract::Seat;
use crate::error::TeamError;
use crate::family::family;
use crate::inventory::Inventory;
use crate::roles::Role;

/// Problems found in a proposed team. Errors block the approval; warnings only show.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TeamCheck {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
struct RawTeam {
    #[serde(default, rename = "papel")]
    seats: Vec<RawSeat>,
}

#[derive(Deserialize)]
struct RawSeat {
    papel: String,
    agente: String,
    modelo: String,
    #[serde(default)]
    esfuerzo: Option<String>,
    #[serde(default = "default_count")]
    cantidad: u32,
}

fn default_count() -> u32 {
    1
}

/// Parses `equipo.toml`. Unknown or repeated roles, `cantidad` out of range and empty
/// fields are errors that name the offending row.
pub fn parse(text: &str) -> Result<Vec<Seat>, TeamError> {
    let raw: RawTeam = toml::from_str(text)
        .map_err(|error| TeamError::Invalid(format!("equipo.toml no es válido: {error}.")))?;
    let mut seats: Vec<Seat> = Vec::with_capacity(raw.seats.len());
    for (index, row) in raw.seats.iter().enumerate() {
        let number = index + 1;
        let role = Role::parse(&row.papel).ok_or_else(|| {
            let known: Vec<&str> = Role::ALL.iter().map(|role| role.id()).collect();
            TeamError::Invalid(format!(
                "fila {number}: papel desconocido «{}». Usa uno de: {}.",
                row.papel,
                known.join(", ")
            ))
        })?;
        if row.cantidad == 0 {
            return Err(TeamError::Invalid(format!(
                "fila {number}: cantidad debe ser al menos 1."
            )));
        }
        if row.cantidad > 1 && role != Role::Developer {
            return Err(TeamError::Invalid(format!(
                "fila {number}: solo {} admite cantidad mayor que 1.",
                Role::Developer.label()
            )));
        }
        if seats.iter().any(|seat| seat.role == role) {
            return Err(TeamError::Invalid(format!(
                "fila {number}: el papel {} está repetido.",
                role.label()
            )));
        }
        if row.agente.trim().is_empty() || row.modelo.trim().is_empty() {
            return Err(TeamError::Invalid(format!(
                "fila {number}: agente y modelo son obligatorios."
            )));
        }
        if row
            .esfuerzo
            .as_deref()
            .map(str::trim)
            .is_some_and(|effort| effort.is_empty())
        {
            return Err(TeamError::Invalid(format!(
                "fila {number}: esfuerzo no puede estar vacío."
            )));
        }
        seats.push(Seat {
            role,
            agent: row.agente.trim().to_string(),
            model: row.modelo.trim().to_string(),
            effort: row
                .esfuerzo
                .as_deref()
                .map(str::trim)
                .filter(|effort| !effort.is_empty())
                .map(str::to_string),
            count: row.cantidad,
        });
    }
    Ok(seats)
}

/// Applies the rules of design §6: missing required roles, unavailable agents, models
/// outside the agent's list, QA sharing the developer's model, and the family warnings.
pub fn validate(seats: &[Seat], inventory: &Inventory) -> TeamCheck {
    let mut check = TeamCheck::default();
    for role in Role::ALL.into_iter().filter(|role| role.is_required()) {
        if !seats.iter().any(|seat| seat.role == role) {
            check
                .errors
                .push(format!("Falta el papel obligatorio: {}.", role.label()));
        }
    }
    for seat in seats {
        let Some(agent) = inventory.find(&seat.agent) else {
            check.errors.push(format!(
                "El agente «{}» del papel {} no está disponible.",
                seat.agent,
                seat.role.label()
            ));
            continue;
        };
        if !agent.models.is_empty() && !agent.models.contains(&seat.model) {
            check.errors.push(format!(
                "El modelo «{}» no está en la lista del agente {}.",
                seat.model, agent.profile.name
            ));
        }
        check_effort(seat, inventory, &mut check);
    }
    for role in [Role::Qa, Role::QaValidator] {
        let (Some(other), Some(developer)) = (find(seats, role), find(seats, Role::Developer))
        else {
            continue;
        };
        if other.model == developer.model {
            check.errors.push(format!(
                "{} y {} usan el mismo modelo «{}»; elige modelos distintos.",
                role.label(),
                Role::Developer.label(),
                other.model
            ));
        }
    }
    for role in [Role::Qa, Role::QaValidator] {
        warn_same_family(role, seats, inventory, &mut check);
    }
    if let (Some(qa), Some(validator)) = (find(seats, Role::Qa), find(seats, Role::QaValidator)) {
        if qa.model == validator.model {
            check.warnings.push(format!(
                "{} y {} usan el mismo modelo «{}»; conviene que el validador use otro.",
                Role::QaValidator.label(),
                Role::Qa.label(),
                qa.model
            ));
        }
    }
    check
}

fn find(seats: &[Seat], role: Role) -> Option<&Seat> {
    seats.iter().find(|seat| seat.role == role)
}

/// A seat with an effort needs a profile that can apply it (design §14.2).
fn check_effort(seat: &Seat, inventory: &Inventory, check: &mut TeamCheck) {
    let Some(effort) = seat.effort.as_deref().filter(|effort| !effort.is_empty()) else {
        return;
    };
    // An unavailable agent already has its own error; do not pile a second one on it.
    let Some(agent) = inventory.find(&seat.agent) else {
        return;
    };
    let can_apply = !agent.profile.effort_args.is_empty()
        || agent
            .profile
            .env
            .values()
            .any(|value| value.contains("{esfuerzo}") || value.contains("{variante}"));
    if !can_apply {
        // DECISION: the name in the message is the profile's human name when the agent
        // is known; the id would be ambiguous for the person reading the card.
        check.errors.push(format!(
            "{} no puede aplicar el esfuerzo «{effort}»: quita `esfuerzo` o configura `esfuerzo_args` en agentes.toml.",
            agent.profile.name
        ));
    }
}

fn warn_same_family(role: Role, seats: &[Seat], inventory: &Inventory, check: &mut TeamCheck) {
    let (Some(seat), Some(developer)) = (find(seats, role), find(seats, Role::Developer)) else {
        return;
    };
    // DECISION: the same model is already an error; the warning is for different models
    // of one family only, so the person does not see the same problem twice.
    if seat.model == developer.model {
        return;
    }
    let (Some(agent), Some(developer_agent)) = (
        inventory.find(&seat.agent),
        inventory.find(&developer.agent),
    ) else {
        return;
    };
    let developer_family = family(&developer_agent.profile, &developer.model);
    let another_family = inventory.agents.iter().any(|available| {
        available
            .families
            .iter()
            .any(|name| *name != developer_family)
    });
    if another_family && family(&agent.profile, &seat.model) == developer_family {
        check.warnings.push(format!(
            "{} y {} son de la misma familia («{developer_family}»); hay otra familia disponible.",
            role.label(),
            Role::Developer.label()
        ));
    }
}

/// Approval card, with the exact format of design §6: the seat lines and then the
/// warnings. Errors are not part of the card: a proposal with errors is rejected to the
/// elephant instead of shown for approval.
pub fn card(task_title: &str, seats: &[Seat], check: &TeamCheck, inventory: &Inventory) -> String {
    let mut text = format!("Equipo propuesto para «{task_title}»:");
    for seat in seats {
        let name = inventory
            .find(&seat.agent)
            .map(|agent| agent.profile.name.as_str())
            .unwrap_or(&seat.agent);
        let mut line = format!("- {}: {name} · {}", seat.role.label(), seat.model);
        if let Some(effort) = seat.effort.as_deref().filter(|effort| !effort.is_empty()) {
            line.push_str(&format!(" · esfuerzo {effort}"));
        }
        // DECISION: `×{cantidad}` appears only when `cantidad > 1`; §6 marks it optional
        // and printing `×1` on every line is noise.
        if seat.count > 1 {
            line.push_str(&format!(" · ×{}", seat.count));
        }
        text.push('\n');
        text.push_str(&line);
    }
    for warning in &check.warnings {
        text.push_str(&format!("\nAviso: {warning}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{card, parse, validate, TeamCheck};
    use crate::contract::Seat;
    use crate::inventory::{AvailableAgent, Inventory};
    use crate::profiles::AgentProfile;
    use crate::roles::Role;

    fn profile(id: &str, name: &str, models: &[&str], family: Option<&str>) -> AgentProfile {
        AgentProfile {
            id: id.to_string(),
            name: name.to_string(),
            argv: vec![id.to_string()],
            models_cmd: None,
            models: models.iter().map(|model| model.to_string()).collect(),
            family: family.map(str::to_string),
            effort_args: Vec::new(),
            env: std::collections::BTreeMap::new(),
            prelaunch: None,
        }
    }

    fn available(profile: AgentProfile) -> AvailableAgent {
        let families = profile
            .models
            .iter()
            .map(|model| crate::family::family(&profile, model))
            .fold(Vec::new(), |mut names: Vec<String>, name| {
                if !name.is_empty() && !names.contains(&name) {
                    names.push(name);
                }
                names
            });
        AvailableAgent {
            path: std::path::PathBuf::from(format!("C:\\tools\\{}.exe", profile.id)),
            models: profile.models.clone(),
            families,
            profile,
        }
    }

    fn inventory() -> Inventory {
        let mut opencode = profile(
            "opencode",
            "OpenCode",
            &["opencode-go/deepseek-v4.1-flash", "opencode-go/qwen4-max"],
            None,
        );
        opencode.env.insert(
            "OPENCODE_CONFIG_CONTENT".to_string(),
            r#"{"model":"{modelo}{variante}"}"#.to_string(),
        );
        Inventory {
            agents: vec![
                available(opencode),
                available(profile(
                    "claude",
                    "Claude Code",
                    &["opus", "sonnet"],
                    Some("anthropic"),
                )),
                available(profile(
                    "codex",
                    "Codex",
                    &["opencode-go/gpt-6-luna", "other/hy4-max"],
                    None,
                )),
            ],
        }
    }

    fn team_text() -> &'static str {
        r#"[[papel]]
papel = "planner"
agente = "opencode"
modelo = "opencode-go/deepseek-v4.1-flash"

[[papel]]
papel = "developer"
agente = "opencode"
modelo = "opencode-go/deepseek-v4.1-flash"
esfuerzo = "max"
cantidad = 2

[[papel]]
papel = "qa"
agente = "claude"
modelo = "opus"

[[papel]]
papel = "qa_validator"
agente = "codex"
modelo = "opencode-go/gpt-6-luna"
"#
    }

    #[test]
    fn parses_a_full_team_with_defaults() {
        let seats = parse(team_text()).expect("parse");
        assert_eq!(seats.len(), 4);
        assert_eq!(seats[0].role, Role::Planner);
        assert_eq!(seats[0].count, 1);
        assert_eq!(seats[1].count, 2);
        assert_eq!(seats[1].effort.as_deref(), Some("max"));
        assert_eq!(seats[2].agent, "claude");
    }

    #[test]
    fn parse_rejects_unknown_role() {
        let error = parse("[[papel]]\npapel = \"elephant\"\nagente = \"a\"\nmodelo = \"m\"\n")
            .expect_err("unknown role");
        assert!(error.to_string().contains("fila 1"), "{error}");
    }

    #[test]
    fn parse_rejects_bad_count_and_repeated_roles() {
        let zero =
            parse("[[papel]]\npapel = \"qa\"\nagente = \"a\"\nmodelo = \"m\"\ncantidad = 0\n")
                .expect_err("zero");
        assert!(zero.to_string().contains("al menos 1"), "{zero}");

        let many_qa =
            parse("[[papel]]\npapel = \"qa\"\nagente = \"a\"\nmodelo = \"m\"\ncantidad = 2\n")
                .expect_err("qa count");
        assert!(
            many_qa.to_string().contains("cantidad mayor que 1"),
            "{many_qa}"
        );

        let repeated = parse(
            "[[papel]]\npapel = \"qa\"\nagente = \"a\"\nmodelo = \"m\"\n\n\
             [[papel]]\npapel = \"qa\"\nagente = \"b\"\nmodelo = \"n\"\n",
        )
        .expect_err("repeated");
        assert!(repeated.to_string().contains("repetido"), "{repeated}");
    }

    #[test]
    fn valid_team_has_no_problems() {
        let seats = parse(team_text()).expect("parse");
        let check = validate(&seats, &inventory());
        assert_eq!(check, TeamCheck::default(), "{check:?}");
    }

    #[test]
    fn missing_required_role_is_an_error() {
        let seats = parse(
            "[[papel]]\npapel = \"developer\"\nagente = \"opencode\"\nmodelo = \"opencode-go/deepseek-v4.1-flash\"\n",
        )
        .expect("parse");
        let check = validate(&seats, &inventory());
        assert_eq!(check.errors.len(), 3, "{check:?}");
        assert!(check
            .errors
            .iter()
            .any(|error| error.contains("Planificador")));
        assert!(check.errors.iter().any(|error| error.contains("QA")));
        assert!(check
            .errors
            .iter()
            .any(|error| error.contains("Validador de QA")));
    }

    #[test]
    fn unavailable_agent_is_an_error() {
        let mut seats = parse(team_text()).expect("parse");
        seats[0].agent = "missing".to_string();
        let check = validate(&seats, &inventory());
        assert!(
            check.errors.iter().any(|error| error.contains("missing")),
            "{check:?}"
        );
    }

    #[test]
    fn model_outside_the_list_is_an_error() {
        let mut seats = parse(team_text()).expect("parse");
        seats[2].model = "not-listed".to_string();
        let check = validate(&seats, &inventory());
        assert!(
            check
                .errors
                .iter()
                .any(|error| error.contains("not-listed")),
            "{check:?}"
        );
    }

    #[test]
    fn qa_with_the_developer_model_is_an_error() {
        let mut seats = parse(team_text()).expect("parse");
        seats[2].model = "opencode-go/deepseek-v4.1-flash".to_string();
        let check = validate(&seats, &inventory());
        assert!(
            check
                .errors
                .iter()
                .any(|error| error.contains("mismo modelo")),
            "{check:?}"
        );
    }

    #[test]
    fn validator_with_the_developer_model_is_an_error() {
        let mut seats = parse(team_text()).expect("parse");
        seats[3].model = "opencode-go/deepseek-v4.1-flash".to_string();
        let check = validate(&seats, &inventory());
        assert!(
            check
                .errors
                .iter()
                .any(|error| error.contains("mismo modelo")),
            "{check:?}"
        );
    }

    #[test]
    fn parse_rejects_an_empty_effort() {
        let error = parse(
            "[[papel]]\npapel = \"developer\"\nagente = \"codex\"\nmodelo = \"m\"\nesfuerzo = \"  \"\n",
        )
        .expect_err("empty effort");
        assert!(
            error.to_string().contains("esfuerzo no puede estar vacío"),
            "{error}"
        );
    }

    #[test]
    fn effort_without_profile_support_is_an_error() {
        let mut seats = parse(team_text()).expect("parse");
        seats[0].effort = Some("max".to_string());
        let mut stock = inventory();
        stock.agents[0].profile.env.clear();
        let check = validate(&seats, &stock);
        assert!(
            check
                .errors
                .iter()
                .any(|error| error.contains("OpenCode no puede aplicar el esfuerzo")),
            "{check:?}"
        );
    }

    #[test]
    fn effort_in_an_env_marker_is_accepted() {
        let mut seats = parse(team_text()).expect("parse");
        seats[0].effort = Some("max".to_string());
        let mut stock = inventory();
        stock.agents[0].profile.env.insert(
            "OPENCODE_CONFIG_CONTENT".to_string(),
            r#"{"model":"{modelo}{variante}"}"#.to_string(),
        );
        let check = validate(&seats, &stock);
        assert!(
            !check
                .errors
                .iter()
                .any(|error| error.contains("no puede aplicar")),
            "{check:?}"
        );
    }

    #[test]
    fn same_family_warning_only_when_another_family_is_available() {
        let mut seats = parse(team_text()).expect("parse");
        let developer_agent = available(profile(
            "opencode",
            "OpenCode",
            &["opencode-go/deepseek-v4.1-flash"],
            None,
        ));
        let same_family = available(profile(
            "same",
            "Same Family",
            &["opencode-go/deepseek-other"],
            None,
        ));
        let codex = available(profile("codex", "Codex", &[], None));
        seats[2].agent = "same".to_string();
        seats[2].model = "opencode-go/deepseek-other".to_string();

        let with_other = Inventory {
            agents: vec![
                developer_agent.clone(),
                same_family.clone(),
                codex.clone(),
                available(profile(
                    "claude",
                    "Claude Code",
                    &["opus"],
                    Some("anthropic"),
                )),
            ],
        };
        let check = validate(&seats, &with_other);
        assert!(
            check
                .warnings
                .iter()
                .any(|warning| warning.contains("misma familia")),
            "{check:?}"
        );

        // Only the developer family is installed: no other family is available.
        let only_same_family = Inventory {
            agents: vec![developer_agent, same_family, codex],
        };
        let check = validate(&seats, &only_same_family);
        assert!(
            !check
                .warnings
                .iter()
                .any(|warning| warning.contains("misma familia")),
            "{check:?}"
        );
    }

    #[test]
    fn validator_with_the_qa_model_is_a_warning() {
        let mut seats = parse(team_text()).expect("parse");
        seats[3].agent = "claude".to_string();
        seats[3].model = "opus".to_string();
        let check = validate(&seats, &inventory());
        assert!(
            check
                .warnings
                .iter()
                .any(|warning| warning.contains("mismo modelo")),
            "{check:?}"
        );
    }

    #[test]
    fn card_has_the_exact_design_format() {
        let seats = parse(team_text()).expect("parse");
        let check = TeamCheck::default();
        assert_eq!(
            card("Arreglar el HUD", &seats, &check, &inventory()),
            "Equipo propuesto para «Arreglar el HUD»:\n\
             - Planificador: OpenCode · opencode-go/deepseek-v4.1-flash\n\
             - Desarrollador: OpenCode · opencode-go/deepseek-v4.1-flash · esfuerzo max · ×2\n\
             - QA: Claude Code · opus\n\
             - Validador de QA: Codex · opencode-go/gpt-6-luna"
        );
    }

    #[test]
    fn card_lists_only_the_warnings() {
        let seats = parse(team_text()).expect("parse");
        let check = TeamCheck {
            errors: vec!["falla".to_string()],
            warnings: vec!["aviso".to_string()],
        };
        let text = card("T", &seats, &check, &inventory());
        assert!(text.ends_with("Aviso: aviso"), "{text}");
        assert!(!text.contains("Error"), "{text}");
    }

    #[test]
    fn seats_compare_by_value() {
        let seats = parse(team_text()).expect("parse");
        assert_eq!(
            seats[2],
            Seat {
                role: Role::Qa,
                agent: "claude".to_string(),
                model: "opus".to_string(),
                effort: None,
                count: 1,
            }
        );
    }
}

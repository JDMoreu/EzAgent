use super::{card, parse, validate, TeamCheck};
use crate::contract::Seat;
use crate::inventory::{AvailableAgent, Inventory};
use crate::profiles::AgentProfile;
use crate::roles::{builtin, RoleId};

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

fn roles() -> Vec<crate::roles::RoleDef> {
    crate::role_text::builtin_roles()
}

fn role(id: &str) -> RoleId {
    RoleId::parse(id).expect("id del papel")
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
    let seats = parse(team_text(), &roles()).expect("parse");
    assert_eq!(seats.len(), 4);
    assert_eq!(seats[0].role, role(builtin::PLANNER));
    assert_eq!(seats[0].count, 1);
    assert_eq!(seats[1].count, 2);
    assert_eq!(seats[1].effort.as_deref(), Some("max"));
    assert_eq!(seats[2].agent, "claude");
}

#[test]
fn parse_rejects_unknown_role() {
    let error = parse(
        "[[papel]]\npapel = \"elephant\"\nagente = \"a\"\nmodelo = \"m\"\n",
        &roles(),
    )
    .expect_err("unknown role");
    assert!(error.to_string().contains("fila 1"), "{error}");
}

#[test]
fn parse_rejects_bad_count_and_repeated_roles() {
    let zero = parse(
        "[[papel]]\npapel = \"qa\"\nagente = \"a\"\nmodelo = \"m\"\ncantidad = 0\n",
        &roles(),
    )
    .expect_err("zero");
    assert!(zero.to_string().contains("al menos 1"), "{zero}");

    let many_qa = parse(
        "[[papel]]\npapel = \"qa\"\nagente = \"a\"\nmodelo = \"m\"\ncantidad = 2\n",
        &roles(),
    )
    .expect_err("qa count");
    assert!(
        many_qa.to_string().contains("cantidad mayor que 1"),
        "{many_qa}"
    );

    let repeated = parse(
        "[[papel]]\npapel = \"qa\"\nagente = \"a\"\nmodelo = \"m\"\n\n\
         [[papel]]\npapel = \"qa\"\nagente = \"b\"\nmodelo = \"n\"\n",
        &roles(),
    )
    .expect_err("repeated");
    assert!(repeated.to_string().contains("repetido"), "{repeated}");
}

#[test]
fn valid_team_has_no_problems() {
    let seats = parse(team_text(), &roles()).expect("parse");
    let check = validate(&seats, &inventory(), &roles());
    assert_eq!(check, TeamCheck::default(), "{check:?}");
}

#[test]
fn missing_verifier_is_an_error() {
    let seats = parse(
        "[[papel]]\npapel = \"developer\"\nagente = \"opencode\"\nmodelo = \"opencode-go/deepseek-v4.1-flash\"\n",
        &roles(),
    )
    .expect("parse");
    let check = validate(&seats, &inventory(), &roles());
    assert_eq!(check.errors.len(), 1, "{check:?}");
    assert!(check
        .errors
        .iter()
        .any(|error| error.contains("verificador")));
}

#[test]
fn parse_uses_stage_and_delegation_defaults() {
    let seats = parse(team_text(), &roles()).expect("parse");
    assert_eq!(seats[0].stage, 1);
    assert_eq!(seats[1].stage, 3);
    assert!(seats[0].delegates_to.is_empty());
}

#[test]
fn parse_rejects_delegation_from_a_non_delegating_role() {
    let error = parse(
        "[[papel]]\npapel = \"qa\"\nagente = \"claude\"\nmodelo = \"opus\"\ndelega_en = [\"developer\"]\n",
        &roles(),
    )
    .expect_err("no puede delegar");
    assert!(error.to_string().contains("no puede delegar"), "{error}");
}

#[test]
fn unavailable_agent_is_an_error() {
    let mut seats = parse(team_text(), &roles()).expect("parse");
    seats[0].agent = "missing".to_string();
    let check = validate(&seats, &inventory(), &roles());
    assert!(
        check.errors.iter().any(|error| error.contains("missing")),
        "{check:?}"
    );
}

#[test]
fn model_outside_the_list_is_an_error() {
    let mut seats = parse(team_text(), &roles()).expect("parse");
    seats[2].model = "not-listed".to_string();
    let check = validate(&seats, &inventory(), &roles());
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
    let mut seats = parse(team_text(), &roles()).expect("parse");
    seats[2].model = "opencode-go/deepseek-v4.1-flash".to_string();
    let check = validate(&seats, &inventory(), &roles());
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
    let mut seats = parse(team_text(), &roles()).expect("parse");
    seats[3].model = "opencode-go/deepseek-v4.1-flash".to_string();
    let check = validate(&seats, &inventory(), &roles());
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
        &roles(),
    )
    .expect_err("empty effort");
    assert!(
        error.to_string().contains("esfuerzo no puede estar vacío"),
        "{error}"
    );
}

#[test]
fn effort_without_profile_support_is_an_error() {
    let mut seats = parse(team_text(), &roles()).expect("parse");
    seats[0].effort = Some("max".to_string());
    let mut stock = inventory();
    stock.agents[0].profile.env.clear();
    let check = validate(&seats, &stock, &roles());
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
    let mut seats = parse(team_text(), &roles()).expect("parse");
    seats[0].effort = Some("max".to_string());
    let mut stock = inventory();
    stock.agents[0].profile.env.insert(
        "OPENCODE_CONFIG_CONTENT".to_string(),
        r#"{"model":"{modelo}{variante}"}"#.to_string(),
    );
    let check = validate(&seats, &stock, &roles());
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
    let mut seats = parse(team_text(), &roles()).expect("parse");
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
    let check = validate(&seats, &with_other, &roles());
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
    let check = validate(&seats, &only_same_family, &roles());
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
    let mut seats = parse(team_text(), &roles()).expect("parse");
    seats[3].agent = "claude".to_string();
    seats[3].model = "opus".to_string();
    let check = validate(&seats, &inventory(), &roles());
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
    let seats = parse(team_text(), &roles()).expect("parse");
    let check = TeamCheck::default();
    assert_eq!(
        card("Arreglar el HUD", &seats, &check, &inventory(), &roles()),
        "Equipo propuesto para «Arreglar el HUD»:\n\
         - Planificador (planner): OpenCode · opencode-go/deepseek-v4.1-flash · etapa 1\n\
         - Desarrollador (developer): OpenCode · opencode-go/deepseek-v4.1-flash · etapa 3 · esfuerzo max · ×2\n\
         - QA (qa): Claude Code · opus · etapa 4\n\
         - Validador de QA (qa_validator): Codex · opencode-go/gpt-6-luna · etapa 5"
    );
}

#[test]
fn card_lists_only_the_warnings() {
    let seats = parse(team_text(), &roles()).expect("parse");
    let check = TeamCheck {
        errors: vec!["falla".to_string()],
        warnings: vec!["aviso".to_string()],
    };
    let text = card("T", &seats, &check, &inventory(), &roles());
    assert!(text.ends_with("Aviso: aviso"), "{text}");
    assert!(!text.contains("Error"), "{text}");
}

#[test]
fn seats_compare_by_value() {
    let seats = parse(team_text(), &roles()).expect("parse");
    assert_eq!(
        seats[2],
        Seat {
            role: role(builtin::QA),
            agent: "claude".to_string(),
            model: "opus".to_string(),
            effort: None,
            count: 1,
            stage: 4,
            delegates_to: Vec::new(),
        }
    );
}

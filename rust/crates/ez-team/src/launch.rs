//! Launch helpers: command line for a seat, the full instructions of a fish and the
//! one-line first message (design §6).

use std::path::Path;

use crate::contract::{InstructionContext, Seat};
use crate::error::TeamError;
use crate::profiles::AgentProfile;
use crate::roles::Role;

const COMMON: &str = include_str!("roles/_comun.md");

fn template(role: Role) -> &'static str {
    match role {
        Role::Planner => include_str!("roles/planner.md"),
        Role::PlanReviewer => include_str!("roles/plan_reviewer.md"),
        Role::Developer => include_str!("roles/developer.md"),
        Role::Qa => include_str!("roles/qa.md"),
        Role::QaValidator => include_str!("roles/qa_validator.md"),
    }
}

/// Markers the templates may use (design §6). Braces of code blocks are not markers.
const MARKERS: [&str; 11] = [
    "{papel_nombre}",
    "{pez}",
    "{ronda}",
    "{carpeta}",
    "{encargo}",
    "{criterios}",
    "{plan}",
    "{encargo_pez}",
    "{informe}",
    "{informe_qa}",
    "{mutaciones_minimas}",
];

/// Command line of the agent for this seat: `{modelo}` and, when the seat has an effort,
/// `{esfuerzo}` are substituted; `effort_args` is only appended with an effort.
pub fn argv(profile: &AgentProfile, seat: &Seat) -> Vec<String> {
    let mut command: Vec<String> = profile
        .argv
        .iter()
        .map(|argument| fill(argument, &seat.model, seat.effort.as_deref()))
        .collect();
    if let Some(effort) = seat.effort.as_deref().filter(|effort| !effort.is_empty()) {
        command.extend(
            profile
                .effort_args
                .iter()
                .map(|argument| argument.replace("{esfuerzo}", effort)),
        );
    }
    command
}

fn fill(argument: &str, model: &str, effort: Option<&str>) -> String {
    let mut text = argument.replace("{modelo}", model);
    if let Some(effort) = effort.filter(|effort| !effort.is_empty()) {
        text = text.replace("{esfuerzo}", effort);
    }
    text
}

/// Full instructions of a fish: `roles/_comun.md`, a blank line and `roles/<role>.md`
/// with the context markers substituted.
///
/// Fails when the validator has no QA report or when a marker from [`MARKERS`] is left.
pub fn render_instructions(context: &InstructionContext) -> Result<String, TeamError> {
    let qa_report = match (context.role, &context.qa_report) {
        (Role::QaValidator, None) => {
            return Err(TeamError::Invalid(format!(
                "El {} necesita el informe del QA de esta ronda (`qa_report`).",
                Role::QaValidator.label()
            )))
        }
        (_, Some(path)) => path.display().to_string(),
        (_, None) => String::new(),
    };
    let mut text = String::new();
    text.push_str(COMMON.trim_end());
    text.push_str("\n\n");
    text.push_str(template(context.role).trim_start());
    let values = [
        ("{papel_nombre}", context.role.label().to_string()),
        ("{pez}", context.fish.clone()),
        ("{ronda}", context.round.to_string()),
        ("{carpeta}", context.folder.display().to_string()),
        ("{encargo}", context.brief.display().to_string()),
        ("{criterios}", context.criteria.display().to_string()),
        ("{plan}", context.plan.display().to_string()),
        ("{encargo_pez}", context.fish_brief.display().to_string()),
        ("{informe_qa}", qa_report),
        ("{informe}", context.report.display().to_string()),
        ("{mutaciones_minimas}", context.min_mutations.to_string()),
    ];
    for (marker, value) in values {
        text = text.replace(marker, &value);
    }
    if let Some(marker) = MARKERS.iter().find(|marker| text.contains(**marker)) {
        return Err(TeamError::Invalid(format!(
            "La plantilla de {} dejó el marcador {marker} sin sustituir.",
            context.role.label()
        )));
    }
    Ok(text)
}

/// First message sent to a new agent: one line, pointing at the instructions file.
pub fn first_line(role: Role, instructions: &Path) -> String {
    format!(
        "Eres un pez dorado de EzAgent con el papel {}. Lee {} y síguelo al pie de la letra.",
        role.label(),
        instructions.display()
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{argv, first_line, render_instructions};
    use crate::contract::{InstructionContext, Seat};
    use crate::error::TeamError;
    use crate::profiles::AgentProfile;
    use crate::roles::Role;

    fn profile(id: &str, argv: &[&str], effort_args: &[&str]) -> AgentProfile {
        AgentProfile {
            id: id.to_string(),
            name: id.to_string(),
            argv: argv.iter().map(|argument| argument.to_string()).collect(),
            models_cmd: None,
            models: Vec::new(),
            family: None,
            effort_args: effort_args
                .iter()
                .map(|argument| argument.to_string())
                .collect(),
            env: std::collections::BTreeMap::new(),
            prelaunch: None,
        }
    }

    fn seat(effort: Option<&str>) -> Seat {
        Seat {
            role: Role::Developer,
            agent: "codex".to_string(),
            model: "openai/gpt-6-luna".to_string(),
            effort: effort.map(str::to_string),
            count: 1,
        }
    }

    fn context(role: Role) -> InstructionContext {
        InstructionContext {
            role,
            fish: format!("{}-r2", role.short()),
            round: 2,
            folder: PathBuf::from(r"C:\proj"),
            brief: PathBuf::from(r"C:\proj\.ez\tareas\t1\encargo.md"),
            criteria: PathBuf::from(r"C:\proj\.ez\tareas\t1\criterios.toml"),
            plan: PathBuf::from(r"C:\proj\.ez\tareas\t1\plan.md"),
            fish_brief: PathBuf::from(r"C:\proj\.ez\tareas\t1\ronda-2\pez.encargo.md"),
            report: PathBuf::from(r"C:\proj\.ez\tareas\t1\ronda-2\pez.informe.md"),
            qa_report: (role == Role::QaValidator)
                .then(|| PathBuf::from(r"C:\proj\.ez\tareas\t1\ronda-2\qa-r2.informe.md")),
            min_mutations: 3,
        }
    }

    #[test]
    fn argv_with_effort_appends_effort_args() {
        let codex = profile(
            "codex",
            &["codex", "--model", "{modelo}"],
            &["-c", "model_reasoning_effort={esfuerzo}"],
        );
        assert_eq!(
            argv(&codex, &seat(Some("max"))),
            [
                "codex",
                "--model",
                "openai/gpt-6-luna",
                "-c",
                "model_reasoning_effort=max"
            ]
        );
    }

    #[test]
    fn argv_without_effort_omits_effort_args() {
        let codex = profile(
            "codex",
            &["codex", "--model", "{modelo}"],
            &["-c", "model_reasoning_effort={esfuerzo}"],
        );
        assert_eq!(
            argv(&codex, &seat(None)),
            ["codex", "--model", "openai/gpt-6-luna"]
        );
    }

    #[test]
    fn instructions_are_rendered_for_every_role() {
        for role in Role::ALL {
            let text = render_instructions(&context(role)).expect("rendered");
            assert!(text.contains(role.label()), "{role:?}: {text}");
            assert!(text.contains(&format!("{}-r2", role.short())), "{role:?}");
            assert!(
                text.contains(r"C:\proj\.ez\tareas\t1\encargo.md"),
                "{role:?}"
            );
            assert!(text.contains("Criterios de aceptación"), "{role:?}");
            assert!(!text.contains("{pez}"), "{role:?}: {text}");
            assert!(!text.contains("{ronda}"), "{role:?}: {text}");
        }
    }

    #[test]
    fn validator_receives_the_qa_report_and_the_minimum() {
        let text = render_instructions(&context(Role::QaValidator)).expect("rendered");
        assert!(text.contains(r"ronda-2\qa-r2.informe.md"), "{text}");
        assert!(text.contains("al menos 3 fallas"), "{text}");
    }

    #[test]
    fn validator_without_qa_report_is_an_error() {
        let mut context = context(Role::QaValidator);
        context.qa_report = None;
        let error = render_instructions(&context).expect_err("missing report");
        assert!(matches!(error, TeamError::Invalid(_)), "{error}");
        assert!(error.to_string().contains("informe del QA"), "{error}");
    }

    #[test]
    fn first_line_is_one_line_and_points_at_the_instructions() {
        assert_eq!(
            first_line(
                Role::Qa,
                &PathBuf::from(r"C:\proj\.ez\qa-r1.instrucciones.md")
            ),
            r"Eres un pez dorado de EzAgent con el papel QA. Lee C:\proj\.ez\qa-r1.instrucciones.md y síguelo al pie de la letra."
        );
    }
}

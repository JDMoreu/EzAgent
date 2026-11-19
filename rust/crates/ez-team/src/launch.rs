//! Launch helpers: command line for a seat, the full instructions of a fish and the
//! one-line first message (design §6).

use std::collections::BTreeMap;
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

/// Everything the launcher needs to start an agent (design §14.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    /// Argument list; `argv[0]` is the program.
    pub argv: Vec<String>,
    /// Environment variables with the markers already substituted.
    pub env: BTreeMap<String, String>,
    /// Pre-launch work requested by the profile, if any.
    pub prelaunch: Option<String>,
}

/// Builds the launch data for `seat`, substituting `{modelo}`, `{esfuerzo}` and
/// `{variante}` (`#<esfuerzo>`, or empty when there is no effort) in `argv`,
/// `esfuerzo_args` (only present with an effort) and the values of `env`.
pub fn command(profile: &AgentProfile, seat: &Seat) -> LaunchCommand {
    let effort = seat.effort.as_deref().filter(|effort| !effort.is_empty());
    let variant = effort.map(|effort| format!("#{effort}")).unwrap_or_default();
    let mut argv: Vec<String> = profile
        .argv
        .iter()
        .map(|argument| fill(argument, &seat.model, effort, &variant))
        .collect();
    if effort.is_some() {
        argv.extend(
            profile
                .effort_args
                .iter()
                .map(|argument| fill(argument, &seat.model, effort, &variant)),
        );
    }
    let env = profile
        .env
        .iter()
        .map(|(key, value)| (key.clone(), fill(value, &seat.model, effort, &variant)))
        .collect();
    LaunchCommand {
        argv,
        env,
        prelaunch: profile.prelaunch.clone(),
    }
}

fn fill(argument: &str, model: &str, effort: Option<&str>, variant: &str) -> String {
    argument
        .replace("{modelo}", model)
        .replace("{esfuerzo}", effort.unwrap_or(""))
        .replace("{variante}", variant)
}

/// First message sent to the elephant when a space is created (design §14.2).
pub fn elephant_first_line() -> String {
    "Eres el elefante de este espacio de EzAgent. Ejecuta `ez --skill` y sigue ese manual; luego espera el pedido de la persona.".to_string()
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
// DECISION: the role name here is `Role::label()` (the human name); the protocol keeps
// using `Role::id()`. The first line is for the fish, not for `ez fish <papel>`.
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

    use super::{command, elephant_first_line, first_line, render_instructions};
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

    fn opencode_profile() -> AgentProfile {
        let mut agent = profile("opencode", &["opencode", "--standalone"], &[]);
        agent.env.insert(
            "OPENCODE_CONFIG_CONTENT".to_string(),
            r#"{"model":"{modelo}{variante}"}"#.to_string(),
        );
        agent.prelaunch = Some("opencode_state".to_string());
        agent
    }

    #[test]
    fn command_with_effort_appends_effort_args() {
        let codex = profile(
            "codex",
            &["codex", "--model", "{modelo}"],
            &["-c", "model_reasoning_effort={esfuerzo}"],
        );
        let launch = command(&codex, &seat(Some("max")));
        assert_eq!(
            launch.argv,
            [
                "codex",
                "--model",
                "openai/gpt-6-luna",
                "-c",
                "model_reasoning_effort=max"
            ]
        );
        assert!(launch.env.is_empty());
        assert_eq!(launch.prelaunch, None);
    }

    #[test]
    fn command_without_effort_omits_effort_args() {
        let codex = profile(
            "codex",
            &["codex", "--model", "{modelo}"],
            &["-c", "model_reasoning_effort={esfuerzo}"],
        );
        assert_eq!(
            command(&codex, &seat(None)).argv,
            ["codex", "--model", "openai/gpt-6-luna"]
        );
    }

    #[test]
    fn command_for_opencode_with_effort_uses_the_variant() {
        let launch = command(&opencode_profile(), &seat(Some("max")));
        assert_eq!(launch.argv, ["opencode", "--standalone"]);
        assert_eq!(
            launch.env.get("OPENCODE_CONFIG_CONTENT").map(String::as_str),
            Some(r#"{"model":"openai/gpt-6-luna#max"}"#)
        );
        assert_eq!(launch.prelaunch.as_deref(), Some("opencode_state"));
    }

    #[test]
    fn command_for_opencode_without_effort_leaves_the_variant_empty() {
        let launch = command(&opencode_profile(), &seat(None));
        assert_eq!(
            launch.env.get("OPENCODE_CONFIG_CONTENT").map(String::as_str),
            Some(r#"{"model":"openai/gpt-6-luna"}"#)
        );
    }

    #[test]
    fn command_for_claude_appends_effort() {
        let claude = profile("claude", &["claude", "--model", "{modelo}"], &["--effort", "{esfuerzo}"]);
        let launch = command(&claude, &seat(Some("high")));
        assert_eq!(launch.argv, ["claude", "--model", "openai/gpt-6-luna", "--effort", "high"]);
    }

    #[test]
    fn elephant_first_line_is_exact() {
        assert_eq!(
            elephant_first_line(),
            "Eres el elefante de este espacio de EzAgent. Ejecuta `ez --skill` y sigue ese manual; luego espera el pedido de la persona."
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

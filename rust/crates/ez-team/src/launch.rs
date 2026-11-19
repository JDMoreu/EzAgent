//! Ayudas de lanzamiento: línea de mando, instrucciones y primer mensaje (§14.2, §15.9).

use std::collections::BTreeMap;
use std::path::Path;

use crate::contract::InstructionContext;
use crate::error::TeamError;
use crate::profiles::AgentProfile;
use crate::roles::RoleDef;

const COMMON: &str = include_str!("roles/_comun.md");
const DELEGATE: &str = include_str!("roles/_delegar.md");

/// Marcadores que pueden usar las plantillas (§15.7). Las llaves de los bloques
/// de código no son marcadores.
const MARKERS: [&str; 13] = [
    "{papel_nombre}",
    "{id}",
    "{ronda}",
    "{carpeta}",
    "{encargo}",
    "{criterios}",
    "{plan}",
    "{mi_encargo}",
    "{informe}",
    "{informe_qa}",
    "{mutaciones_minimas}",
    "{papeles_delegables}",
    "{borradores}",
];

/// Todo lo que necesita el lanzador para abrir un agente (diseño §14.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchCommand {
    /// Lista de argumentos; `argv[0]` es el programa.
    pub argv: Vec<String>,
    /// Variables de entorno con los marcadores ya sustituidos.
    pub env: BTreeMap<String, String>,
    /// Trabajo previo que pide el perfil, si lo hay.
    pub prelaunch: Option<String>,
}

/// Construye los datos de lanzamiento de `seat`, sustituyendo `{modelo}`,
/// `{esfuerzo}` y `{variante}` en `argv`, `esfuerzo_args` y los valores de `env`.
pub fn command(profile: &AgentProfile, seat: &crate::contract::Seat) -> LaunchCommand {
    let effort = seat.effort.as_deref().filter(|effort| !effort.is_empty());
    let variant = effort
        .map(|effort| format!("#{effort}"))
        .unwrap_or_default();
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

/// Primer mensaje del agente principal al abrir un espacio (§14.2, §15.9).
pub fn main_first_line() -> String {
    "Eres el agente principal de este espacio de EzAgent. Ejecuta ez --skill y sigue ese manual; luego espera el pedido de la persona.".to_string()
}

/// Instrucciones completas de un subagente: común, cuerpo del papel y, si puede
/// delegar, la sección de delegación, con los marcadores sustituidos (§15.7).
///
/// Falla si un papel con `mutations` no tiene informe previo o si queda un marcador.
pub fn render_instructions(
    def: &RoleDef,
    context: &InstructionContext,
) -> Result<String, TeamError> {
    let qa_report = match (def.mutations, &context.qa_report) {
        (true, None) => {
            return Err(TeamError::Invalid(format!(
                "El papel «{}» necesita el informe del verificador que escribe pruebas de esta ronda (`qa_report`).",
                def.name
            )))
        }
        (_, Some(path)) => path.display().to_string(),
        (_, None) => String::new(),
    };
    let mut text = String::new();
    text.push_str(COMMON.trim_end());
    text.push_str("\n\n");
    text.push_str(def.body.trim_start());
    if def.can_delegate {
        text.push_str("\n\n");
        text.push_str(DELEGATE.trim_start());
    }
    let values = [
        ("{papel_nombre}", def.name.clone()),
        ("{id}", context.fish.clone()),
        ("{ronda}", context.round.to_string()),
        ("{carpeta}", context.folder.display().to_string()),
        ("{encargo}", context.brief.display().to_string()),
        ("{criterios}", context.criteria.display().to_string()),
        ("{plan}", context.plan.display().to_string()),
        ("{mi_encargo}", context.fish_brief.display().to_string()),
        ("{informe_qa}", qa_report),
        ("{informe}", context.report.display().to_string()),
        ("{mutaciones_minimas}", context.min_mutations.to_string()),
        (
            "{papeles_delegables}",
            context
                .delegable_roles
                .iter()
                .map(|role| role.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ),
        ("{borradores}", context.drafts.display().to_string()),
    ];
    for (marker, value) in values {
        text = text.replace(marker, &value);
    }
    if let Some(marker) = MARKERS.iter().find(|marker| text.contains(**marker)) {
        return Err(TeamError::Invalid(format!(
            "La plantilla de «{}» dejó el marcador {marker} sin sustituir.",
            def.name
        )));
    }
    Ok(text)
}

/// Primer mensaje de un subagente: una sola línea con el nombre del papel y la
/// ruta de sus instrucciones (§15.9).
pub fn first_line(role_name: &str, instructions: &Path) -> String {
    format!(
        "Eres un subagente de EzAgent con el papel {role_name}. Lee {} y síguelo al pie de la letra.",
        instructions.display()
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{command, first_line, main_first_line, render_instructions};
    use crate::contract::{InstructionContext, Seat};
    use crate::error::TeamError;
    use crate::profiles::AgentProfile;
    use crate::roles::{builtin, RoleId};

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
            role: RoleId::parse(builtin::DEVELOPER).expect("id"),
            agent: "codex".to_string(),
            model: "openai/gpt-6-luna".to_string(),
            effort: effort.map(str::to_string),
            count: 1,
            stage: 3,
            delegates_to: Vec::new(),
        }
    }

    fn context(id: &str, qa: bool) -> InstructionContext {
        InstructionContext {
            role: RoleId::parse(id).expect("id"),
            role_name: id.to_string(),
            fish: format!("{id}-r2"),
            parent: None,
            round: 2,
            folder: PathBuf::from(r"C:\proj"),
            brief: PathBuf::from(r"C:\proj\.ez\tareas\t1\encargo.md"),
            criteria: PathBuf::from(r"C:\proj\.ez\tareas\t1\criterios.toml"),
            plan: PathBuf::from(r"C:\proj\.ez\tareas\t1\plan.md"),
            fish_brief: PathBuf::from(r"C:\proj\.ez\tareas\t1\ronda-2\pez.encargo.md"),
            report: PathBuf::from(r"C:\proj\.ez\tareas\t1\ronda-2\pez.informe.md"),
            qa_report: qa.then(|| PathBuf::from(r"C:\proj\.ez\tareas\t1\ronda-2\qa-r2.informe.md")),
            min_mutations: 3,
            delegable_roles: Vec::new(),
            drafts: PathBuf::from(r"C:\proj\.ez\borrador\pez"),
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
            launch
                .env
                .get("OPENCODE_CONFIG_CONTENT")
                .map(String::as_str),
            Some(r#"{"model":"openai/gpt-6-luna#max"}"#)
        );
        assert_eq!(launch.prelaunch.as_deref(), Some("opencode_state"));
    }

    #[test]
    fn command_for_opencode_without_effort_leaves_the_variant_empty() {
        let launch = command(&opencode_profile(), &seat(None));
        assert_eq!(
            launch
                .env
                .get("OPENCODE_CONFIG_CONTENT")
                .map(String::as_str),
            Some(r#"{"model":"openai/gpt-6-luna"}"#)
        );
    }

    #[test]
    fn command_for_claude_appends_effort() {
        let claude = profile(
            "claude",
            &["claude", "--model", "{modelo}"],
            &["--effort", "{esfuerzo}"],
        );
        let launch = command(&claude, &seat(Some("high")));
        assert_eq!(
            launch.argv,
            ["claude", "--model", "openai/gpt-6-luna", "--effort", "high"]
        );
    }

    #[test]
    fn main_first_line_is_exact() {
        assert_eq!(
            main_first_line(),
            "Eres el agente principal de este espacio de EzAgent. Ejecuta ez --skill y sigue ese manual; luego espera el pedido de la persona."
        );
    }

    #[test]
    fn instructions_are_rendered_for_every_builtin_role() {
        for def in crate::role_text::builtin_roles() {
            let text = render_instructions(&def, &context(def.id.as_str(), def.mutations))
                .expect("rendered");
            assert!(text.contains(&def.name), "{text}");
            assert!(text.contains(r"C:\proj\.ez\tareas\t1\encargo.md"), "{text}");
            assert!(text.contains("Criterios de aceptación"), "{text}");
            assert!(!text.contains("{id}"), "{text}");
            assert!(!text.contains("{ronda}"), "{text}");
        }
    }

    #[test]
    fn validator_receives_the_qa_report_and_the_minimum() {
        let def = crate::role_text::builtin_roles()
            .into_iter()
            .find(|role| role.id.as_str() == builtin::QA_VALIDATOR)
            .expect("validador");
        let text =
            render_instructions(&def, &context(builtin::QA_VALIDATOR, true)).expect("rendered");
        assert!(text.contains(r"ronda-2\qa-r2.informe.md"), "{text}");
        assert!(text.contains("al menos 3 fallas"), "{text}");
    }

    #[test]
    fn validator_without_qa_report_is_an_error() {
        let def = crate::role_text::builtin_roles()
            .into_iter()
            .find(|role| role.id.as_str() == builtin::QA_VALIDATOR)
            .expect("validador");
        let error = render_instructions(&def, &context(builtin::QA_VALIDATOR, false))
            .expect_err("missing report");
        assert!(matches!(error, TeamError::Invalid(_)), "{error}");
        assert!(error.to_string().contains("informe"), "{error}");
    }

    #[test]
    fn first_line_is_one_line_and_points_at_the_instructions() {
        assert_eq!(
            first_line("QA", &PathBuf::from(r"C:\proj\.ez\qa-r1.instrucciones.md")),
            r"Eres un subagente de EzAgent con el papel QA. Lee C:\proj\.ez\qa-r1.instrucciones.md y síguelo al pie de la letra."
        );
    }

    #[test]
    fn delegating_role_includes_the_section_and_the_delegable_roles() {
        let mut def = crate::role_text::builtin_roles()
            .into_iter()
            .find(|role| role.id.as_str() == builtin::DEVELOPER)
            .expect("desarrollador");
        def.can_delegate = true;
        let mut ctx = context(builtin::DEVELOPER, false);
        ctx.delegable_roles = vec![
            RoleId::parse(builtin::QA).expect("id"),
            RoleId::parse(builtin::QA_VALIDATOR).expect("id"),
        ];
        let text = render_instructions(&def, &ctx).expect("rendered");
        assert!(text.contains("Puedes delegar"), "{text}");
        assert!(text.contains("qa, qa_validator"), "{text}");
        assert!(text.contains(r"C:\proj\.ez\borrador\pez"), "{text}");
    }
}

//! Lectura de papeles con cabecera TOML entre líneas `+++` (diseño §15.2).
//!
//! Un papel es un Markdown con `nombre`, `tipo`, `descripcion` y banderas opcionales
//! en la cabecera; el resto del archivo son las instrucciones del papel.

use std::path::Path;

use serde::Deserialize;

use crate::error::TeamError;
use crate::roles::{builtin, RoleDef, RoleId, RoleKind, RoleOrigin};

#[derive(Deserialize)]
struct Header {
    nombre: String,
    tipo: RoleKind,
    descripcion: String,
    #[serde(default)]
    puede_delegar: bool,
    #[serde(default)]
    escribe_pruebas: bool,
    #[serde(default)]
    mutaciones: bool,
    #[serde(default)]
    solo_primera_ronda: bool,
    etapa: Option<u32>,
}

/// Lee un archivo de papel. `text` es su contenido y `path` solo se usa en los errores.
pub fn parse(text: &str, path: &Path, origin: RoleOrigin) -> Result<RoleDef, TeamError> {
    let named = path.display().to_string();
    let mut lines = text.lines();
    lines
        .next()
        .map(str::trim)
        .filter(|line| *line == "+++")
        .ok_or_else(|| invalid(&named, "falta la línea «+++» inicial"))?;
    let mut header = String::new();
    let body = loop {
        let line = lines
            .next()
            .ok_or_else(|| invalid(&named, "falta la línea «+++» que cierra la cabecera"))?;
        if line.trim() == "+++" {
            break lines.collect::<Vec<_>>().join("\n");
        }
        header.push_str(line);
        header.push('\n');
    };
    let raw: Header = toml::from_str(&header)
        .map_err(|error| invalid(&named, &format!("cabecera no válida: {error}")))?;
    let id = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| invalid(&named, "el nombre del archivo no es un id válido"))?;
    let id = RoleId::parse(id).map_err(|error| invalid(&named, &error))?;
    if raw.nombre.trim().is_empty() || raw.descripcion.trim().is_empty() {
        return Err(invalid(
            &named,
            "nombre y descripcion no pueden estar vacíos",
        ));
    }
    if raw.puede_delegar && raw.tipo == RoleKind::Verifier {
        return Err(invalid(
            &named,
            "un verificador no puede delegar (puede_delegar = true)",
        ));
    }
    let default_stage = raw.etapa.unwrap_or(match raw.tipo {
        RoleKind::Executor => 1,
        RoleKind::Verifier => 2,
    });
    if default_stage == 0 {
        return Err(invalid(&named, "etapa debe ser al menos 1"));
    }
    Ok(RoleDef {
        id,
        name: raw.nombre.trim().to_string(),
        kind: raw.tipo,
        description: raw.descripcion.trim().to_string(),
        can_delegate: raw.puede_delegar,
        writes_tests: raw.escribe_pruebas,
        mutations: raw.mutaciones,
        first_round_only: raw.solo_primera_ronda,
        default_stage,
        body,
        origin,
    })
}

/// Los cinco papeles incluidos, en el orden de sus etapas sugeridas.
pub fn builtin_roles() -> Vec<RoleDef> {
    builtin::ALL
        .iter()
        .map(|id| {
            let text: &str = match *id {
                builtin::PLANNER => include_str!("roles/planner.md"),
                builtin::PLAN_REVIEWER => include_str!("roles/plan_reviewer.md"),
                builtin::DEVELOPER => include_str!("roles/developer.md"),
                builtin::QA => include_str!("roles/qa.md"),
                builtin::QA_VALIDATOR => include_str!("roles/qa_validator.md"),
                _ => unreachable!("builtin id"),
            };
            let path = Path::new(id).with_extension("md");
            parse(text, &path, RoleOrigin::Builtin).expect("papel incluido válido")
        })
        .collect()
}

fn invalid(file: &str, reason: &str) -> TeamError {
    TeamError::Invalid(format!("«{file}»: {reason}."))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{builtin_roles, parse};
    use crate::roles::{builtin, RoleKind, RoleOrigin};

    fn write(temp: &std::path::Path, id: &str, content: &str) -> std::path::PathBuf {
        let path = temp.join(format!("{id}.md"));
        std::fs::write(&path, content).expect("archivo de papel");
        path
    }

    #[test]
    fn parses_a_valid_role_with_defaults() {
        let temp = crate::test_support::TempDir::new("role-parse");
        let path = write(
            temp.path(),
            "auditor",
            "+++\nnombre = \"Auditor\"\ntipo = \"verificador\"\ndescripcion = \"Revisa\"\n+++\nCuerpo.\n",
        );
        let def = parse(
            &std::fs::read_to_string(&path).expect("leer"),
            &path,
            RoleOrigin::Task,
        )
        .expect("papel válido");
        assert_eq!(def.id.as_str(), "auditor");
        assert_eq!(def.kind, RoleKind::Verifier);
        assert_eq!(def.default_stage, 2);
        assert!(!def.can_delegate);
        assert!(def.body.contains("Cuerpo."));
    }

    #[test]
    fn rejects_a_missing_field() {
        let temp = crate::test_support::TempDir::new("role-missing");
        let path = write(
            temp.path(),
            "auditor",
            "+++\nnombre = \"Auditor\"\ntipo = \"ejecutor\"\n+++\nCuerpo.\n",
        );
        let error = parse(
            &std::fs::read_to_string(&path).expect("leer"),
            &path,
            RoleOrigin::Library,
        )
        .expect_err("falta descripcion");
        assert!(error.to_string().contains("auditor.md"), "{error}");
        assert!(error.to_string().contains("descripcion"), "{error}");
    }

    #[test]
    fn rejects_an_invalid_kind_and_a_delegating_verifier() {
        let temp = crate::test_support::TempDir::new("role-bad");
        let bad_kind = write(
            temp.path(),
            "auditor",
            "+++\nnombre = \"A\"\ntipo = \"otro\"\ndescripcion = \"D\"\n+++\nCuerpo.\n",
        );
        let error = parse(
            &std::fs::read_to_string(&bad_kind).expect("leer"),
            &bad_kind,
            RoleOrigin::Task,
        )
        .expect_err("tipo inválido");
        assert!(error.to_string().contains("tipo"), "{error}");

        let delegating = write(
            temp.path(),
            "auditor",
            "+++\nnombre = \"A\"\ntipo = \"verificador\"\ndescripcion = \"D\"\npuede_delegar = true\n+++\nCuerpo.\n",
        );
        let error = parse(
            &std::fs::read_to_string(&delegating).expect("leer"),
            &delegating,
            RoleOrigin::Task,
        )
        .expect_err("verificador que delega");
        assert!(error.to_string().contains("no puede delegar"), "{error}");
    }

    #[test]
    fn rejects_an_invalid_id() {
        let temp = crate::test_support::TempDir::new("role-id");
        let path = write(
            temp.path(),
            "Mal_id",
            "+++\nnombre = \"A\"\ntipo = \"ejecutor\"\ndescripcion = \"D\"\n+++\nCuerpo.\n",
        );
        let error = parse(
            &std::fs::read_to_string(&path).expect("leer"),
            &path,
            RoleOrigin::Library,
        )
        .expect_err("id inválido");
        assert!(error.to_string().contains("válido"), "{error}");
    }

    #[test]
    fn builtin_roles_have_their_properties() {
        let roles = builtin_roles();
        assert_eq!(roles.len(), builtin::ALL.len());
        let stages: Vec<u32> = roles.iter().map(|role| role.default_stage).collect();
        assert_eq!(stages, [1, 2, 3, 4, 5]);
        assert!(roles
            .iter()
            .find(|role| role.id.as_str() == builtin::QA)
            .is_some_and(|role| role.writes_tests));
        assert!(roles
            .iter()
            .find(|role| role.id.as_str() == builtin::QA_VALIDATOR)
            .is_some_and(|role| role.mutations));
        assert!(roles
            .iter()
            .find(|role| role.id.as_str() == builtin::PLANNER)
            .is_some_and(|role| role.first_round_only));
        assert!(roles
            .iter()
            .all(|role| role.origin == RoleOrigin::Builtin && !role.body.is_empty()));
    }

    #[test]
    fn parse_accepts_an_inline_document() {
        let def = parse(
            "+++\nnombre = \"A\"\ntipo = \"ejecutor\"\ndescripcion = \"D\"\n+++\nCuerpo.\n",
            Path::new("ayudante.md"),
            RoleOrigin::Builtin,
        )
        .expect("papel");
        assert_eq!(def.default_stage, 1);
    }
}

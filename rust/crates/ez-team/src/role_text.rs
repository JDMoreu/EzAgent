//! Lectura de papeles con cabecera TOML entre líneas `+++` (diseño §15.2).
//!
//! Un papel es un Markdown con `nombre`, `tipo`, `descripcion` y banderas opcionales
//! en la cabecera; el resto del archivo son las instrucciones del papel.

use std::path::{Path, PathBuf};

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

/// `%APPDATA%\EzAgent\hub\papeles`: la biblioteca de papeles de la persona (§15.2).
pub fn library_dir() -> PathBuf {
    ez_core::dirs::app_data_dir().join("hub").join("papeles")
}

/// Papeles disponibles: incluidos, biblioteca y tarea, en ese orden, con el mismo id
/// ganando el de mayor prioridad (tarea > biblioteca > incluido). Los archivos que
/// empiezan por `_` no son papeles. Una carpeta que no existe aporta ningún papel.
///
/// Un archivo inválido es un error que nombra el archivo y el motivo: no se ignora.
pub fn load_roles(task_dir: Option<&Path>, library: &Path) -> Result<Vec<RoleDef>, TeamError> {
    let mut roles = builtin_roles();
    read_into(&mut roles, library, RoleOrigin::Library)?;
    if let Some(dir) = task_dir {
        read_into(&mut roles, dir, RoleOrigin::Task)?;
    }
    roles.sort_by(|left, right| {
        origin_rank(left.origin)
            .cmp(&origin_rank(right.origin))
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(roles)
}

fn read_into(roles: &mut Vec<RoleDef>, dir: &Path, origin: RoleOrigin) -> Result<(), TeamError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(TeamError::io(dir, error)),
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| TeamError::io(dir, error))?.path();
        let is_role = path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| !name.starts_with('_'));
        if is_role {
            paths.push(path);
        }
    }
    // DECISION: se ordenan los archivos para que el resultado y el primer error sean
    // deterministas: `read_dir` no garantiza orden.
    paths.sort();
    for path in paths {
        let text = std::fs::read_to_string(&path).map_err(|error| TeamError::io(&path, error))?;
        let def = parse(&text, &path, origin)?;
        if let Some(existing) = roles.iter_mut().find(|role| role.id == def.id) {
            *existing = def;
        } else {
            roles.push(def);
        }
    }
    Ok(())
}

/// Texto de `ez roles`: una línea por papel, ordenados por origen y luego por id.
pub fn roles_text(roles: &[RoleDef]) -> String {
    sorted_roles(roles)
        .iter()
        .map(|def| {
            format!(
                "- {} · {} · {}{}{}{} ({}): {}",
                def.id,
                def.name,
                kind_label(def.kind),
                if def.can_delegate { " · delega" } else { "" },
                if def.writes_tests {
                    " · escribe pruebas"
                } else {
                    ""
                },
                if def.mutations { " · mutaciones" } else { "" },
                origin_label(def.origin),
                def.description
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Los mismos datos de [`roles_text`], para `ez roles --json`.
// DECISION: las claves JSON van en español, como en `Inventory::json`; el protocolo
// no las fija y la vista las presenta a la persona en español neutro.
pub fn roles_json(roles: &[RoleDef]) -> serde_json::Value {
    serde_json::Value::Array(
        sorted_roles(roles)
            .iter()
            .map(|def| {
                serde_json::json!({
                    "id": def.id.as_str(),
                    "nombre": def.name,
                    "tipo": kind_label(def.kind),
                    "delega": def.can_delegate,
                    "escribe_pruebas": def.writes_tests,
                    "mutaciones": def.mutations,
                    "origen": origin_label(def.origin),
                    "descripcion": def.description,
                })
            })
            .collect(),
    )
}

fn sorted_roles(roles: &[RoleDef]) -> Vec<&RoleDef> {
    let mut sorted: Vec<&RoleDef> = roles.iter().collect();
    sorted.sort_by(|left, right| {
        origin_rank(left.origin)
            .cmp(&origin_rank(right.origin))
            .then_with(|| left.id.cmp(&right.id))
    });
    sorted
}

pub(crate) fn kind_label(kind: RoleKind) -> &'static str {
    match kind {
        RoleKind::Executor => "ejecutor",
        RoleKind::Verifier => "verificador",
    }
}

fn origin_label(origin: RoleOrigin) -> &'static str {
    match origin {
        RoleOrigin::Builtin => "incluido",
        RoleOrigin::Library => "biblioteca",
        RoleOrigin::Task => "tarea",
    }
}

// DECISION: en la lista se muestran primero los incluidos, luego los de la biblioteca
// y al final los nuevos de la tarea, como en `ez --skill` y en la tarjeta del equipo.
// Es el orden de lectura, no el de prioridad (que es el inverso).
fn origin_rank(origin: RoleOrigin) -> u8 {
    match origin {
        RoleOrigin::Builtin => 0,
        RoleOrigin::Library => 1,
        RoleOrigin::Task => 2,
    }
}

fn invalid(file: &str, reason: &str) -> TeamError {
    TeamError::Invalid(format!("«{file}»: {reason}."))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{builtin_roles, library_dir, load_roles, parse, roles_json, roles_text};
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

    fn role_file(dir: &Path, id: &str, name: &str, kind: &str) {
        let content = format!(
            "+++\nnombre = \"{name}\"\ntipo = \"{kind}\"\ndescripcion = \"{name} colabora\"\n+++\nCuerpo.\n"
        );
        write(dir, id, &content);
    }

    fn find<'a>(roles: &'a [crate::roles::RoleDef], id: &str) -> &'a crate::roles::RoleDef {
        roles
            .iter()
            .find(|role| role.id.as_str() == id)
            .unwrap_or_else(|| panic!("papel {id}"))
    }

    #[test]
    fn load_roles_merges_the_three_sources_by_priority() {
        let temp = crate::test_support::TempDir::new("load-roles");
        let task = temp.path().join("task");
        let library = temp.path().join("library");
        std::fs::create_dir_all(&task).expect("carpeta de tarea");
        std::fs::create_dir_all(&library).expect("carpeta de biblioteca");
        role_file(&task, "developer", "Propio", "ejecutor");
        role_file(&task, "auditor", "Auditor", "verificador");
        role_file(&library, "developer", "Biblioteca", "ejecutor");
        role_file(&library, "investigador", "Investigador", "ejecutor");

        let roles = load_roles(Some(&task), &library).expect("papeles");
        assert_eq!(roles.len(), builtin::ALL.len() + 2);
        assert_eq!(find(&roles, "developer").name, "Propio");
        assert_eq!(find(&roles, "developer").origin, RoleOrigin::Task);
        assert_eq!(find(&roles, "investigador").origin, RoleOrigin::Library);
        assert_eq!(find(&roles, "qa").origin, RoleOrigin::Builtin);
        let ids: Vec<&str> = roles.iter().map(|role| role.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "plan_reviewer",
                "planner",
                "qa",
                "qa_validator",
                "investigador",
                "auditor",
                "developer"
            ]
        );
    }

    #[test]
    fn an_invalid_library_role_is_an_error_that_names_the_file() {
        let temp = crate::test_support::TempDir::new("load-invalid");
        write(temp.path(), "roto", "esto no es un papel");
        let error = load_roles(None, temp.path()).expect_err("archivo inválido");
        assert!(error.to_string().contains("roto.md"), "{error}");
    }

    #[test]
    fn underscore_files_are_not_roles() {
        let temp = crate::test_support::TempDir::new("load-underscore");
        write(temp.path(), "_comun", "contenido que no es una cabecera");
        write(temp.path(), "_delegar", "tampoco");
        let roles = load_roles(None, temp.path()).expect("sin error");
        assert_eq!(roles.len(), builtin::ALL.len());
    }

    #[test]
    fn missing_folders_yield_only_the_builtin_roles() {
        let temp = crate::test_support::TempDir::new("load-missing");
        let absent = temp.path().join("no-existe");
        let roles = load_roles(Some(&absent), &absent).expect("carpetas ausentes");
        assert_eq!(roles.len(), builtin::ALL.len());
        assert!(roles.iter().all(|role| role.origin == RoleOrigin::Builtin));
    }

    #[test]
    fn library_dir_points_at_hub_papeles() {
        let path = library_dir();
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("papeles")
        );
        assert_eq!(
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str()),
            Some("hub")
        );
    }

    fn sample_roles() -> Vec<crate::roles::RoleDef> {
        let pairs = [
            (
                "developer",
                RoleOrigin::Builtin,
                "nombre = \"Desarrollador\"\ntipo = \"ejecutor\"\ndescripcion = \"Implementa\"\npuede_delegar = true\n",
            ),
            (
                "auditor",
                RoleOrigin::Library,
                "nombre = \"Auditor\"\ntipo = \"verificador\"\ndescripcion = \"Revisa\"\nescribe_pruebas = true\n",
            ),
            (
                "validador_x",
                RoleOrigin::Task,
                "nombre = \"Validador\"\ntipo = \"verificador\"\ndescripcion = \"Mete fallas\"\nmutaciones = true\n",
            ),
        ];
        pairs
            .iter()
            .map(|(id, origin, header)| {
                let content = format!("+++\n{header}+++\nCuerpo.\n");
                parse(&content, Path::new(&format!("{id}.md")), *origin).expect("papel")
            })
            .collect()
    }

    #[test]
    fn roles_text_matches_the_design_format() {
        let sample = sample_roles();
        assert_eq!(
            roles_text(&sample),
            "- developer · Desarrollador · ejecutor · delega (incluido): Implementa\n\
             - auditor · Auditor · verificador · escribe pruebas (biblioteca): Revisa\n\
             - validador_x · Validador · verificador · mutaciones (tarea): Mete fallas"
        );
        assert_eq!(roles_text(&[]), "");
    }

    #[test]
    fn roles_json_has_the_same_data() {
        let json = roles_json(&sample_roles());
        let values = json.as_array().expect("arreglo");
        assert_eq!(values.len(), 3);
        assert_eq!(values[0]["id"], "developer");
        assert_eq!(values[0]["tipo"], "ejecutor");
        assert_eq!(values[0]["delega"], true);
        assert_eq!(values[0]["origen"], "incluido");
        assert_eq!(values[1]["escribe_pruebas"], true);
        assert_eq!(values[1]["origen"], "biblioteca");
        assert_eq!(values[2]["mutaciones"], true);
        assert_eq!(values[2]["origen"], "tarea");
        assert_eq!(values[2]["descripcion"], "Mete fallas");
    }
}

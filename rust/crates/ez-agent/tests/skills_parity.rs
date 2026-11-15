//! Paridad de `agent/skills.py` contra `parity/fixtures/skills.jsonl` (P-AGENT-A).
//!
//! Reproduce el árbol de skills del oráculo en un directorio temporal y
//! normaliza las rutas a `<root>`, como el generador de fixtures.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ez_agent::skills::{
    ensure_default_skills, hash_folder, skill_tokens, Skill, SkillContent, SkillError,
    SkillLibrary, MAX_CATALOG, MAX_CATALOG_DESCRIPTION_CHARS, MAX_DESCRIPTION_CHARS,
    MAX_NAME_CHARS, MAX_SKILL_CHARS,
};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/skills.jsonl";
fn field<'a>(input: &'a Value, name: &str) -> &'a Value {
    input.get(name).unwrap_or(&Value::Null)
}

fn write_at(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("carpeta de la skill");
    }
    std::fs::write(path, bytes).expect("archivo de la skill");
}

fn write_skill(base: &Path, folder: &str, front: &str, body: &str) {
    write_at(
        &base.join(folder).join("SKILL.md"),
        format!("---\n{front}---\n{body}").as_bytes(),
    );
}

fn write_tree(base: &Path) {
    write_skill(
        base,
        "pdf",
        "name: pdf\ndescription: \"Procesa documentos PDF: extrae texto, une archivos y rellena formularios.\"\n",
        "Guía de la skill.\n\nPaso 1.\nPaso 2.\n",
    );
    write_skill(
        base,
        "web-search",
        "name: web-search\ndescription: >\n  Busca en la web\n  y resume resultados.\n",
        "Abre el navegador y busca.\n",
    );
    write_skill(
        base,
        "no-name",
        "description: Skill sin nombre declarado.\n",
        "Cuerpo de la skill sin name.\n",
    );
    write_skill(
        base,
        "mismatch",
        "name: otro-nombre\ndescription: El name no coincide con la carpeta.\n",
        "x\n",
    );
    write_skill(
        base,
        "BadName",
        "description: Carpeta con nombre inválido.\n",
        "x\n",
    );
    write_skill(
        base,
        "long-desc",
        &format!("name: long-desc\ndescription: {}\n", "d".repeat(1030)),
        "x\n",
    );
    write_skill(
        base,
        "catalog-clip",
        &format!("name: catalog-clip\ndescription: {}\n", "g".repeat(120)),
        "x\n",
    );
    write_skill(
        base,
        "broken-yaml",
        "name: [unclosed\ndescription: roto\n",
        "x\n",
    );
    write_skill(base, "not-a-map", "- uno\n- dos\n", "x\n");
    write_skill(base, "empty-front", "", "x\n");
    write_skill(base, "no-desc", "name: no-desc\n", "x\n");
    write_skill(
        base,
        "_proposed",
        "name: _proposed\ndescription: No revisada.\n",
        "x\n",
    );
    write_skill(
        base,
        ".hidden",
        "name: .hidden\ndescription: Oculta.\n",
        "x\n",
    );
    write_at(
        &base.join("crlf").join("SKILL.md"),
        "---\r\nname: crlf\r\ndescription: Skill editada en Windows.\r\n---\r\nLínea uno.\r\nLínea dos.\r\n"
            .as_bytes(),
    );
    write_at(
        &base.join("no-front").join("SKILL.md"),
        b"# Sin frontmatter\n\nTexto.\n",
    );
    write_at(
        &base.join("bad-utf8").join("SKILL.md"),
        b"---\nname: bad-utf8\n---\n\xff\xfe roto",
    );
    std::fs::create_dir_all(base.join("empty-folder")).expect("carpeta vacía");

    let pdf = base.join("pdf");
    write_at(&pdf.join("scripts/run.py"), b"print('hola')\n");
    write_at(
        &pdf.join("references/guide.md"),
        "# Guía\n\nDetalle.\n".as_bytes(),
    );
    write_at(&pdf.join("assets/logo.txt"), b"logo\n");
    write_at(&pdf.join("assets/binary.dat"), b"\xff\xfe roto");
    write_at(&pdf.join("scripts/big.txt"), "z".repeat(20010).as_bytes());
}

fn write_global(base: &Path) {
    write_skill(
        base,
        "global-one",
        "name: global-one\ndescription: Skill global del perfil.\n",
        "global\n",
    );
    write_skill(
        base,
        "pdf",
        "name: pdf\ndescription: Copia global que debe perder.\n",
        "global\n",
    );
}

struct Context {
    root: PathBuf,
    library: SkillLibrary,
    audit: Arc<Mutex<Vec<String>>>,
}

fn norm_text(text: &str, root: &Path) -> String {
    text.replace(&root.display().to_string(), "<root>")
        .replace('\\', "/")
}

fn norm(value: &Value, root: &Path) -> Value {
    match value {
        Value::String(text) => Value::String(norm_text(text, root)),
        Value::Array(items) => Value::Array(items.iter().map(|item| norm(item, root)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), norm(item, root)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn skill_info(skill: &Skill, root: &Path) -> Value {
    json!({
        "name": skill.name,
        "description": skill.description,
        "body": skill.body,
        "files": skill.files,
        "scripts": skill.scripts,
        "warnings": skill.warnings,
        "declared_name": skill.declared_name,
        "source": skill.source,
        "path": norm_text(&skill.path.display().to_string(), root),
    })
}

fn content_info(content: &SkillContent, root: &Path) -> Value {
    json!({
        "name": content.name,
        "text": content.text,
        "files": content.files,
        "path": norm_text(&content.path, root),
        "file": content.file,
        "truncated": content.truncated,
    })
}

fn error_value(error: SkillError, root: &Path) -> Result<Value, String> {
    Err(norm_text(&error.message, root))
}

fn get_case(context: &Context, name: &Value) -> Value {
    let Some(name) = name.as_str() else {
        return json!({"skill": null});
    };
    json!({
        "skill": context
            .library
            .get(name)
            .map(|skill| skill_info(&skill, &context.root))
    })
}

fn load_case(context: &Context, input: &Value) -> Result<Value, String> {
    let name = field(input, "name").as_str().unwrap_or_default();
    let file = input.get("file").and_then(Value::as_str);
    match context.library.load_skill(name, file) {
        Ok(content) => Ok(json!({"content": content_info(&content, &context.root)})),
        Err(error) => error_value(error, &context.root),
    }
}

fn ensure_case(context: &Context, input: &Value) -> Value {
    let source = field(input, "source")
        .as_str()
        .unwrap_or_default()
        .replace("<root>", &context.root.display().to_string());
    let target = context.root.join("ensure-target");
    if source.contains("bundled") {
        write_tree_bundled(&PathBuf::from(&source));
    }
    let first = ensure_default_skills(&target, Some(Path::new(&source)));
    let second = ensure_default_skills(&target, Some(Path::new(&source)));
    let mut installed: Vec<String> = std::fs::read_dir(&target)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    installed.sort();
    let mut first = first;
    first.sort();
    if !source.contains("bundled") {
        return json!({"names": first});
    }
    let mut second = second;
    second.sort();
    json!({"first": first, "second": second, "installed": installed})
}

fn write_tree_bundled(base: &Path) {
    for (folder, front) in [
        ("alpha", "name: alpha\ndescription: Primera.\n"),
        ("beta", "name: beta\ndescription: Segunda.\n"),
        ("_skip", "name: _skip\ndescription: Oculta.\n"),
        (".hidden", "name: .hidden\ndescription: Oculta.\n"),
    ] {
        write_skill(base, folder, front, "x\n");
    }
    std::fs::create_dir_all(base.join("plain")).expect("carpeta plana");
}

fn max_catalog_case(context: &Context, input: &Value) -> Value {
    let limit = field(input, "max_catalog").as_u64().unwrap_or(0) as usize;
    let local = context.root.join("max-local");
    let library = SkillLibrary::new(&local, &context.root.join("max-global"), None, limit);
    json!({
        "text": library.catalog_text(),
        "warnings": norm(&json!(library.warnings()), &context.root),
    })
}

fn refresh_case(context: &Context) -> Value {
    let local = context.root.join("refresh-local");
    let library = SkillLibrary::new(
        &local,
        &context.root.join("refresh-global"),
        None,
        MAX_CATALOG,
    );
    let before = library.names();
    let unchanged = library.refresh(false);
    write_skill(&local, "dos", "name: dos\ndescription: Segunda.\n", "x\n");
    let changed = library.refresh(false);
    json!({
        "before": before,
        "unchanged": unchanged,
        "changed": changed,
        "after": library.names(),
        "forced": library.refresh(true),
    })
}

fn constants_case() -> Value {
    json!({
        "max_catalog": MAX_CATALOG,
        "max_description_chars": MAX_DESCRIPTION_CHARS,
        "max_catalog_description_chars": MAX_CATALOG_DESCRIPTION_CHARS,
        "max_skill_chars": MAX_SKILL_CHARS,
        "max_name_chars": MAX_NAME_CHARS,
        "name_regex": "^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$",
    })
}

fn run_case(case: &FixtureCase, context: &Context) -> Result<Value, String> {
    let input = &case.input;
    let root = &context.root;
    Ok(match case.function.as_str() {
        "names" => json!({"names": context.library.names()}),
        "catalog_text" => match input.get("max_catalog") {
            Some(_) => max_catalog_case(context, input),
            None => json!({"text": context.library.catalog_text()}),
        },
        "warnings" => json!({
            "warnings": norm(&json!(context.library.warnings()), root),
            "collisions": norm(&json!(context.library.collisions()), root),
            "audit": norm(&json!(*context.audit.lock().expect("auditoría")), root),
        }),
        "keywords" => {
            let mut keywords: Vec<String> = context.library.keywords().iter().cloned().collect();
            keywords.sort();
            json!({"keywords": keywords})
        }
        "all" => json!({
            "skills": context.library.all().iter().map(|skill| skill_info(skill, root)).collect::<Vec<_>>()
        }),
        "get" => get_case(context, field(input, "name")),
        "load_skill" => load_case(context, input)?,
        "folder_hash" => {
            let name = field(input, "name").as_str().unwrap_or_default();
            match context.library.folder_hash(name) {
                Ok(hash) => json!({"hash": hash}),
                Err(error) => return error_value(error, root),
            }
        }
        "hash_folder" => {
            let folder = field(input, "folder")
                .as_str()
                .unwrap_or_default()
                .replace("<root>", &root.display().to_string());
            json!({"hash": hash_folder(Path::new(&folder))})
        }
        "skill_tokens" => {
            let text = field(input, "text").as_str().unwrap_or_default();
            let mut tokens: Vec<String> = skill_tokens(text).into_iter().collect();
            tokens.sort();
            json!({"tokens": tokens})
        }
        "ensure_default_skills" => ensure_case(context, input),
        "refresh" => refresh_case(context),
        "module_constants" => constants_case(),
        other => return Err(format!("función sin portar: {other}")),
    })
}

fn prepare(root: &Path) -> Context {
    let local = root.join("local");
    let global = root.join("global");
    write_tree(&local);
    write_global(&global);
    let audit: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&audit);
    let library = SkillLibrary::new(
        &local,
        &global,
        Some(Box::new(move |message: &str| {
            sink.lock().expect("auditoría").push(message.to_string());
        })),
        MAX_CATALOG,
    );
    // Árboles auxiliares que el oráculo crea antes de sus casos.
    write_tree_bundled(&root.join("bundled"));
    for name in ["alfa", "beta", "gamma", "delta"] {
        write_skill(
            &root.join("max-local"),
            name,
            &format!("name: {name}\ndescription: Skill {name}.\n"),
            "x\n",
        );
    }
    write_skill(
        &root.join("refresh-local"),
        "uno",
        "name: uno\ndescription: Primera.\n",
        "x\n",
    );
    std::fs::create_dir_all(root.join("empty-hash")).expect("carpeta de hash vacío");
    Context {
        root: root.to_path_buf(),
        library,
        audit,
    }
}

#[test]
fn every_skills_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de skills legibles");
    let root = std::env::temp_dir()
        .join(format!(
            "ez-skills-parity-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ))
        .join("skills-cases");
    std::fs::create_dir_all(&root).expect("carpeta temporal");
    let context = prepare(&root);
    let mut failures = Vec::new();
    let mut cases_run = 0;
    for case in &cases {
        let got = run_case(case, &context);
        let ok = match (&got, &case.error) {
            (Ok(value), None) => value == &case.output,
            (Err(message), Some(expected)) => message == expected,
            _ => false,
        };
        if !ok {
            failures.push(format!(
                "{}: {got:?} != {:?} / {:?}",
                case.case, case.output, case.error
            ));
        }
        cases_run += 1;
    }
    assert!(
        failures.is_empty(),
        "{} fallos:\n{}",
        failures.len(),
        failures.join("\n")
    );
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 41, "faltan casos en el fixture");
}

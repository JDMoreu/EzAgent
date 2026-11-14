use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ez-skills-{}-{name}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn write_skill(root: &Path, folder: &str, front: &str, body: &str) {
    let dir = root.join(folder);
    let _ = std::fs::create_dir_all(dir.join("references"));
    let _ = std::fs::write(dir.join("SKILL.md"), format!("---\n{front}\n---\n{body}\n"));
    let _ = std::fs::write(dir.join("references").join("guia.md"), "guía\r\nlarga");
}

#[test]
fn discovers_validates_and_loads() {
    let root = scratch("load");
    let (local, global) = (root.join("local"), root.join("global"));
    write_skill(
        &local,
        "hojas-excel",
        "name: hojas-excel\ndescription: Crea hojas   de cálculo",
        "# Pasos\n1. a",
    );
    write_skill(&global, "hojas-excel", "description: duplicada", "x");
    write_skill(
        &global,
        "Raro",
        "name: otro\ndescription: Una skill rara",
        "y",
    );
    write_skill(&local, "_proposed", "description: oculta", "z");
    write_skill(&local, "sin-desc", "name: sin-desc", "z");
    let library = SkillLibrary::new(&local, &global, None, MAX_CATALOG);
    assert_eq!(
        library.names(),
        vec!["hojas-excel".to_string(), "Raro".to_string()]
    );
    assert_eq!(library.collisions().len(), 1);
    let warnings = library.warnings().join("\n");
    assert!(warnings.contains("falta «description»"));
    assert!(warnings.contains("el name «otro» no coincide con la carpeta «Raro»"));
    assert!(library
        .catalog_text()
        .starts_with("<available_skills>\n- hojas-excel: Crea hojas de cálculo\n"));
    let content = library.load_skill("HOJAS-EXCEL", None).expect("cuerpo");
    assert_eq!(content.text, "# Pasos\n1. a");
    assert_eq!(content.files, vec!["references/guia.md".to_string()]);
    let file = library
        .load_skill("hojas-excel", Some("references\\guia.md"))
        .expect("archivo");
    assert_eq!(file.text, "guía\nlarga");
    assert_eq!(file.file.as_deref(), Some("references/guia.md"));
    assert_eq!(
        library
            .load_skill("hojas-excel", Some("../x"))
            .unwrap_err()
            .message,
        "La ruta del archivo no puede contener «..»."
    );
    assert_eq!(
        library.load_skill("nada", None).unwrap_err().kind,
        SkillErrorKind::NotFound
    );
    assert!(library.keywords().contains("hoja"));
    assert!(!library.refresh(false));
    assert_eq!(library.folder_hash("hojas-excel").expect("hash").len(), 64);
    for dir in [&local, &global] {
        let _ = std::fs::remove_file(dir.join("hojas-excel").join("references").join("guia.md"));
    }
}

#[test]
fn tokens_are_folded_and_singular() {
    let tokens = skill_tokens("Páginas PDF, página");
    assert!(tokens.contains("pagina") && tokens.contains("pdf"));
    assert_eq!(tokens.len(), 2);
}

#[test]
fn splits_frontmatter() {
    assert_eq!(
        split_frontmatter("---\na: 1\n---\ncuerpo"),
        Some(("a: 1\n".into(), "cuerpo".into()))
    );
    assert_eq!(split_frontmatter("hola"), None);
    assert_eq!(split_frontmatter("---\na: 1\n"), None);
}

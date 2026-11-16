use super::*;

fn root() -> PathBuf {
    std::env::temp_dir().join("ezagent-paths-unit")
}

fn env() -> Env {
    let home = root().display().to_string();
    Env::new(&[("HOME", &home), ("USERPROFILE", &home), ("APPDATA", &home)])
}

fn resolve(raw: &str) -> Result<PathBuf, PathDenied> {
    let workspace = root().join("ws");
    resolve_safe_with(
        &env(),
        raw,
        &workspace,
        &[workspace.display().to_string()],
        &[workspace.join("secret").display().to_string()],
        &["**/.env", "**/*.pem"],
    )
}

#[test]
fn relative_paths_land_in_the_workspace() {
    let path = resolve("sub/../a.txt").expect("ruta válida");
    assert_eq!(path, root().join("ws").join("a.txt"));
}

#[test]
fn reserved_names_are_denied() {
    assert_eq!(
        resolve("sub/com1.log").map_err(|error| error.message().to_string()),
        Err("nombre reservado de Windows".to_string())
    );
    assert!(resolve("sub/COM0.txt").is_ok());
}

#[test]
fn deny_globs_protect_secrets() {
    assert_eq!(
        resolve(".env").map_err(|error| error.message().to_string()),
        Err("archivo protegido".to_string())
    );
    assert!(resolve("notes.txt").is_ok());
}

#[test]
fn sibling_prefixes_empty_roots_streams_and_devices_are_denied() {
    let workspace = root().join("ws");
    let base = workspace.display().to_string();
    let denied = |raw: &str, roots: &[String], deny: &[String]| {
        resolve_safe_with(&env(), raw, &workspace, roots, deny, &[] as &[&str])
            .map_err(|error| error.message().to_string())
    };
    let roots = vec![base.clone()];
    assert_eq!(
        denied(
            &root().join("work-evil").join("x.txt").display().to_string(),
            &roots,
            &[]
        ),
        Err("fuera de las carpetas permitidas".to_string())
    );
    assert_eq!(
        resolve("doc.txt:Zone.Identifier").map_err(|error| error.message().to_string()),
        Err("flujos de datos alternativos no permitidos".to_string())
    );
    assert_eq!(
        resolve("\\\\?\\C:\\x").map_err(|error| error.message().to_string()),
        Err("rutas UNC o de dispositivo no permitidas".to_string())
    );
    for name in ["CON", "nul", "COM1", "lpt9", "aux.txt", "PRN.log"] {
        assert_eq!(
            resolve(name).map_err(|error| error.message().to_string()),
            Err("nombre reservado de Windows".to_string()),
            "{name}"
        );
    }
    assert_eq!(
        denied("x", &[] as &[String], &[]),
        Err("fuera de las carpetas permitidas".to_string())
    );

    let secret = workspace.join("secreto");
    let deny = vec![secret.display().to_string()];
    let dotted = workspace.join("otra").join("..").join("secreto").join("x");
    assert_eq!(
        denied(&dotted.display().to_string(), &roots, &deny),
        Err("carpeta protegida".to_string())
    );
    let sibling = workspace.join("secreto2").join("x.txt");
    assert!(denied(&sibling.display().to_string(), &roots, &deny).is_ok());
}

#[test]
fn a_symlink_cannot_escape_the_roots() {
    let workspace = root().join("ws");
    let outside = root().join("outside");
    let _ = std::fs::create_dir_all(&workspace);
    let _ = std::fs::create_dir_all(&outside);
    let link = workspace.join("enlace");
    let _ = std::fs::remove_dir(&link);
    #[cfg(windows)]
    let created = std::os::windows::fs::symlink_dir(&outside, &link);
    #[cfg(unix)]
    let created = std::os::unix::fs::symlink(&outside, &link);
    if created.is_err() {
        // Windows sin modo desarrollador no permite crear enlaces (como el skip de Python).
        return;
    }
    let raw = link.join("secreto.txt");
    let error = resolve_safe_with(
        &env(),
        &raw.display().to_string(),
        &workspace,
        &[workspace.display().to_string()],
        &[] as &[&str],
        &[] as &[&str],
    )
    .expect_err("el enlace sale de la raíz");
    assert_eq!(error.message(), "fuera de las carpetas permitidas");
    let _ = std::fs::remove_dir(&link);
}

#[test]
fn every_denied_message_matches_python() {
    let text = |raw: &str| {
        resolve(raw)
            .map(|path| path.display().to_string())
            .map_err(|error| error.message().to_string())
    };
    let outside = root().join("fuera.txt").display().to_string();
    assert_eq!(text("").unwrap_err(), "ruta vacía o inválida");
    assert_eq!(
        text("//server/share/x").unwrap_err(),
        "rutas UNC o de dispositivo no permitidas"
    );
    assert_eq!(
        text("notas.txt:oculto").unwrap_err(),
        "flujos de datos alternativos no permitidos"
    );
    assert_eq!(
        text(&outside).unwrap_err(),
        "fuera de las carpetas permitidas"
    );
    assert_eq!(text("secret/x.txt").unwrap_err(), "carpeta protegida");
    let anchor = resolve_safe_with(
        &env(),
        "/",
        &root().join("ws"),
        &["/"],
        &[] as &[&str],
        &[] as &[&str],
    )
    .map_err(|error| error.message().to_string());
    assert_eq!(anchor, Err("raíz de unidad no permitida".to_string()));
}

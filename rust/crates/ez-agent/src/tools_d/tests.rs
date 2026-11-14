use super::*;

fn config(dir: &Path, exe: &str) -> AgentConfig {
    let file = dir.join("permissions.toml");
    let _ = std::fs::write(
        &file,
        format!(
            "[tools_d]\nallowed_executables = ['{exe}']\n[paths]\nread_roots = ['{}']\n",
            dir.display().to_string().replace('\\', "/")
        ),
    );
    let (mut config, _) = AgentConfig::load(Some(&file));
    config.workspace = dir.to_path_buf();
    config
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ez-toolsd-{}-{name}",
        uuid::Uuid::new_v4().simple()
    ));
    let _ = std::fs::create_dir_all(dir.join("tools.d"));
    dir
}

#[test]
fn loads_valid_tools_and_rejects_bad_ones() {
    let dir = scratch("load");
    let tools = dir.join("tools.d");
    let echo = if cfg!(windows) { "ping" } else { "echo" };
    let _ = std::fs::write(tools.join("a.toml"), format!(
        "name='saludar'\ndescription='Saluda'\ncommand=['{echo}', '{{texto}}']\nlevel=0\n[params.texto]\ntype='string'\nvalidate='enum:hola,adios'\n[params.n]\ntype='integer'\nrequired=false\nminimum=1\n"));
    let _ = std::fs::write(
        tools.join("b.toml"),
        "name='malo'\ndescription='x'\ncommand='echo hola'\n",
    );
    let _ = std::fs::write(
        tools.join("c.toml"),
        "name='libre'\ndescription='x'\ncommand=['powershell', 'x']\n",
    );
    let config = config(&dir, echo);
    let loaded = load_tools_d(&config);
    assert_eq!(loaded.len(), 1);
    let tool = &loaded[0];
    assert_eq!(tool.level, 1, "el nivel mínimo es 1");
    assert_eq!(tool.timeout_s, 11.0);
    assert_eq!(tool.parameters["required"], json!(["texto"]));
    assert_eq!(
        tool.parameters["properties"]["texto"]["enum"],
        json!(["hola", "adios"])
    );
    assert_eq!(tool.parameters["properties"]["n"]["minimum"], 1);
}

#[test]
fn validates_arguments_before_running() {
    let dir = scratch("validate");
    let config = config(&dir, "ping");
    let ctx = RunContext {
        tool_name: "t".into(),
        command: vec!["ping".into(), "{u}".into()],
        params: IndexMap::from([
            (
                "u".to_string(),
                ParamSpec {
                    kind: "string".into(),
                    required: true,
                    validate: Some("url_http".into()),
                    choices: None,
                    minimum: None,
                    maximum: None,
                },
            ),
            (
                "n".to_string(),
                ParamSpec {
                    kind: "integer".into(),
                    required: false,
                    validate: None,
                    choices: None,
                    minimum: Some(json!(1)),
                    maximum: Some(json!(5)),
                },
            ),
        ]),
        workspace: config.workspace.clone(),
        read_roots: vec![],
        write_roots: vec![],
        deny: vec![],
        deny_globs: vec![],
        timeout_s: 2.0,
    };
    let run = |args: Value| run_declarative_tool(&ctx, args.as_object().expect("objeto"));
    assert_eq!(
        run(json!({})).error.as_deref(),
        Some("Falta el parámetro obligatorio «u».")
    );
    assert_eq!(
        run(json!({"u": "x", "z": 1})).error.as_deref(),
        Some("Parámetros no admitidos: z")
    );
    assert_eq!(
        run(json!({"u": "ftp://a"})).error.as_deref(),
        Some("t: «u» debe ser una URL http o https válida.")
    );
    assert_eq!(
        run(json!({"u": "http://a:b@c/"})).error.as_deref(),
        Some("t: «u» no puede incluir credenciales en la URL.")
    );
    assert_eq!(
        run(json!({"u": "http://a/", "n": "9"})).error.as_deref(),
        Some("t: «n» debe ser menor o igual que 5.")
    );
    assert_eq!(
        run(json!({"u": "http://a/", "n": true})).error.as_deref(),
        Some("t: «n» debe ser un entero.")
    );
}

#[test]
fn captures_output_and_times_out() {
    let dir = std::env::temp_dir();
    let env = clean_environment();
    let (quick, slow): (Vec<String>, Vec<String>) = if cfg!(windows) {
        (
            ["ping", "-n", "1", "127.0.0.1"].map(String::from).to_vec(),
            ["ping", "-n", "8", "127.0.0.1"].map(String::from).to_vec(),
        )
    } else {
        (
            ["echo", "127.0.0.1"].map(String::from).to_vec(),
            ["sleep", "3"].map(String::from).to_vec(),
        )
    };
    let out = run_captured(&quick, &dir, &env, 10.0).expect("termina");
    assert_eq!(out.returncode, 0);
    assert!(out.stdout.contains("127.0.0.1"));
    assert!(matches!(
        run_captured(&slow, &dir, &env, 0.3),
        Err(RunError::Timeout { .. })
    ));
    let missing = ["no-existe-ez".to_string()];
    assert!(matches!(
        run_captured(&missing, &dir, &env, 1.0),
        Err(RunError::Os(_))
    ));
}

//! Pruebas del adaptador con dobles del árbitro.

use super::*;

#[derive(Default)]
struct FakeArbiter {
    calls: Mutex<Vec<(ArbiterCaller, Command)>>,
    event: Mutex<Option<ez_arbiter::ElephantEvent>>,
}

impl ArbiterApi for FakeArbiter {
    fn execute(
        &self,
        caller: &ArbiterCaller,
        command: Command,
    ) -> Result<ez_arbiter::Outcome, ez_arbiter::Rejection> {
        self.calls.lock().push((caller.clone(), command));
        Ok(ez_arbiter::Outcome::new("hecho"))
    }

    fn wait(&self, _space: &str, _timeout: Duration) -> Option<ez_arbiter::ElephantEvent> {
        self.event.lock().clone()
    }

    fn snapshot(&self, _space: &str) -> Option<Value> {
        Some(json!({ "title": "Tarea de prueba" }))
    }

    fn worktree(&self, _space: &str, _elephant: &str) -> Option<PathBuf> {
        None
    }

    fn card_action(
        &self,
        _card: &str,
        _action: &str,
        _comment: Option<&str>,
    ) -> Result<ez_arbiter::Outcome, ez_arbiter::Rejection> {
        Ok(ez_arbiter::Outcome::new("hecho"))
    }
}

fn handler(api: Arc<FakeArbiter>) -> (ArbiterHandler, crate::testutil::TempDir) {
    let dir = crate::testutil::TempDir::new("runtime-handler");
    let brief = dir.path().join("encargo.md");
    let criteria = dir.path().join("criterios.toml");
    std::fs::write(&brief, "encargo").expect("brief");
    std::fs::write(&criteria, "[[criterio]]\n").expect("criteria");
    let h = ArbiterHandler::new(
        api,
        vec![profile()],
        vec![Subscription {
            name: "Todo".to_string(),
            agents: vec!["opencode".to_string()],
        }],
        vec![space_config_in(dir.path().to_path_buf())],
    );
    (h, dir)
}

fn elephant_call() -> Caller {
    Caller::Elephant {
        space: "s1".to_string(),
        elephant: "e00000000".to_string(),
    }
}

fn fish_call() -> Caller {
    Caller::Fish {
        space: "s1".to_string(),
        task: "t1".to_string(),
        elephant: "e00000000".to_string(),
        fish: "dev-r1-1".to_string(),
        attempt: 1,
        parent: None,
    }
}

fn call(h: &ArbiterHandler, caller: &Caller, command: &str, args: Value) -> Reply {
    h.handle(
        caller,
        &Request {
            command: command.to_string(),
            args,
        },
    )
}

#[test]
fn handler_translates_elephant_commands() {
    let api = Arc::new(FakeArbiter::default());
    let (h, dir) = handler(Arc::clone(&api));
    let file = dir.path().join("archivo.md").to_string_lossy().into_owned();
    for command in [
        "status",
        "round.start",
        "verify",
        "round.close",
        "task.close",
    ] {
        assert!(
            call(&h, &elephant_call(), command, json!({})).ok,
            "{command}"
        );
    }
    assert!(
        call(
            &h,
            &elephant_call(),
            "task.new",
            json!({ "title": "Tarea", "brief": file, "criteria": file })
        )
        .ok
    );
    assert!(call(&h, &elephant_call(), "pause", json!({ "reason": "espera" })).ok);
    let calls = api.calls.lock();
    assert_eq!(calls.len(), 7);
    assert!(calls.iter().all(|(caller, _)| caller
        == &ArbiterCaller::Elephant(ElephantKey {
            space: "s1".to_string(),
            elephant: "e00000000".to_string(),
        })));
}

#[test]
fn handler_translates_fish_commands_and_the_fish_caller() {
    let api = Arc::new(FakeArbiter::default());
    let (h, dir) = handler(Arc::clone(&api));
    let file = dir.path().join("archivo.md").to_string_lossy().into_owned();
    assert!(
        call(
            &h,
            &elephant_call(),
            "agent.delegate",
            json!({ "role": "developer", "brief": file })
        )
        .ok
    );
    assert!(
        call(
            &h,
            &elephant_call(),
            "fish.send",
            json!({ "fish": "dev-r1-1", "file": file })
        )
        .ok
    );
    assert!(
        call(
            &h,
            &fish_call(),
            "report",
            json!({ "verdict": "pass", "file": file, "blocking": 0, "major": 0, "minor": 0 })
        )
        .ok
    );
    let calls = api.calls.lock();
    assert_eq!(calls.len(), 3);
    let Command::FishNew { role, .. } = &calls[0].1 else {
        panic!("el primer comando debe ser FishNew");
    };
    assert_eq!(role.as_str(), builtin::DEVELOPER);
    assert_eq!(
        calls[2].0,
        ArbiterCaller::Fish(FishKey {
            space: "s1".to_string(),
            task: "t1".to_string(),
            elephant: "e00000000".to_string(),
            fish: "dev-r1-1".to_string(),
            attempt: 1,
            parent: None,
        })
    );
    assert!(matches!(calls[2].1, Command::Report { .. }));
}

#[test]
fn handler_wait_without_and_with_an_event() {
    let api = Arc::new(FakeArbiter::default());
    let (h, _) = handler(Arc::clone(&api));
    let reply = call(&h, &elephant_call(), "wait", json!({ "timeout": 1 }));
    assert!(reply.ok);
    assert_eq!(reply.message, "Sin novedades; vuelve a ejecutar `ez wait`.");
    assert_eq!(reply.data, json!({ "event": null }));

    *api.event.lock() = Some(ez_arbiter::ElephantEvent::new(
        "fish_reported",
        "s1",
        1.0,
        "El pez informó.",
        json!({}),
    ));
    let reply = call(&h, &elephant_call(), "wait", json!({}));
    assert!(reply.ok);
    assert_eq!(reply.message, "El pez informó.");
    assert!(reply
        .data
        .get("event")
        .is_some_and(|value| !value.is_null()));
}

#[test]
fn handler_rejects_unknown_commands_and_bad_arguments() {
    let api = Arc::new(FakeArbiter::default());
    let (h, _) = handler(api);
    assert!(!call(&h, &elephant_call(), "otro", json!({})).ok);
    assert!(!call(&h, &elephant_call(), "task.new", json!({ "title": "" })).ok);
    assert!(
        !call(
            &h,
            &fish_call(),
            "report",
            json!({
                "verdict": "mal", "file": "x", "blocking": 0, "major": 0, "minor": 0
            })
        )
        .ok
    );
}

#[test]
fn handler_team_propose_rejects_an_invalid_team() {
    let api = Arc::new(FakeArbiter::default());
    let (h, dir) = handler(Arc::clone(&api));
    let file = dir.path().join("equipo.toml");
    std::fs::write(
        &file,
        "[[papel]]\npapel = \"elephant\"\nagente = \"a\"\nmodelo = \"m\"\n",
    )
    .expect("equipo");
    let reply = call(
        &h,
        &elephant_call(),
        "team.propose",
        json!({ "file": file.to_string_lossy() }),
    );
    assert!(!reply.ok, "{reply:?}");
    assert!(api.calls.lock().is_empty());
}

#[test]
fn handler_inventory_returns_text_and_json() {
    let api = Arc::new(FakeArbiter::default());
    let (h, _) = handler(api);
    let reply = call(&h, &elephant_call(), "inventory", json!({}));
    assert!(reply.ok);
    assert!(reply.data.is_array());
}

fn dummy_profile(dir: &crate::testutil::TempDir, id: &str, model: &str) -> AgentProfile {
    let program = dir.path().join(id);
    std::fs::write(&program, "doble").expect("programa falso");
    AgentProfile {
        id: id.to_string(),
        name: id.to_string(),
        argv: vec![program.to_string_lossy().into_owned()],
        models_cmd: None,
        models: vec![model.to_string()],
        family: None,
        effort_args: Vec::new(),
        env: BTreeMap::new(),
        prelaunch: None,
    }
}

#[test]
fn handler_team_propose_accepts_a_valid_team() {
    let dir = crate::testutil::TempDir::new("runtime-team-valid");
    let team = dir.path().join("equipo.toml");
    std::fs::write(
        &team,
        r#"[[papel]]
papel = "planner"
agente = "opencode"
modelo = "opencode-go/deepseek-v4.1-flash"

[[papel]]
papel = "developer"
agente = "claude"
modelo = "opus"

[[papel]]
papel = "qa"
agente = "codex"
modelo = "gpt-6"

[[papel]]
papel = "qa_validator"
agente = "opencode"
modelo = "qwen4-max"
"#,
    )
    .expect("equipo");
    let mut opencode = dummy_profile(&dir, "opencode", "opencode-go/deepseek-v4.1-flash");
    opencode.models.push("qwen4-max".to_string());
    let profiles = vec![
        opencode,
        dummy_profile(&dir, "claude", "opus"),
        dummy_profile(&dir, "codex", "gpt-6"),
    ];
    let api = Arc::new(FakeArbiter::default());
    let h = ArbiterHandler::new(
        Arc::clone(&api) as Arc<dyn ArbiterApi>,
        profiles,
        vec![Subscription {
            name: "Todo".to_string(),
            agents: vec![
                "opencode".to_string(),
                "claude".to_string(),
                "codex".to_string(),
            ],
        }],
        vec![space_config_in(dir.path().to_path_buf())],
    );
    let reply = call(
        &h,
        &elephant_call(),
        "team.propose",
        json!({ "file": team.to_string_lossy() }),
    );
    assert!(reply.ok, "{reply:?}");
    assert!(matches!(api.calls.lock()[0].1, Command::TeamPropose { .. }));
}

#[test]
fn handler_lists_the_builtin_roles() {
    let api = Arc::new(FakeArbiter::default());
    let (h, _dir) = handler(api);
    let reply = call(&h, &elephant_call(), "roles", json!({}));
    assert!(reply.ok, "{reply:?}");
    let lines: Vec<&str> = reply.message.lines().collect();
    assert_eq!(lines.len(), 5, "{}", reply.message);
    for line in [
        "- planner · Planificador · ejecutor",
        "- developer · Desarrollador · ejecutor",
        "- qa · QA · verificador",
        "- qa_validator · Validador de QA · verificador",
        "- plan_reviewer · Revisor del plan · verificador",
    ] {
        assert!(
            lines.iter().any(|actual| actual.starts_with(line)),
            "falta {line} en {}",
            reply.message
        );
    }
    assert!(lines.iter().all(|line| line.contains("(incluido):")));
}

fn real_arbiter(dir: &crate::testutil::TempDir) -> Arc<ez_arbiter::Arbiter> {
    let mut command = std::process::Command::new("git");
    command.args(["init"]).current_dir(dir.path());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let status = command.status().expect("git init");
    assert!(status.success());
    let tokens = Arc::new(Tokens::new());
    let terminals = FakeTerminals::with_tokens(Arc::clone(&tokens));
    let launcher = launcher(terminals, tokens);
    let arbiter = ez_arbiter::Arbiter::new(
        dir.path().to_path_buf(),
        launcher as Arc<dyn ez_arbiter::FishLauncher>,
        Arc::new(crate::runtime::launcher::TeamInstructionWriter),
        Arc::new(ez_arbiter::SystemClock),
    )
    .expect("árbitro");
    arbiter.register_space(ez_arbiter::Space {
        id: "s1".to_string(),
        name: "Ez".to_string(),
        folder: dir.path().to_path_buf(),
    });
    Arc::new(arbiter)
}

#[test]
fn a_fish_delegating_sends_agent_delegate_with_its_full_key() {
    let api = Arc::new(FakeArbiter::default());
    let (h, dir) = handler(Arc::clone(&api));
    let brief = dir.path().join("encargo.md").to_string_lossy().into_owned();
    let mut fish = fish_call();
    if let Caller::Fish { parent, .. } = &mut fish {
        *parent = Some("dev-r1-1".to_string());
    }
    assert!(call(&h, &fish, "agent.delegate", json!({ "role": "qa", "brief": brief })).ok);
    let calls = api.calls.lock();
    assert!(matches!(&calls[0].1, Command::AgentDelegate { .. }), "{:?}", calls[0]);
    assert!(matches!(
        &calls[0].0,
        ArbiterCaller::Fish(key) if key.parent.as_deref() == Some("dev-r1-1")
    ));
}

#[test]
fn roles_include_one_from_the_task_draft_folder() {
    let api = Arc::new(FakeArbiter::default());
    let (h, dir) = handler(api);
    let drafts = dir.path().join(".ez").join("borrador").join("papeles");
    std::fs::create_dir_all(&drafts).expect("carpeta de borradores");
    std::fs::write(
        drafts.join("security_auditor.md"),
        "+++\nnombre = \"Auditor de seguridad\"\ntipo = \"verificador\"\ndescripcion = \"Busca fallos\"\n+++\nCuerpo.\n",
    )
    .expect("papel de la tarea");
    let reply = call(&h, &elephant_call(), "roles", json!({}));
    assert!(reply.ok, "{reply:?}");
    assert!(
        reply.message.contains("security_auditor · Auditor de seguridad"),
        "{}",
        reply.message
    );
    assert!(reply.message.contains("(tarea):"), "{}", reply.message);
}

#[test]
fn team_propose_sends_the_definition_of_a_task_role() {
    let dir = crate::testutil::TempDir::new("runtime-team-task-role");
    let drafts = dir.path().join(".ez").join("borrador").join("papeles");
    std::fs::create_dir_all(&drafts).expect("borradores");
    std::fs::write(
        drafts.join("docs_writer.md"),
        "+++\nnombre = \"Escritor de documentos\"\ntipo = \"ejecutor\"\ndescripcion = \"Escribe la documentación\"\n+++\nCuerpo.\n",
    )
    .expect("papel propio");
    let team = dir.path().join("equipo.toml");
    std::fs::write(
        &team,
        r#"[[papel]]
papel = "docs_writer"
agente = "opencode"
modelo = "opencode-go/deepseek-v4.1-flash"

[[papel]]
papel = "qa"
agente = "codex"
modelo = "gpt-6"
"#,
    )
    .expect("equipo");
    let profiles = vec![
        dummy_profile(&dir, "opencode", "opencode-go/deepseek-v4.1-flash"),
        dummy_profile(&dir, "codex", "gpt-6"),
    ];
    let api = Arc::new(FakeArbiter::default());
    let h = ArbiterHandler::new(
        Arc::clone(&api) as Arc<dyn ArbiterApi>,
        profiles,
        vec![Subscription {
            name: "Todo".to_string(),
            agents: vec!["opencode".to_string(), "codex".to_string()],
        }],
        vec![space_config_in(dir.path().to_path_buf())],
    );
    let reply = call(
        &h,
        &elephant_call(),
        "team.propose",
        json!({ "file": team.to_string_lossy() }),
    );
    assert!(reply.ok, "{reply:?}");
    let calls = api.calls.lock();
    let Command::TeamPropose { roles, seats, .. } = &calls[0].1 else {
        panic!("se esperaba TeamPropose");
    };
    assert!(seats.iter().any(|seat| seat.role.as_str() == "docs_writer"));
    assert!(roles
        .iter()
        .any(|role| role.id.as_str() == "docs_writer" && role.name == "Escritor de documentos"));
}

#[test]
fn inventory_completes_the_codex_models_with_the_discovery() {
    let dir = crate::testutil::TempDir::new("runtime-codex-models");
    let mut codex = dummy_profile(&dir, "codex", "");
    codex.models.clear();
    let api = Arc::new(FakeArbiter::default());
    let h = ArbiterHandler::new(
        api,
        vec![codex],
        vec![Subscription {
            name: "Todo".to_string(),
            agents: vec!["codex".to_string()],
        }],
        vec![space_config_in(dir.path().to_path_buf())],
    )
    .with_codex_models(|| vec!["gpt-6-nuevo".to_string()]);
    let reply = call(&h, &elephant_call(), "inventory", json!({}));
    assert!(reply.ok, "{reply:?}");
    assert!(reply.message.contains("gpt-6-nuevo"), "{}", reply.message);
}

#[test]
fn handler_with_the_real_arbiter_creates_a_task_and_wakes_the_elephant() {
    let dir = crate::testutil::TempDir::new("runtime-real-arbiter");
    let brief = dir.path().join("encargo.md");
    let criteria = dir.path().join("criterios.toml");
    std::fs::write(&brief, "Haz la tarea.").expect("brief");
    std::fs::write(
        &criteria,
        "[[criterio]]\nid = \"C1\"\ndescripcion = \"Verdad\"\ncomando = [\"hostname\"]\n",
    )
    .expect("criterios");
    let arbiter = real_arbiter(&dir);
    let h = ArbiterHandler::new(
        Arc::clone(&arbiter) as Arc<dyn ArbiterApi>,
        vec![profile()],
        vec![Subscription {
            name: "Todo".to_string(),
            agents: vec!["opencode".to_string()],
        }],
        vec![space_config()],
    );
    let created = call(
        &h,
        &elephant_call(),
        "task.new",
        json!({
            "title": "Tarea real",
            "brief": brief.to_string_lossy(),
            "criteria": criteria.to_string_lossy(),
        }),
    );
    assert!(created.ok, "{created:?}");
    assert!(created.data.get("task").is_some());
    let status = call(&h, &elephant_call(), "status", json!({}));
    assert!(status.ok, "{status:?}");
    let waiting = call(&h, &elephant_call(), "wait", json!({ "timeout": 1 }));
    assert!(waiting.ok, "{waiting:?}");
    assert!(waiting.data.get("event").is_some());
}

//! Integración del motor con git real, árbitro real y terminales falsas (§15).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use ez_arbiter::{FishLauncher, Space, SystemClock};
use ez_term::Terminal;

use super::*;

struct Fixture {
    #[allow(dead_code)]
    dir: crate::testutil::TempDir,
    folder: PathBuf,
    tokens: Arc<Tokens>,
    terminals: Arc<FakeTerminals>,
    arbiter: Arc<Arbiter>,
    runtime: HubRuntime,
}

fn git(folder: &PathBuf, args: &[&str]) {
    let mut command = std::process::Command::new("git");
    command.args(args).current_dir(folder);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    assert!(command.status().expect("git").success(), "git {args:?}");
}

fn fixture(name: &str) -> Fixture {
    let dir = crate::testutil::TempDir::new(name);
    let folder = dir.path().join("proyecto");
    let data = dir.path().join("datos");
    fs::create_dir_all(&folder).expect("proyecto");
    fs::create_dir_all(&data).expect("datos");
    git(&folder, &["init", "-q"]);
    git(&folder, &["config", "user.name", "Pez de prueba"]);
    git(&folder, &["config", "user.email", "pez@example.test"]);
    git(&folder, &["commit", "--allow-empty", "-q", "-m", "inicial"]);

    let tokens = Arc::new(Tokens::new());
    let terminals = FakeTerminals::with_tokens(Arc::clone(&tokens));
    let instance = launcher(Arc::clone(&terminals), Arc::clone(&tokens));
    let arbiter = Arc::new(
        Arbiter::new(
            data,
            Arc::clone(&instance) as Arc<dyn FishLauncher>,
            Arc::new(TeamInstructionWriter),
            Arc::new(SystemClock),
        )
        .expect("árbitro"),
    );
    arbiter.register_space(Space {
        id: "s1".to_string(),
        name: "Ez".to_string(),
        folder: folder.clone(),
    });
    let config = SpaceConfig {
        id: "s1".to_string(),
        name: "Ez".to_string(),
        folder: folder.clone(),
        suggested: Some(main_choice()),
    };
    let runtime = HubRuntime::with_parts(
        vec![config],
        instance,
        Arc::clone(&arbiter),
        tokens.clone(),
        ElephantTokens::load(dir.path().join("elefantes.json")),
        Principales::load(dir.path().join("principales.json")),
        false,
    );
    Fixture {
        dir,
        folder,
        tokens,
        terminals,
        arbiter,
        runtime,
    }
}

#[test]
fn two_main_agents_have_two_worktrees_and_two_tokens() {
    let f = fixture("runtime-two-mains");
    let a = f
        .runtime
        .main_agent_create("s1", "A", &main_choice())
        .expect("primer agente");
    let b = f
        .runtime
        .main_agent_create("s1", "B", &main_choice())
        .expect("segundo agente");
    assert_ne!(a.id, b.id);
    assert!(PathBuf::from(&a.worktree).is_dir());
    assert!(PathBuf::from(&b.worktree).is_dir());
    assert_ne!(a.address, b.address);
    let first = f
        .runtime
        .elephant_tokens
        .get(&a.id)
        .expect("token del primero");
    let second = f
        .runtime
        .elephant_tokens
        .get(&b.id)
        .expect("token del segundo");
    assert_ne!(first, second);
    assert!(matches!(
        f.tokens.lookup(&first),
        Some(Caller::Elephant { elephant, .. }) if elephant == a.id
    ));
    assert!(matches!(
        f.tokens.lookup(&second),
        Some(Caller::Elephant { elephant, .. }) if elephant == b.id
    ));
    assert_eq!(f.arbiter.main_agents("s1").len(), 3);
    assert!(PathBuf::from(&a.worktree).starts_with(f.folder.parent().expect("raiz")));
}

#[test]
fn space_tree_nests_by_parent_and_hangs_unlabeled_terminals() {
    let f = fixture("runtime-tree");
    let main = f
        .runtime
        .main_agent_create("s1", "A", &main_choice())
        .expect("agente principal");
    let mut parent_labels = BTreeMap::new();
    parent_labels.insert("ez".to_string(), "1".to_string());
    parent_labels.insert("ez_space".to_string(), "s1".to_string());
    parent_labels.insert("ez_elephant".to_string(), main.id.clone());
    parent_labels.insert("ez_fish".to_string(), "dev-r1-1".to_string());
    parent_labels.insert("ez_role".to_string(), "developer".to_string());
    parent_labels.insert("ez_agent".to_string(), "opencode".to_string());
    parent_labels.insert("ez_model".to_string(), "opencode-go/x".to_string());
    f.terminals.add("f1", parent_labels, "live");

    let child_labels = BTreeMap::from([
        ("ez".to_string(), "1".to_string()),
        ("ez_space".to_string(), "s1".to_string()),
        ("ez_elephant".to_string(), main.id.clone()),
        ("ez_fish".to_string(), "dev-r1-1-auditor".to_string()),
        ("ez_role".to_string(), "security_auditor".to_string()),
        ("ez_parent".to_string(), "dev-r1-1".to_string()),
    ]);
    f.terminals.add("f2", child_labels, "working");
    f.terminals.terminals.lock().insert(
        "u1".to_string(),
        Terminal {
            address: "u1".to_string(),
            name: Some("consulta".to_string()),
            parent: Some("f1".to_string()),
            state: "live".to_string(),
            ..Terminal::default()
        },
    );

    let tree = f.runtime.space_tree("s1").expect("árbol");
    let node = tree
        .nodes
        .iter()
        .find(|node| node.id == main.id)
        .expect("nodo principal");
    assert_eq!(node.children.len(), 1, "{node:?}");
    let parent = &node.children[0];
    assert_eq!(parent.id, "dev-r1-1");
    assert_eq!(parent.role.as_deref(), Some("Desarrollador"));
    let debug = serde_json::to_string(&tree).expect("json del árbol");
    assert_eq!(parent.children.len(), 2, "{debug}");
    let child = &parent.children[0];
    assert_eq!(child.id, "dev-r1-1-auditor", "{debug}");
    let loose = &parent.children[1];
    assert_eq!(loose.role.as_deref(), Some("sin papel"), "{debug}");
    assert_eq!(loose.address.as_deref(), Some("u1"));
}

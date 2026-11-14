//! Pruebas de los subcomandos de `fake-hc` sobre un estado temporal.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::{dispatch, Reply, State};

fn temp_state(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("reloj")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("ez-fakes-hc-{}-{nanos}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("carpeta temporal");
    dir.join("state.json")
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

fn call(args: &[&str]) -> Reply {
    let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    dispatch(&owned)
}

fn call_for(path: &Path, rest: &[&str]) -> Reply {
    let mut args = vec!["--state".to_string(), path.display().to_string()];
    args.extend(rest.iter().map(|arg| arg.to_string()));
    dispatch(&args)
}

fn json(reply: &Reply) -> Value {
    serde_json::from_str(&reply.stdout).expect("salida JSON")
}

#[test]
fn tab_new_lists_a_shell_terminal() {
    let path = temp_state("ls");
    let created = call_for(
        &path,
        &[
            "do",
            "tab.new",
            "--name",
            "Claude Code",
            "--folder",
            "C:\\work",
            "--argv",
            "[\"powershell.exe\"]",
            "--space",
            "EzAgent",
        ],
    );
    assert_eq!(created.code, 0);
    let created = json(&created);
    assert_eq!(created["terminal"], "t1");
    assert_eq!(created["space"], "EzAgent");

    let listed = json(&call_for(&path, &["ls", "--json"]));
    let terminal = &listed["terminals"][0];
    assert_eq!(terminal["address"], "t1");
    assert_eq!(terminal["agent"], "powershell.exe");
    assert_eq!(terminal["status"]["state"], "idle");
    assert_eq!(terminal["where"], "EzAgent / Claude Code");
    cleanup(&path);
}

#[test]
fn send_starts_an_agent_and_read_shows_its_screen() {
    let path = temp_state("send");
    call_for(
        &path,
        &["do", "tab.new", "--name", "Agente", "--argv", "[\"pwsh\"]"],
    );
    let sent = call_for(&path, &["send", "t1", "opencode --standalone", "--json"]);
    assert_eq!(sent.code, 0);
    let info = json(&call_for(&path, &["get", "t1"]));
    assert_eq!(info["agent"], "opencode");
    assert_eq!(info["status"]["state"], "idle");
    let screen = call_for(&path, &["read", "t1", "--lines", "40"]);
    assert!(screen.stdout.contains("OpenCode"));
    assert!(screen.stdout.contains("opencode --standalone"));
    cleanup(&path);
}

#[test]
fn keys_are_recorded_and_kill_removes_the_terminal() {
    let path = temp_state("keys");
    call_for(&path, &["do", "tab.new", "--name", "Agente"]);
    let pressed = call_for(&path, &["keys", "t1", "esc", "enter"]);
    assert_eq!(pressed.code, 0);
    assert_eq!(json(&pressed)["keys"], serde_json::json!(["esc", "enter"]));
    assert_eq!(call_for(&path, &["kill", "t1"]).code, 0);
    let missing = call_for(&path, &["get", "t1"]);
    assert_eq!(missing.code, 1);
    assert_eq!(json(&missing)["error"]["kind"], "not_found");
    cleanup(&path);
}

#[test]
fn put_checks_the_file_and_returns_the_path() {
    let path = temp_state("put");
    call_for(&path, &["do", "tab.new", "--name", "Agente"]);
    let missing = call_for(&path, &["put", "t1", "C:\\no-existe.md", "--json"]);
    assert_eq!(missing.code, 1);
    assert_eq!(json(&missing)["ok"], false);

    let file = path.with_file_name("mensaje.md");
    std::fs::write(&file, "hola").expect("archivo temporal");
    let delivered = call_for(&path, &["put", "t1", &file.display().to_string(), "--json"]);
    assert_eq!(delivered.code, 0);
    assert_eq!(json(&delivered)["path"], file.display().to_string());
    let _ = std::fs::remove_file(&file);
    cleanup(&path);
}

#[test]
fn spaces_and_tabs_group_terminals() {
    let path = temp_state("spaces");
    call_for(
        &path,
        &["do", "tab.new", "--name", "OpenCode", "--space", "EzAgent"],
    );
    call_for(
        &path,
        &["do", "tab.new", "--name", "Codex", "--space", "Otro"],
    );
    let spaces = json(&call_for(&path, &["spaces", "--json"]));
    assert_eq!(spaces["spaces"].as_array().expect("espacios").len(), 2);
    let tabs = json(&call_for(&path, &["tabs", "--json"]));
    assert_eq!(tabs["tabs"].as_array().expect("pestañas").len(), 2);
    cleanup(&path);
}

#[test]
fn run_is_asynchronous_and_uses_the_label() {
    let path = temp_state("run");
    let reply = call_for(
        &path,
        &[
            "run",
            "--json",
            "--bg",
            "--no-parent",
            "--cwd",
            "C:\\work",
            "--name",
            "Claude voz",
            "--label",
            "agent=claude",
            "--",
            "claude",
            "--model",
            "sonnet",
        ],
    );
    assert_eq!(reply.code, 0);
    let address = json(&reply)["address"]
        .as_str()
        .expect("dirección")
        .to_string();
    let info = json(&call_for(&path, &["get", &address]));
    assert_eq!(info["agent"], "claude");
    assert_eq!(info["status"]["state"], "idle");
    let screen = call_for(&path, &["read", &address]);
    assert!(screen.stdout.contains("Claude Code"));
    cleanup(&path);
}

#[test]
fn blocked_and_done_states_come_from_the_seed() {
    let path = temp_state("seed");
    let seed = path.with_file_name("seed.json");
    std::fs::write(
        &seed,
        r#"{
            "next": 1,
            "terminals": [{
                "address": "t1",
                "name": "Agente",
                "agent": "opencode",
                "state": "working",
                "screen": "• trabajando\n+ Thought · 1s\nTrabajando…",
                "done_after_reads": 2,
                "conclusion": "Terminé el plan."
            }]
        }"#,
    )
    .expect("seed");
    let mut args = vec![
        "--seed".to_string(),
        seed.display().to_string(),
        "--state".to_string(),
        path.display().to_string(),
        "read".to_string(),
        "t1".to_string(),
    ];
    assert!(!dispatch(&args).stdout.contains("Terminé"));
    args.pop();
    args.pop();
    args.extend(["read".to_string(), "t1".to_string()]);
    let done = dispatch(&args);
    assert!(done.stdout.contains("Terminé el plan."));
    let info = json(&call_for(&path, &["get", "t1"]));
    assert_eq!(info["status"]["state"], "idle");
    let _ = std::fs::remove_file(&seed);
    cleanup(&path);
}

#[test]
fn time_based_completion_is_visible_to_the_watcher() {
    let path = temp_state("timed");
    std::fs::write(
        &path,
        r#"{
            "next": 1,
            "terminals": [{
                "address": "t1",
                "name": "Agente",
                "agent": "opencode",
                "state": "working",
                "screen": "• trabajando",
                "done_after_ms": 0,
                "conclusion": "Listo."
            }]
        }"#,
    )
    .expect("estado");
    let first = json(&call_for(&path, &["ls", "--json"]));
    assert_eq!(first["terminals"][0]["status"]["state"], "working");
    let second = json(&call_for(&path, &["ls", "--json"]));
    assert_eq!(second["terminals"][0]["status"]["state"], "idle");
    let screen = call_for(&path, &["read", "t1"]);
    assert!(screen.stdout.contains("Listo."));
    cleanup(&path);
}

#[test]
fn session_selectors_emit_the_expected_markers() {
    let path = temp_state("selector");
    call_for(&path, &["do", "tab.new", "--name", "Agente"]);
    call_for(&path, &["send", "t1", "/models"]);
    call_for(&path, &["send", "t1", "sonnet"]);
    let screen = call_for(&path, &["read", "t1"]);
    assert!(screen.stdout.contains("Select model"));
    assert!(screen.stdout.contains("Switched model to sonnet"));

    call_for(&path, &["send", "t1", "/effort"]);
    call_for(&path, &["keys", "t1", "ctrl+t"]);
    let screen = call_for(&path, &["read", "t1"]);
    assert!(screen.stdout.contains("Select variant"));
    assert!(screen.stdout.contains("Switched variant to low"));
    cleanup(&path);
}

#[test]
fn concurrent_sends_do_not_lose_entries() {
    let path = temp_state("concurrent");
    call_for(&path, &["do", "tab.new", "--name", "Agente"]);
    let mut handles = Vec::new();
    for index in 0..8 {
        let path = path.clone();
        handles.push(std::thread::spawn(move || {
            let text = format!("mensaje-{index}");
            call_for(&path, &["send", "t1", text.as_str(), "--json"])
        }));
    }
    for handle in handles {
        assert_eq!(handle.join().expect("hilo").code, 0);
    }
    let state = State::load(&path, None).expect("estado");
    assert_eq!(state.terminals[0].sent.len(), 8);
    cleanup(&path);
}

#[test]
fn help_and_unknown_commands_have_their_codes() {
    assert_eq!(call(&["--help"]).code, 0);
    assert!(call(&["--help"]).stdout.contains("fake-hc"));
    let unknown = call(&["--state", "C:\\tmp\\nada.json", "inventado"]);
    assert_eq!(unknown.code, 2);
    assert!(unknown.stdout.contains("comando desconocido"));
}

#[test]
fn seeded_state_round_trips_through_serde() {
    let state: State = serde_json::from_str(
        r#"{"next": 2, "spaces": ["EzAgent"], "terminals": [{"address": "t2", "name": "x"}]}"#,
    )
    .expect("estado");
    assert_eq!(state.next, 2);
    assert_eq!(state.spaces, vec!["EzAgent"]);
    assert_eq!(state.terminals[0].agent, "powershell.exe");
    let text = serde_json::to_string(&state).expect("serializa");
    assert!(text.contains("\"where\":\"\""));
}

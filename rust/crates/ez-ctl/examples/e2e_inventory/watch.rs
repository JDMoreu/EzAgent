//! Step 6: `ez watch` warns the fake parent once per low-memory episode.
//!
//! The watcher runs in the foreground, with a short interval, and is killed by
//! the test after two rounds. It never kills a subagent.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::checks::Report;
use super::run;
use super::setup::{self, Sandbox};

/// How often the test watcher looks (ms).
const INTERVAL_MS: u64 = 300;

/// Runs `ez watch` for at least two rounds and checks the single warning.
pub fn run(sandbox: &Sandbox, parent: &str, report: &mut Report) {
    if setup::set_resources(sandbox, 999_999_999, 999_999_999, 99).is_err() {
        report.fail("umbrales de memoria absurdos", "no pude escribirlos");
        return;
    }
    let mut child = match spawn(sandbox, parent) {
        Ok(child) => child,
        Err(message) => {
            report.fail("`ez watch` arranca", message);
            return;
        }
    };
    let found = wait_warning(sandbox, parent, Duration::from_secs(10));
    std::thread::sleep(Duration::from_millis(INTERVAL_MS * 3));
    let still_running = child.try_wait().ok().flatten().is_none();
    let count = warning_count(sandbox, parent);
    let _ = child.kill();
    let _ = child.wait();
    report.check(
        "`ez watch` arranca con el padre falso",
        found.is_some(),
        match &found {
            Some(line) => line.chars().take(160).collect(),
            None => "no llegó ningún aviso de memoria".to_string(),
        },
    );
    report.check(
        "el aviso de memoria llega una sola vez en dos vueltas",
        count == 1 && still_running,
        format!("{count} aviso(s); seguía vivo={still_running}"),
    );
    if let Some(line) = found {
        report.check(
            "el aviso usa el texto de §4",
            line.contains("[EzAgent · aviso] Memoria baja:")
                && line.contains("RAM libre")
                && line.contains("virtual libre"),
            line,
        );
    }
}

/// Starts the watcher with the temporary environment.
fn spawn(sandbox: &Sandbox, parent: &str) -> Result<std::process::Child, String> {
    let args = vec![
        "watch".to_string(),
        "--parent".to_string(),
        parent.to_string(),
        "--dir".to_string(),
        setup::path_text(&sandbox.subagentes),
        "--hc".to_string(),
        setup::path_text(&sandbox.hc),
    ];
    let mut env = sandbox.env.clone();
    env.set("EZ_WATCH_INTERVAL_MS", INTERVAL_MS.to_string());
    env.set("EZ_WATCH_GRACE_MS", "60000".to_string());
    env.set("EZ_WATCH_IDLE_MS", "60000".to_string());
    let mut command = Command::new(&sandbox.ez);
    command
        .args(&args)
        .current_dir(&sandbox.work)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    env.apply(&mut command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn()
        .map_err(|error| format!("no pude ejecutar ez watch: {error}"))
}

/// Waits until the parent received the memory warning.
fn wait_warning(sandbox: &Sandbox, parent: &str, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(line) = parent_lines(sandbox, parent)
            .into_iter()
            .find(|line| line.contains("Memoria baja"))
        {
            return Some(line);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// How many memory warnings the parent received.
fn warning_count(sandbox: &Sandbox, parent: &str) -> usize {
    parent_lines(sandbox, parent)
        .iter()
        .filter(|line| line.contains("[EzAgent · aviso] Memoria baja:"))
        .count()
}

/// The lines the fake parent received.
fn parent_lines(sandbox: &Sandbox, parent: &str) -> Vec<String> {
    let path = sandbox.registros.join(format!("{parent}.json"));
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    run::strings(&value, "lines")
}

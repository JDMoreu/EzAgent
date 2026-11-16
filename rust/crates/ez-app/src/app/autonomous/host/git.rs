//! Evidencia del final (10.16): commit base y cambios, con `git` de solo
//! lectura, argumentos en lista y sin shell. Si no es un repositorio, se omite.

use std::process::{Command, Stdio};

/// `git -C <workdir> rev-parse HEAD`, o `None` si no es un repositorio.
pub fn base_commit(workdir: &str) -> Option<String> {
    git(workdir, &["rev-parse", "HEAD"])
}

/// `git -C <workdir> diff --stat <base>..HEAD`.
pub fn diff_stat(workdir: &str, base: &str) -> String {
    let range = format!("{base}..HEAD");
    git(workdir, &["diff", "--stat", &range]).unwrap_or_default()
}

/// `git -C <workdir> log --oneline <base>..HEAD`.
pub fn log_oneline(workdir: &str, base: &str) -> String {
    let range = format!("{base}..HEAD");
    git(workdir, &["log", "--oneline", &range]).unwrap_or_default()
}

fn git(workdir: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workdir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

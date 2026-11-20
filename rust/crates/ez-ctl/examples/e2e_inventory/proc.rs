//! Subprocesses of the test: `ez`, `hc` and `git` without a shell.
//!
//! Every process runs with the argument list and the temporary environment of
//! the sandbox, never with the user's folders.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// An environment overlay: variables to set and variables to remove.
#[derive(Debug, Clone, Default)]
pub struct EnvVars {
    pub pairs: Vec<(String, String)>,
    pub removed: Vec<String>,
}

impl EnvVars {
    /// Sets or replaces a variable.
    pub fn set(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        self.pairs.retain(|(key, _)| key != name);
        self.pairs.push((name.to_string(), value));
    }

    /// Removes a variable from the child environment.
    pub fn remove(&mut self, name: &str) {
        self.removed.push(name.to_string());
        self.pairs.retain(|(key, _)| key != name);
    }

    /// Applies the overlay to a command.
    pub fn apply(&self, command: &mut Command) {
        for name in &self.removed {
            command.env_remove(name);
        }
        for (name, value) in &self.pairs {
            command.env(name, value);
        }
    }
}

/// What a subprocess returned.
#[derive(Debug, Clone, Default)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// Whether the process exited with code 0.
    pub fn ok(&self) -> bool {
        self.code == 0
    }

    /// The error output, or the standard output if there is none.
    pub fn message(&self) -> String {
        let stderr = self.stderr.trim();
        if !stderr.is_empty() {
            return stderr.to_string();
        }
        self.stdout.trim().to_string()
    }
}

/// Runs a program with an argument list, without a shell.
pub fn run(program: &Path, args: &[String], env: &EnvVars, cwd: Option<&Path>) -> Output {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(folder) = cwd {
        command.current_dir(folder);
    }
    env.apply(&mut command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    match command.output() {
        Ok(output) => Output {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        },
        Err(error) => Output {
            code: -1,
            stdout: String::new(),
            stderr: format!("no pude ejecutar {}: {error}", program.display()),
        },
    }
}

/// Parses the JSON printed by a subprocess.
pub fn json(output: &Output) -> Result<serde_json::Value, String> {
    let text = output.stdout.trim();
    if text.is_empty() {
        return Err(format!("no devolvió nada: {}", output.message()));
    }
    serde_json::from_str(text).map_err(|error| {
        format!(
            "no es JSON ({error}): {}",
            text.chars().take(300).collect::<String>()
        )
    })
}

/// `shutil.which`: the real executable of a name or path.
pub fn which(program: &str) -> Option<PathBuf> {
    if program.is_empty() {
        return None;
    }
    let path = Path::new(program);
    if path.is_absolute() || path.components().count() > 1 {
        return candidate(path);
    }
    let dirs = std::env::var_os("PATH")?;
    std::env::split_paths(&dirs).find_map(|folder| candidate(&folder.join(path)))
}

/// The file of a PATH entry, with the Windows extensions.
fn candidate(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    #[cfg(windows)]
    {
        for extension in [".COM", ".EXE", ".BAT", ".CMD"] {
            let mut name = path.as_os_str().to_os_string();
            name.push(extension);
            let variant = PathBuf::from(name);
            if variant.is_file() {
                return Some(variant);
            }
        }
    }
    None
}

//! Subprocesos de la prueba: ejecutar `ez`, `hc` y `herdr-app` sin shell.
//!
//! Cada proceso se lanza con argumentos en lista y con el entorno de la
//! prueba (carpetas temporales), nunca con el del usuario.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// El entorno de un subproceso: lo que se cambia y lo que se quita.
#[derive(Debug, Clone, Default)]
pub struct EnvVars {
    /// Variables que se cambian o se agregan.
    pub pairs: Vec<(String, String)>,
    /// Variables que se quitan (para que no llegue la del usuario).
    pub removed: Vec<String>,
}

impl EnvVars {
    /// Una variable del entorno.
    pub fn set(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        self.pairs.retain(|(key, _)| key != name);
        self.pairs.push((name.to_string(), value));
    }

    /// Quita una variable del entorno.
    pub fn remove(&mut self, name: &str) {
        self.removed.push(name.to_string());
        self.pairs.retain(|(key, _)| key != name);
    }

    /// Aplica el entorno a un comando.
    pub fn apply(&self, command: &mut Command) {
        for name in &self.removed {
            command.env_remove(name);
        }
        for (name, value) in &self.pairs {
            command.env(name, value);
        }
    }
}

/// Lo que devolvió un subproceso.
#[derive(Debug, Clone, Default)]
pub struct Output {
    /// El código de salida.
    pub code: i32,
    /// La salida estándar.
    pub stdout: String,
    /// La salida de error.
    pub stderr: String,
}

impl Output {
    /// ¿Terminó bien?
    pub fn ok(&self) -> bool {
        self.code == 0
    }

    /// La salida de error, o la estándar si no hay.
    pub fn message(&self) -> String {
        let stderr = self.stderr.trim();
        if !stderr.is_empty() {
            return stderr.to_string();
        }
        self.stdout.trim().to_string()
    }
}

/// Ejecuta un programa con argumentos en lista (sin shell).
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

/// El JSON de la salida de un subproceso.
pub fn json(output: &Output) -> Result<serde_json::Value, String> {
    let text = output.stdout.trim();
    if text.is_empty() {
        return Err(format!("no devolvió nada: {}", output.message()));
    }
    serde_json::from_str(text).map_err(|error| format!("no es JSON ({error}): {}", head(text, 300)))
}

/// Las primeras letras de un texto.
pub fn head(text: &str, count: usize) -> String {
    text.chars().take(count).collect()
}

/// `shutil.which`: el ejecutable real de un nombre o ruta.
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

/// El archivo del PATH con su nombre (y su extensión en Windows).
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

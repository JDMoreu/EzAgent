//! `AgentConfig` de `src/ezagent/agent/config.py`: la política más las rutas de la app.
//!
//! `permissions.rs` tiene el contenido y los tipos de `permissions.toml`; aquí
//! está lo que faltaba: leerlo del disco y las carpetas de trabajo (F3-05).

use std::path::{Path, PathBuf};

use crate::dirs;
use crate::permissions::{
    parse_permissions, ConfirmConfig, GeneralConfig, PathsConfig, SkillsConfig, ToolsDConfig,
    WebConfig, DEFAULT_PERMISSIONS_TOML,
};

#[derive(Debug, Clone, PartialEq)]
pub struct AgentConfig {
    pub general: GeneralConfig,
    pub paths: PathsConfig,
    pub confirm: ConfirmConfig,
    pub web: WebConfig,
    pub skills: SkillsConfig,
    pub tools_d: ToolsDConfig,
    pub app_dir: PathBuf,
    pub workspace: PathBuf,
    pub permissions_path: PathBuf,
    pub system_md_path: PathBuf,
    pub memory_dir: PathBuf,
    pub history_path: PathBuf,
    pub audit_path: PathBuf,
    pub journal_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub global_skills_dir: PathBuf,
    pub tools_d_dir: PathBuf,
}

/// `_default_app_dir`: `%APPDATA%/EzAgent`, o XDG, o `~/.config/EzAgent`.
fn default_app_dir() -> PathBuf {
    let base = ["APPDATA", "XDG_CONFIG_HOME"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()));
    match base {
        Some(base) => PathBuf::from(base).join("EzAgent"),
        None => dirs::home().join(".config").join("EzAgent"),
    }
}

fn write_default_template(path: &Path) {
    // `Path.write_text` traduce `\n` al final de línea del sistema.
    let body = if cfg!(windows) {
        DEFAULT_PERMISSIONS_TOML.replace('\n', "\r\n")
    } else {
        DEFAULT_PERMISSIONS_TOML.to_string()
    };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, body));
    match written {
        Ok(()) => tracing::info!("Creada la política por defecto en {}", path.display()),
        Err(error) => tracing::warn!(
            "No se pudo crear {}; se usan los valores por defecto: {error}",
            path.display()
        ),
    }
}

impl AgentConfig {
    /// Carga la política; crea la plantilla por defecto si no existe.
    ///
    /// Si se pasa `path`, su carpeta se toma como `app_dir`. Devuelve también
    /// los avisos que la lectura generó.
    pub fn load(path: Option<&Path>) -> (Self, Vec<String>) {
        let permissions_path = path.map_or_else(
            || default_app_dir().join("permissions.toml"),
            Path::to_path_buf,
        );
        if !permissions_path.exists() {
            write_default_template(&permissions_path);
        }
        let source = permissions_path.display().to_string();
        let text = match std::fs::read(&permissions_path) {
            Ok(bytes) => String::from_utf8(bytes).ok(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(String::new()),
            Err(_) => None,
        };
        let (permissions, warnings) = match text {
            Some(text) => parse_permissions(&source, &text),
            None => {
                let message = format!("No se pudo leer {source}; se usan los valores por defecto");
                tracing::warn!("{message}");
                (Default::default(), vec![message])
            }
        };
        let app_dir = permissions_path
            .parent()
            .map_or_else(PathBuf::new, Path::to_path_buf);
        let memory_dir = app_dir.join("memory");
        let home = dirs::home();
        let config = Self {
            general: permissions.general,
            paths: permissions.paths,
            confirm: permissions.confirm,
            web: permissions.web,
            skills: permissions.skills,
            tools_d: permissions.tools_d,
            workspace: home.join("EzAgentWorkspace"),
            system_md_path: app_dir.join("system.md"),
            history_path: memory_dir.join("history.db"),
            audit_path: app_dir.join("logs").join("audit.jsonl"),
            journal_dir: app_dir.join("journal"),
            skills_dir: app_dir.join("skills"),
            global_skills_dir: home.join(".agents").join("skills"),
            tools_d_dir: app_dir.join("tools.d"),
            memory_dir,
            permissions_path,
            app_dir,
        };
        (config, warnings)
    }

    /// Crea las carpetas de trabajo de la app y el workspace.
    ///
    /// `~/.agents/skills` es compartida con otros clientes y puede no existir:
    /// el descubrimiento de skills lo tolera y no la crea.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        let audit_dir = self.audit_path.parent().map(Path::to_path_buf);
        let directories = [
            Some(self.app_dir.clone()),
            Some(self.workspace.clone()),
            Some(self.memory_dir.clone()),
            audit_dir,
            Some(self.journal_dir.clone()),
            Some(self.skills_dir.clone()),
            Some(self.tools_d_dir.clone()),
        ];
        for directory in directories.into_iter().flatten() {
            std::fs::create_dir_all(directory)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_defaults_and_paths_from_a_new_folder() {
        let dir =
            std::env::temp_dir().join(format!("ez-agentcfg-{}", uuid::Uuid::new_v4().simple()));
        let file = dir.join("permissions.toml");
        let (config, warnings) = AgentConfig::load(Some(&file));
        assert!(warnings.is_empty());
        assert!(file.exists());
        let text = std::fs::read_to_string(&file).expect("plantilla escrita");
        if cfg!(windows) {
            assert!(text.contains("\r\n"));
            assert!(!text.replace("\r\n", "").contains('\n'));
        } else {
            assert!(text.contains('\n'));
        }
        assert_eq!(config.general.max_tool_calls_per_turn, 4);
        assert_eq!(config.app_dir, dir);
        assert_eq!(config.history_path, dir.join("memory").join("history.db"));
        assert_eq!(config.audit_path, dir.join("logs").join("audit.jsonl"));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn invalid_files_fall_back_with_a_warning() {
        let dir =
            std::env::temp_dir().join(format!("ez-agentcfg-bad-{}", uuid::Uuid::new_v4().simple()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("permissions.toml");
        let _ = std::fs::write(&file, "[general\nx = ");
        let (config, warnings) = AgentConfig::load(Some(&file));
        assert_eq!(warnings.len(), 1);
        assert_eq!(config.general.bulk_threshold, 5);
        let _ = std::fs::remove_file(&file);
    }
}

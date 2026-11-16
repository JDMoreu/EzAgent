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
    use crate::permissions::Permissions;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ez-agentcfg-{}-{label}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn load_with(label: &str, text: Option<&str>) -> (AgentConfig, Vec<String>, PathBuf) {
        let dir = scratch(label);
        let file = dir.join("app").join("permissions.toml");
        if let Some(text) = text {
            let _ = std::fs::create_dir_all(file.parent().expect("carpeta"));
            let _ = std::fs::write(&file, text);
        }
        let (config, warnings) = AgentConfig::load(Some(&file));
        (config, warnings, file)
    }

    #[test]
    fn loads_defaults_and_paths_from_a_new_folder() {
        let dir = scratch("defaults");
        let file = dir.join("permissions.toml");
        let (config, warnings) = AgentConfig::load(Some(&file));
        assert!(warnings.is_empty());
        assert!(file.exists());
        let text = std::fs::read_to_string(&file).expect("plantilla escrita");
        let expected = if cfg!(windows) {
            DEFAULT_PERMISSIONS_TOML.replace('\n', "\r\n")
        } else {
            DEFAULT_PERMISSIONS_TOML.to_string()
        };
        assert_eq!(text, expected);
        let defaults = Permissions::default();
        assert_eq!(config.general, defaults.general);
        assert_eq!(config.paths, defaults.paths);
        assert_eq!(config.confirm, defaults.confirm);
        assert_eq!(config.web, defaults.web);
        assert_eq!(config.skills, defaults.skills);
        assert_eq!(config.tools_d, defaults.tools_d);
        assert_eq!(config.app_dir, dir);
        for derived in [
            &config.system_md_path,
            &config.memory_dir,
            &config.audit_path,
            &config.journal_dir,
            &config.skills_dir,
            &config.tools_d_dir,
            &config.history_path,
        ] {
            assert!(derived.starts_with(&dir), "{}", derived.display());
        }
        assert_eq!(
            config.workspace.file_name(),
            Some(std::ffi::OsStr::new("EzAgentWorkspace"))
        );
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn valid_values_are_respected() {
        let text = "[general]\nmax_tool_calls_per_turn = 9\nconfirm_timeout_s = 2.5\n\
                    bulk_threshold = 1\nmax_file_mb = 1\n\
                    [confirm]\naccept = [\"vale\", \"ok\"]\nreject = [\"nunca\"]\n\
                    [web]\nmax_fetch_chars = 100\nblock_private_ips = false\n\
                    [skills]\nallow_scripts = true\ntrusted = [\"a\"]\n\
                    [paths]\nread_roots = [\"~/x\"]\nwrite_roots = [\"~/y\"]\n\
                    deny = [\"~/z\"]\ndeny_globs = [\"*.sec\"]\n";
        let (config, warnings, _file) = load_with("valid", Some(text));
        assert!(warnings.is_empty(), "avisos: {warnings:?}");
        assert_eq!(config.general.max_tool_calls_per_turn, 9);
        assert_eq!(config.general.confirm_timeout_s, 2.5);
        assert_eq!(config.general.bulk_threshold, 1);
        assert_eq!(config.general.max_file_mb, 1.0);
        assert_eq!(config.confirm.accept, ["vale", "ok"]);
        assert_eq!(config.confirm.reject, ["nunca"]);
        assert_eq!(config.web.max_fetch_chars, 100);
        assert!(!config.web.block_private_ips);
        assert!(config.skills.allow_scripts);
        assert_eq!(config.skills.trusted, ["a"]);
        assert_eq!(config.paths.read_roots, ["~/x"]);
        assert_eq!(config.paths.deny_globs, ["*.sec"]);
    }

    #[test]
    fn invalid_types_fall_back_to_the_defaults() {
        let cases = [
            "[general]\nmax_tool_calls_per_turn = 'cuatro'",
            "[general]\nmax_tool_calls_per_turn = true",
            "[general]\nconfirm_timeout_s = 'rápido'",
            "[general]\nbulk_threshold = 2.5",
            "[web]\nblock_private_ips = 'yes'",
            "[web]\nmax_fetch_chars = '8000'",
            "[confirm]\naccept = 'sí'",
            "[confirm]\nreject = [1, 2]",
            "[paths]\ndeny = 'C:/Windows'",
            "[skills]\ntrusted = [1]",
        ];
        let defaults = Permissions::default();
        for (index, text) in cases.iter().enumerate() {
            let (config, warnings, _file) = load_with(&format!("bad-type-{index}"), Some(text));
            assert!(!warnings.is_empty(), "sin aviso: {text}");
            assert_eq!(config.general, defaults.general, "{text}");
            assert_eq!(config.web, defaults.web, "{text}");
            assert_eq!(config.confirm, defaults.confirm, "{text}");
            assert_eq!(config.paths, defaults.paths, "{text}");
            assert_eq!(config.skills, defaults.skills, "{text}");
        }
    }

    #[test]
    fn broken_toml_and_non_utf8_files_use_the_defaults() {
        let defaults = Permissions::default();
        let (_config, warnings, _file) = load_with("broken", Some("[general\nmax = "));
        assert_eq!(warnings.len(), 1);
        let (config, _warnings, _file) = load_with("broken", Some("[general\nmax = "));
        assert_eq!(config.general, defaults.general);

        let dir = scratch("non-utf8");
        let file = dir.join("app").join("permissions.toml");
        std::fs::create_dir_all(file.parent().expect("carpeta")).expect("carpeta");
        std::fs::write(&file, b"\xff\xfe[general]").expect("archivo");
        let (config, _warnings) = AgentConfig::load(Some(&file));
        assert_eq!(config.general.max_tool_calls_per_turn, 4);
    }

    #[test]
    fn sections_that_are_not_tables_are_ignored() {
        let (config, _warnings, _file) =
            load_with("not-tables", Some("general = \"no soy tabla\"\nweb = 3"));
        assert_eq!(config.general.max_tool_calls_per_turn, 4);
        assert_eq!(config.web.max_fetch_chars, 8000);
    }

    #[test]
    fn secrets_are_protected_by_default() {
        let (config, _warnings, _file) = load_with("secrets", None);
        for must in ["~/.ssh", "C:/Windows", "C:/Program Files"] {
            assert!(
                config.paths.deny.iter().any(|item| item == must),
                "{must} no está en deny"
            );
        }
        for must in ["**/.env", "**/*.pem", "**/*.key", "**/id_rsa*", "**/*.kdbx"] {
            assert!(
                config.paths.deny_globs.iter().any(|item| item == must),
                "{must} no está en deny_globs"
            );
        }
    }

    #[test]
    fn ensure_dirs_creates_the_folders_and_not_the_shared_skills_one() {
        let (mut config, _warnings, _file) = load_with("ensure-dirs", None);
        let root = scratch("ensure-dirs-root");
        config.workspace = root.join("workspace");
        config.global_skills_dir = root.join("global-skills");
        config.ensure_dirs().expect("carpetas creadas");
        for directory in [
            &config.app_dir,
            &config.memory_dir,
            config.audit_path.parent().expect("carpeta de auditoría"),
            &config.journal_dir,
            &config.skills_dir,
            &config.tools_d_dir,
            &config.workspace,
        ] {
            assert!(directory.is_dir(), "{}", directory.display());
        }
        assert!(!config.global_skills_dir.exists());
        config.ensure_dirs().expect("idempotente");
    }
}

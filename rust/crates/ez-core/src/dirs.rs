//! Rutas del perfil del usuario, como `voice/config.py` y `agent/paths.py` (F2-01).

use std::path::PathBuf;

/// `%APPDATA%\EzAgent` (o el equivalente XDG/home como en Python).
pub fn app_data_dir() -> PathBuf {
    if let Some(base) = std::env::var_os("APPDATA") {
        return PathBuf::from(base).join("EzAgent");
    }
    if let Some(base) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(base).join("EzAgent");
    }
    home().join(".config").join("EzAgent")
}

pub fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn config_path() -> PathBuf {
    app_data_dir().join("voice.json")
}

pub fn llm_config_path() -> PathBuf {
    app_data_dir().join("llm.json")
}

pub fn tasks_path() -> PathBuf {
    app_data_dir().join("tasks.json")
}

pub fn logs_dir() -> PathBuf {
    app_data_dir().join("logs")
}

pub fn latency_path() -> PathBuf {
    logs_dir().join("latency.jsonl")
}

pub fn audit_path() -> PathBuf {
    logs_dir().join("audit.jsonl")
}

pub fn history_db_path() -> PathBuf {
    app_data_dir().join("history.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_lives_under_appdata() {
        let path = config_path();
        assert!(path.ends_with("EzAgent/voice.json") || path.ends_with("EzAgent\\voice.json"));
    }

    #[test]
    fn logs_are_siblings_of_config() {
        assert_eq!(logs_dir().parent(), config_path().parent());
    }
}

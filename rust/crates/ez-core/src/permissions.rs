//! Port of `src/ezagent/agent/config.py` (F2-04).
//!
//! `permissions.toml` es la política que edita el usuario; el agente nunca lo
//! escribe. Un archivo inválido no tumba el arranque: se avisa y se usan los
//! valores por defecto. La lectura desde disco y las carpetas de la app se
//! portan cuando exista su tarea (aquí solo el contenido y los tipos).

use crate::pyjson::float_repr;

/// Plantilla que se escribe cuando no existe `permissions.toml`.
pub const DEFAULT_PERMISSIONS_TOML: &str = "\
[general]
max_tool_calls_per_turn = 4
confirm_timeout_s = 15
bulk_threshold = 5
max_file_mb = 10

[paths]
read_roots = ['~']
write_roots = ['~']
deny = ['C:/Windows', 'C:/Program Files', 'C:/Program Files (x86)', '~/.ssh', '~/AppData', '%APPDATA%/EzAgent']
deny_globs = ['**/.env', '**/.env.*', '**/*.pem', '**/*.key', '**/id_rsa*', '**/*.kdbx']

[confirm]
accept = ['sí', 'si', 'confirmo', 'adelante', 'dale']
reject = ['no', 'cancela', 'para']

[web]
max_fetch_chars = 8000
block_private_ips = true

[skills]
allow_scripts = false
trusted = []

[tools_d]
allowed_executables = ['code']
";

// DECISION: el agente debe poder operar en todo el perfil del usuario; los
// secretos siguen protegidos por `deny` y `deny_globs`.
pub const DEFAULT_READ_ROOTS: [&str; 1] = ["~"];
pub const DEFAULT_WRITE_ROOTS: [&str; 1] = ["~"];
pub const DEFAULT_DENY: [&str; 6] = [
    "C:/Windows",
    "C:/Program Files",
    "C:/Program Files (x86)",
    "~/.ssh",
    "~/AppData",
    "%APPDATA%/EzAgent",
];
pub const DEFAULT_DENY_GLOBS: [&str; 6] = [
    "**/.env",
    "**/.env.*",
    "**/*.pem",
    "**/*.key",
    "**/id_rsa*",
    "**/*.kdbx",
];
pub const DEFAULT_ACCEPT: [&str; 5] = ["sí", "si", "confirmo", "adelante", "dale"];
pub const DEFAULT_REJECT: [&str; 3] = ["no", "cancela", "para"];
pub const DEFAULT_ALLOWED_EXECUTABLES: [&str; 1] = ["code"];

pub const DEFAULT_MAX_TOOL_CALLS_PER_TURN: i64 = 4;
pub const DEFAULT_CONFIRM_TIMEOUT_S: f64 = 15.0;
pub const DEFAULT_BULK_THRESHOLD: i64 = 5;
pub const DEFAULT_MAX_FILE_MB: f64 = 10.0;
pub const DEFAULT_MAX_FETCH_CHARS: i64 = 8000;

const SECTION_KEYS: [(&str, &[&str]); 6] = [
    (
        "general",
        &[
            "max_tool_calls_per_turn",
            "confirm_timeout_s",
            "bulk_threshold",
            "max_file_mb",
        ],
    ),
    (
        "paths",
        &["read_roots", "write_roots", "deny", "deny_globs"],
    ),
    ("confirm", &["accept", "reject"]),
    ("web", &["max_fetch_chars", "block_private_ips"]),
    ("skills", &["allow_scripts", "trusted"]),
    ("tools_d", &["allowed_executables"]),
];

#[derive(Debug, Clone, PartialEq)]
pub struct GeneralConfig {
    pub max_tool_calls_per_turn: i64,
    pub confirm_timeout_s: f64,
    pub bulk_threshold: i64,
    pub max_file_mb: f64,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            max_tool_calls_per_turn: DEFAULT_MAX_TOOL_CALLS_PER_TURN,
            confirm_timeout_s: DEFAULT_CONFIRM_TIMEOUT_S,
            bulk_threshold: DEFAULT_BULK_THRESHOLD,
            max_file_mb: DEFAULT_MAX_FILE_MB,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathsConfig {
    pub read_roots: Vec<String>,
    pub write_roots: Vec<String>,
    pub deny: Vec<String>,
    pub deny_globs: Vec<String>,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            read_roots: strings(&DEFAULT_READ_ROOTS),
            write_roots: strings(&DEFAULT_WRITE_ROOTS),
            deny: strings(&DEFAULT_DENY),
            deny_globs: strings(&DEFAULT_DENY_GLOBS),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmConfig {
    pub accept: Vec<String>,
    pub reject: Vec<String>,
}

impl Default for ConfirmConfig {
    fn default() -> Self {
        Self {
            accept: strings(&DEFAULT_ACCEPT),
            reject: strings(&DEFAULT_REJECT),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebConfig {
    pub max_fetch_chars: i64,
    pub block_private_ips: bool,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            max_fetch_chars: DEFAULT_MAX_FETCH_CHARS,
            block_private_ips: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillsConfig {
    pub allow_scripts: bool,
    pub trusted: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolsDConfig {
    pub allowed_executables: Vec<String>,
}

impl Default for ToolsDConfig {
    fn default() -> Self {
        Self {
            allowed_executables: strings(&DEFAULT_ALLOWED_EXECUTABLES),
        }
    }
}

/// La política completa, sin las rutas de la app (`agent_dir`, `workspace`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Permissions {
    pub general: GeneralConfig,
    pub paths: PathsConfig,
    pub confirm: ConfirmConfig,
    pub web: WebConfig,
    pub skills: SkillsConfig,
    pub tools_d: ToolsDConfig,
}

/// Lee el contenido de `permissions.toml` y devuelve la política y los avisos.
pub fn parse_permissions(source: &str, text: &str) -> (Permissions, Vec<String>) {
    let mut warnings = Vec::new();
    let data = match toml::from_str::<toml::Table>(text) {
        Ok(table) => table,
        Err(error) => {
            warnings.push(format!(
                "No se pudo leer {source}; se usan los valores por defecto: {error}"
            ));
            return (Permissions::default(), warnings);
        }
    };
    warn_unknown_keys(&data, &mut warnings);
    let permissions = Permissions {
        general: general_config(section(&data, "general"), &mut warnings),
        paths: paths_config(section(&data, "paths"), &mut warnings),
        confirm: confirm_config(section(&data, "confirm"), &mut warnings),
        web: web_config(section(&data, "web"), &mut warnings),
        skills: skills_config(section(&data, "skills"), &mut warnings),
        tools_d: tools_d_config(section(&data, "tools_d"), &mut warnings),
    };
    for warning in &warnings {
        tracing::warn!("{warning}");
    }
    (permissions, warnings)
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn general_config(section: Option<&toml::Table>, warnings: &mut Vec<String>) -> GeneralConfig {
    GeneralConfig {
        max_tool_calls_per_turn: get_int(
            section,
            "max_tool_calls_per_turn",
            DEFAULT_MAX_TOOL_CALLS_PER_TURN,
            "general",
            warnings,
        ),
        confirm_timeout_s: get_float(
            section,
            "confirm_timeout_s",
            DEFAULT_CONFIRM_TIMEOUT_S,
            "general",
            warnings,
        ),
        bulk_threshold: get_int(
            section,
            "bulk_threshold",
            DEFAULT_BULK_THRESHOLD,
            "general",
            warnings,
        ),
        max_file_mb: get_float(
            section,
            "max_file_mb",
            DEFAULT_MAX_FILE_MB,
            "general",
            warnings,
        ),
    }
}

fn paths_config(section: Option<&toml::Table>, warnings: &mut Vec<String>) -> PathsConfig {
    PathsConfig {
        read_roots: get_str_list(
            section,
            "read_roots",
            &DEFAULT_READ_ROOTS,
            "paths",
            warnings,
        ),
        write_roots: get_str_list(
            section,
            "write_roots",
            &DEFAULT_WRITE_ROOTS,
            "paths",
            warnings,
        ),
        deny: get_str_list(section, "deny", &DEFAULT_DENY, "paths", warnings),
        deny_globs: get_str_list(
            section,
            "deny_globs",
            &DEFAULT_DENY_GLOBS,
            "paths",
            warnings,
        ),
    }
}

fn confirm_config(section: Option<&toml::Table>, warnings: &mut Vec<String>) -> ConfirmConfig {
    ConfirmConfig {
        accept: get_str_list(section, "accept", &DEFAULT_ACCEPT, "confirm", warnings),
        reject: get_str_list(section, "reject", &DEFAULT_REJECT, "confirm", warnings),
    }
}

fn web_config(section: Option<&toml::Table>, warnings: &mut Vec<String>) -> WebConfig {
    WebConfig {
        max_fetch_chars: get_int(
            section,
            "max_fetch_chars",
            DEFAULT_MAX_FETCH_CHARS,
            "web",
            warnings,
        ),
        block_private_ips: get_bool(section, "block_private_ips", true, "web", warnings),
    }
}

fn skills_config(section: Option<&toml::Table>, warnings: &mut Vec<String>) -> SkillsConfig {
    SkillsConfig {
        allow_scripts: get_bool(section, "allow_scripts", false, "skills", warnings),
        trusted: get_str_list(section, "trusted", &[], "skills", warnings),
    }
}

fn tools_d_config(section: Option<&toml::Table>, warnings: &mut Vec<String>) -> ToolsDConfig {
    ToolsDConfig {
        allowed_executables: get_str_list(
            section,
            "allowed_executables",
            &DEFAULT_ALLOWED_EXECUTABLES,
            "tools_d",
            warnings,
        ),
    }
}

fn warn_unknown_keys(data: &toml::Table, warnings: &mut Vec<String>) {
    for (name, value) in data {
        let Some((_, keys)) = SECTION_KEYS.iter().find(|(known, _)| known == name) else {
            warnings.push(format!(
                "Sección desconocida en permissions.toml: [{name}] (se ignora)"
            ));
            continue;
        };
        let Some(table) = value.as_table() else {
            warnings.push(format!(
                "La sección [{name}] debe ser una tabla; se usan los valores por defecto"
            ));
            continue;
        };
        for key in table.keys() {
            if key == "destructive_always_confirm" {
                warnings.push(
                    "La opción destructive_always_confirm no existe; el borrado siempre pide \
                     confirmación. Se ignora."
                        .to_string(),
                );
            } else if !keys.contains(&key.as_str()) {
                warnings.push(format!("Clave desconocida en [{name}]: {key} (se ignora)"));
            }
        }
    }
}

fn section<'data>(data: &'data toml::Table, name: &str) -> Option<&'data toml::Table> {
    data.get(name).and_then(toml::Value::as_table)
}

fn get_int(
    section: Option<&toml::Table>,
    key: &str,
    default: i64,
    section_name: &str,
    warnings: &mut Vec<String>,
) -> i64 {
    let Some(section) = section else {
        return default;
    };
    match section.get(key) {
        None => default,
        Some(toml::Value::Integer(value)) => *value,
        Some(_) => {
            warnings.push(format!(
                "Tipo inválido en {section_name}.{key}: se esperaba un entero y se usa {default}"
            ));
            default
        }
    }
}

fn get_float(
    section: Option<&toml::Table>,
    key: &str,
    default: f64,
    section_name: &str,
    warnings: &mut Vec<String>,
) -> f64 {
    let Some(section) = section else {
        return default;
    };
    match section.get(key) {
        None => default,
        Some(toml::Value::Integer(value)) => *value as f64,
        Some(toml::Value::Float(value)) => *value,
        Some(_) => {
            warnings.push(format!(
                "Tipo inválido en {section_name}.{key}: se esperaba un número y se usa {}",
                float_repr(default)
            ));
            default
        }
    }
}

fn get_bool(
    section: Option<&toml::Table>,
    key: &str,
    default: bool,
    section_name: &str,
    warnings: &mut Vec<String>,
) -> bool {
    let Some(section) = section else {
        return default;
    };
    match section.get(key) {
        None => default,
        Some(toml::Value::Boolean(value)) => *value,
        Some(_) => {
            warnings.push(format!(
                "Tipo inválido en {section_name}.{key}: se esperaba true/false y se usa {}",
                py_bool(default)
            ));
            default
        }
    }
}

fn get_str_list(
    section: Option<&toml::Table>,
    key: &str,
    default: &[&str],
    section_name: &str,
    warnings: &mut Vec<String>,
) -> Vec<String> {
    let Some(section) = section else {
        return strings(default);
    };
    match section.get(key) {
        None => strings(default),
        Some(toml::Value::Array(items))
            if items
                .iter()
                .all(|item| matches!(item, toml::Value::String(_))) =>
        {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_string())
                .collect()
        }
        Some(_) => {
            warnings.push(format!(
                "Tipo inválido en {section_name}.{key}: se esperaba una lista de textos y se usa {}",
                py_list(default)
            ));
            strings(default)
        }
    }
}

fn py_bool(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}

fn py_list(items: &[&str]) -> String {
    let quoted: Vec<String> = items.iter().map(|item| format!("'{item}'")).collect();
    format!("[{}]", quoted.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_template() {
        let (permissions, warnings) =
            parse_permissions("permissions.toml", DEFAULT_PERMISSIONS_TOML);
        assert!(warnings.is_empty(), "avisos: {warnings:?}");
        assert_eq!(permissions, Permissions::default());
        assert_eq!(permissions.general.bulk_threshold, 5);
        assert_eq!(permissions.paths.deny.len(), 6);
    }

    #[test]
    fn wrong_types_fall_back_with_a_warning() {
        let text = "[general]\nbulk_threshold = true\nconfirm_timeout_s = \"15\"\n";
        let (permissions, warnings) = parse_permissions("permissions.toml", text);
        assert_eq!(permissions.general.bulk_threshold, 5);
        assert_eq!(permissions.general.confirm_timeout_s, 15.0);
        assert_eq!(warnings.len(), 2, "avisos: {warnings:?}");
        assert!(warnings
            .iter()
            .any(|item| item.contains("general.bulk_threshold: se esperaba un entero")));
        assert!(warnings
            .iter()
            .any(|item| item.contains("general.confirm_timeout_s: se esperaba un número")));
    }

    #[test]
    fn unknown_sections_and_keys_are_reported() {
        let text = "[paths]\ndestructive_always_confirm = true\nborrar = 1\n\n[nueva]\nx = 1\n";
        let (_, warnings) = parse_permissions("permissions.toml", text);
        assert_eq!(warnings.len(), 3, "avisos: {warnings:?}");
        assert!(warnings.iter().any(|item| item.contains("[nueva]")));
        assert!(warnings
            .iter()
            .any(|item| item.contains("destructive_always_confirm")));
        assert!(warnings
            .iter()
            .any(|item| item.contains("siempre pide confirmación")));
        assert!(warnings.iter().any(|item| item.contains("borrar")));
    }

    #[test]
    fn broken_toml_uses_every_default() {
        let (permissions, warnings) = parse_permissions("permissions.toml", "[general\n");
        assert_eq!(permissions, Permissions::default());
        assert_eq!(warnings.len(), 1);
    }
}

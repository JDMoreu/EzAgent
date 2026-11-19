//! Inventario de los tipos del catálogo (§3): instalado, ruta nativa, versión,
//! modelos y esfuerzos. Nunca abre un agente: solo `--version` y listados.

use std::path::{Path, PathBuf};

use ez_core::config::VoiceConfig;
use ez_herdr::driver::text_list;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::probe::Probe;

/// Un tipo del catálogo con lo que se pudo detectar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentInfo {
    pub key: String,
    pub label: String,
    pub installed: bool,
    /// La ruta que de verdad se ejecuta (`.exe` real o el `.cmd`).
    pub path: Option<String>,
    pub version: Option<String>,
    pub models: Vec<String>,
    pub efforts: Vec<String>,
}

/// Recorre el catálogo de `voice.json` en su orden.
pub fn inventory(probe: &dyn Probe, cfg: &VoiceConfig) -> Vec<AgentInfo> {
    let Some(Value::Object(catalog)) = cfg.get("agents") else {
        return Vec::new();
    };
    let profiles = cfg.get("agent_profiles").and_then(Value::as_object);
    let empty = Map::new();
    catalog
        .iter()
        .map(|(key, command)| {
            let profile = profiles
                .and_then(|profiles| profiles.get(key))
                .and_then(Value::as_object)
                .unwrap_or(&empty);
            inspect(probe, key, command, profile)
        })
        .collect()
}

fn inspect(
    probe: &dyn Probe,
    key: &str,
    command: &Value,
    profile: &Map<String, Value>,
) -> AgentInfo {
    let program = text_list(Some(command))
        .into_iter()
        .next()
        .unwrap_or_default();
    let found = probe.which(&program);
    let native = found.as_deref().map(native_path);
    let version = native
        .as_deref()
        .and_then(|path| probe.version(path))
        .filter(|text| !text.is_empty());
    let mut models = text_list(profile.get("models"));
    if found.is_some() {
        models.extend(discovered(probe, key, &program, profile, native.as_deref()));
    }
    AgentInfo {
        key: key.to_string(),
        label: label_of(key, profile),
        installed: found.is_some(),
        path: native.map(|path| path.to_string_lossy().into_owned()),
        version,
        models: dedupe(models),
        efforts: efforts_of(profile),
    }
}

fn label_of(key: &str, profile: &Map<String, Value>) -> String {
    match profile.get("label").and_then(Value::as_str) {
        Some(label) if !label.trim().is_empty() => label.to_string(),
        _ => key.to_string(),
    }
}

/// Modelos que se descubren sin red: los de `ez-herdr` y `opencode models`.
fn discovered(
    probe: &dyn Probe,
    key: &str,
    program: &str,
    profile: &Map<String, Value>,
    native: Option<&Path>,
) -> Vec<String> {
    let mut models = Vec::new();
    match profile.get("discover").and_then(Value::as_str) {
        Some("mcode") => models.extend(ez_herdr::prelaunch::discover_mcode_models()),
        Some("codex") => {
            models.extend(ez_herdr::prelaunch::discover_codex_models().keys().cloned())
        }
        _ => {}
    }
    if is_opencode(key, program) {
        if let Some(path) = native {
            models.extend(probe.opencode_models(path));
        }
    }
    models
}

fn is_opencode(key: &str, program: &str) -> bool {
    key.eq_ignore_ascii_case("opencode")
        || Path::new(program)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase().contains("opencode"))
            .unwrap_or(false)
}

/// Niveles de esfuerzo del perfil y de cada modelo (OpenCode).
fn efforts_of(profile: &Map<String, Value>) -> Vec<String> {
    let mut efforts = text_list(profile.get("efforts"));
    if let Some(Value::Object(by_model)) = profile.get("model_efforts") {
        for levels in by_model.values() {
            efforts.extend(text_list(Some(levels)));
        }
    }
    dedupe(efforts)
}

fn dedupe(items: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let text = item.trim();
        if text.is_empty() {
            continue;
        }
        if !out.iter().any(|seen| seen.eq_ignore_ascii_case(text)) {
            out.push(text.to_string());
        }
    }
    out
}

/// La ruta nativa: si `found` es un guion de npm, el `.exe` al que apunta o el
/// `.cmd` que Windows puede ejecutar.
pub fn native_path(found: &Path) -> PathBuf {
    if has_extension(found, "exe") {
        return found.to_path_buf();
    }
    let Some(dir) = found.parent() else {
        return found.to_path_buf();
    };
    let name = found
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = script_stem(&name);
    let sibling = dir.join(format!("{stem}.exe"));
    if sibling.is_file() {
        return sibling;
    }
    for candidate in [
        found.to_path_buf(),
        dir.join(format!("{stem}.cmd")),
        dir.join(format!("{stem}.bat")),
    ] {
        if let Ok(text) = std::fs::read_to_string(&candidate) {
            if let Some(path) = exe_in_script(&text, dir, &stem) {
                return path;
            }
        }
    }
    let cmd = dir.join(format!("{stem}.cmd"));
    if cmd.is_file() {
        return cmd;
    }
    found.to_path_buf()
}

fn has_extension(path: &Path, wanted: &str) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().eq_ignore_ascii_case(wanted))
        .unwrap_or(false)
}

fn script_stem(name: &str) -> String {
    let path = Path::new(name);
    let is_script = ["cmd", "bat", "exe", "ps1"]
        .iter()
        .any(|extension| has_extension(path, extension));
    if is_script {
        path.file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| name.to_string())
    } else {
        name.to_string()
    }
}

/// Busca en un guion de npm un argumento que termine en `.exe` y exista.
///
/// shortcut: solo acepta un `.exe` cuyo nombre se parezca al del programa
/// (`opencode` → `opencode.exe`); cuando el guion solo lanza otro binario
/// (`node.exe` para Pi), se prefiere el `.cmd`, que es lo que Windows ejecuta
/// sin un shell propio. Entiende las formas de npm y bun (`$basedir`, `%~dp0`,
/// `%dp0%`) y no un shell arbitrario.
fn exe_in_script(text: &str, dir: &Path, program_stem: &str) -> Option<PathBuf> {
    let base = dir.to_string_lossy().into_owned();
    let wanted = program_stem.to_lowercase();
    let separators = [' ', '\t', '"', '\'', ';', '(', ')'];
    for token in text.split(separators) {
        let token = token.trim();
        if !token.to_lowercase().ends_with(".exe") {
            continue;
        }
        let expanded = token
            .replace("${basedir}", &base)
            .replace("$basedir", &base)
            .replace("%~dp0", &format!("{base}\\"))
            .replace("%dp0%", &base);
        let path = PathBuf::from(expanded);
        let path = if path.is_absolute() {
            path
        } else {
            dir.join(path)
        };
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let related = !wanted.is_empty() && (name.contains(&wanted) || wanted.contains(&name));
        if related {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod more_tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn un_cmd_con_dp0_tambien_encuentra_el_exe() {
        let dir = std::env::temp_dir().join(format!("ez-scan-dp0-{}", std::process::id()));
        let bin = dir.join("node_modules").join("tool").join("tool.exe");
        std::fs::create_dir_all(bin.parent().expect("carpeta")).expect("carpeta");
        std::fs::write(&bin, "").expect("exe");
        let cmd = dir.join("tool.cmd");
        std::fs::write(
            &cmd,
            "@ECHO off\n\"%dp0%\\node_modules\\tool\\tool.exe\" %*\n",
        )
        .expect("cmd");
        assert_eq!(native_path(&cmd), bin);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn un_exe_se_queda_tal_cual() {
        let path = Path::new("C:\\tools\\agent.exe");
        assert_eq!(native_path(path), path);
    }

    #[test]
    fn si_el_guion_solo_lanza_node_se_queda_el_cmd() {
        let dir = std::env::temp_dir().join(format!("ez-scan-node-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("carpeta");
        std::fs::write(dir.join("node.exe"), "").expect("node");
        let cmd = dir.join("pi.cmd");
        std::fs::write(
            &cmd,
            "@ECHO off\n\"%dp0%\\node.exe\" \"%dp0%\\cli.js\" %*\n",
        )
        .expect("cmd");
        assert_eq!(native_path(&dir.join("pi")), cmd);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

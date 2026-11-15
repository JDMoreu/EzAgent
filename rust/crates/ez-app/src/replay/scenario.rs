//! Escenario YAML del arnés y preparación del perfil temporal.
//!
//! Porta `load_scenario`, `scenario_settings`, `prepare_home`,
//! `setup_environment` y `substitute` de `parity/replay/run_py.py`. El bloque
//! `llm`/`whisper` se entrega en JSON a los binarios de `ez-fakes`; el resto es
//! el estado inicial de la app.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pyvalue::{py_strip, python_str, truthy};
use serde_json::{json, Map, Value};

/// Raíz de datos del repo (`parity/replay/...`) y binarios falsos.
pub struct Paths {
    pub root: PathBuf,
    pub replay: PathBuf,
    pub audio: PathBuf,
    pub scenarios: PathBuf,
    pub golden: PathBuf,
    pub fakes: PathBuf,
}

impl Paths {
    /// Rutas del arnés: raíz del repo (o `EZ_REPLAY_ROOT`) y `target`.
    pub fn discover() -> Result<Self, String> {
        let root = match std::env::var_os("EZ_REPLAY_ROOT") {
            Some(root) => PathBuf::from(root),
            None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join(".."),
        };
        let replay = root.join("parity").join("replay");
        let fakes = match std::env::var_os("EZ_REPLAY_FAKES") {
            Some(dir) => PathBuf::from(dir),
            None => std::env::current_exe()
                .map_err(|error| format!("no pude localizar el ejecutable: {error}"))?
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| "el ejecutable no tiene carpeta".to_string())?,
        };
        Ok(Self {
            audio: replay.join("audio"),
            scenarios: replay.join("scenarios"),
            golden: replay.join("golden"),
            replay,
            root,
            fakes,
        })
    }
}

/// Un escenario cargado del YAML.
pub struct Scenario {
    pub id: String,
    pub stem: String,
    pub name: String,
    pub timeout: f64,
    /// Documento completo en JSON (ya con `{home}`, `{workspace}` y `{fakes}`).
    pub data: Value,
}

impl Scenario {
    /// `load_scenario`: el id es `str(data["id"] or stem.split("-")[0])`.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("no pude leer {}: {error}", path.display()))?;
        let yaml: serde_yaml::Value = serde_yaml::from_str(&text)
            .map_err(|error| format!("{} no es YAML válido: {error}", path.display()))?;
        let data = serde_json::to_value(&yaml)
            .map_err(|error| format!("{} no se puede convertir a JSON: {error}", path.display()))?;
        let stem = path
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        let id = data
            .get("id")
            .map(scalar_text)
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| stem.split('-').next().unwrap_or_default().to_string());
        let name = data.get("name").map(scalar_text).unwrap_or_default();
        let timeout = data.get("timeout").and_then(Value::as_f64).unwrap_or(60.0);
        Ok(Self {
            id,
            stem,
            name,
            timeout,
            data,
        })
    }

    /// Sustituye `{home}`, `{workspace}`, `{fakes}` y `{web_url}`.
    pub fn substitute(&mut self, tokens: &BTreeMap<String, String>) {
        self.data = substitute(&self.data, tokens);
    }

    /// Bloque `llm`/`whisper` para los falsos (objeto vacío si falta).
    pub fn block(&self, key: &str) -> Value {
        self.data.get(key).cloned().unwrap_or_else(|| json!({}))
    }

    /// Ajustes del arnés con los `settings:` del escenario encima.
    pub fn settings(&self, llm_port: u16, whisper_port: u16, fakes: &Path) -> Map<String, Value> {
        let mut settings = Map::new();
        settings.insert("chat_voice".to_string(), json!(true));
        settings.insert("auto_start_whisper".to_string(), json!(false));
        settings.insert("smart_turn".to_string(), json!(false));
        settings.insert("silence_secs".to_string(), json!(1.3));
        settings.insert("wake_phrase".to_string(), json!("EzAgent, escúchame"));
        settings.insert("close_action".to_string(), json!("pause"));
        settings.insert("llm_provider".to_string(), json!("custom"));
        settings.insert("llm_api_style".to_string(), json!("openai"));
        settings.insert("llm_model".to_string(), json!("fake-model"));
        settings.insert(
            "llm_base_url".to_string(),
            json!(format!("http://127.0.0.1:{llm_port}/v1")),
        );
        settings.insert("api_key".to_string(), json!("sin-clave"));
        settings.insert(
            "whisper_url".to_string(),
            json!(format!("http://127.0.0.1:{whisper_port}/inference")),
        );
        settings.insert("stt_model".to_string(), json!("fake-model"));
        settings.insert("stt_timeout_secs".to_string(), json!(5.0));
        settings.insert("llm_timeout_secs".to_string(), json!(10.0));
        settings.insert("llm_turn_timeout_secs".to_string(), json!(30.0));
        let hc = fakes.join("fake-hc.exe").display().to_string();
        settings.insert("hc_path".to_string(), json!(hc));
        settings.insert("herdr_app_path".to_string(), json!(hc));
        settings.insert("hud_hidden".to_string(), json!(false));
        settings.insert("announce_all".to_string(), json!(false));
        if let Some(overrides) = self.data.get("settings").and_then(Value::as_object) {
            for (key, value) in overrides {
                settings.insert(key.clone(), value.clone());
            }
        }
        settings
    }

    /// `prepare_home`: `voice.json`, `hc.json`, `agent.json` y variables.
    pub fn prepare_home(
        &self,
        home: &Path,
        settings: &Map<String, Value>,
    ) -> Result<PathBuf, String> {
        let config_path = home.join("EzAgent").join("voice.json");
        write_text(
            &config_path,
            &format!(
                "{}\n",
                dumps_with(
                    &Value::Object(settings.clone()),
                    &DumpOptions::utf8().with_indent(2)
                )
            ),
        )?;
        let hc = self
            .data
            .get("hc")
            .cloned()
            .unwrap_or_else(|| json!({"terminals": []}));
        let hc_state = home.join("hc.json");
        write_text(&hc_state, &dumps_with(&hc, &DumpOptions::utf8()))?;
        if let Some(agent) = self.data.get("agent") {
            write_text(
                &home.join("agent.json"),
                &dumps_with(agent, &DumpOptions::utf8()),
            )?;
        }
        let text = |key: &str, default: &str| {
            settings
                .get(key)
                .map(scalar_text)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| default.to_string())
        };
        let env = [
            ("LLM_PROVIDER", text("llm_provider", "custom")),
            ("LLM_API_STYLE", text("llm_api_style", "openai")),
            ("LLM_MODEL", text("llm_model", "fake-model")),
            ("LLM_BASE_URL", text("llm_base_url", "")),
            ("LLM_API_KEY", text("api_key", "sin-clave")),
            ("WHISPER_URL", text("whisper_url", "")),
            ("FAKE_HC_STATE", hc_state.display().to_string()),
        ];
        for (name, value) in env {
            std::env::set_var(name, value);
        }
        Ok(config_path)
    }

    /// `setup_environment`: permisos, archivos, agentes falsos y skills.
    ///
    /// `web` se levanta antes de arrancar los servicios (ver `run.rs`).
    pub fn setup_environment(&self, home: &Path, fakes: &Path) -> Result<(), String> {
        let Some(setup) = self.data.get("setup") else {
            return Ok(());
        };
        if let Some(permissions) = setup.get("permissions").filter(|value| truthy(value)) {
            let path = home.join("EzAgent").join("permissions.toml");
            write_text(&path, &format!("{}\n", py_strip(&python_str(permissions))))?;
        }
        if let Some(files) = setup.get("files").and_then(Value::as_object) {
            for (raw_path, content) in files {
                write_text(Path::new(raw_path), &python_str(content))?;
            }
        }
        if let Some(bins) = setup.get("agent_bins").and_then(Value::as_array) {
            let source = fakes.join("fake-agent.exe");
            let bytes = std::fs::read(&source)
                .map_err(|error| format!("no pude leer {}: {error}", source.display()))?;
            for name in bins.iter().map(python_str) {
                let target = home.join("bin").join(format!("{name}.exe"));
                write_bytes(&target, &bytes)?;
            }
        }
        let skills_dir = home.join("EzAgent").join("skills");
        if let Some(skills) = setup.get("skills").and_then(Value::as_object) {
            for (name, spec) in skills {
                let folder = skills_dir.join(name);
                std::fs::create_dir_all(&folder)
                    .map_err(|error| format!("no pude crear {}: {error}", folder.display()))?;
                let description = spec
                    .get("description")
                    .filter(|value| truthy(value))
                    .map_or_else(String::new, |value| {
                        py_strip(&python_str(value)).to_string()
                    });
                let body = spec
                    .get("body")
                    .filter(|value| truthy(value))
                    .map_or_else(String::new, |value| {
                        py_strip(&python_str(value)).to_string()
                    });
                write_text(
                    &folder.join("SKILL.md"),
                    &format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n"),
                )?;
                if let Some(scripts) = spec.get("scripts").and_then(Value::as_object) {
                    for (raw_script, source) in scripts {
                        write_text(&folder.join(raw_script), &python_str(source))?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Lista ordenada de escenarios disponibles.
pub fn scenario_paths(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("no pude listar {}: {error}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .collect();
    paths.sort();
    Ok(paths)
}

/// `substitute`: reemplaza los tokens en textos, mapas y listas.
fn substitute(value: &Value, tokens: &BTreeMap<String, String>) -> Value {
    match value {
        Value::String(text) => Value::String(replace_tokens(text, tokens)),
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| substitute(item, tokens)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (replace_tokens(key, tokens), substitute(item, tokens)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Sustituye los tokens de un texto.
fn replace_tokens(text: &str, tokens: &BTreeMap<String, String>) -> String {
    let mut text = text.to_string();
    for (key, replacement) in tokens {
        text = text.replace(&format!("{{{key}}}"), replacement);
    }
    text
}

/// `str(value)` de un escalar YAML (números y booleanos incluidos).
pub fn scalar_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// `Path.write_bytes`: crea la carpeta y escribe los bytes tal cual.
fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("no pude crear {}: {error}", parent.display()))?;
    }
    std::fs::write(path, bytes)
        .map_err(|error| format!("no pude escribir {}: {error}", path.display()))
}

/// `Path.write_text(text, newline="\n")`: crea la carpeta y no traduce saltos.
fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("no pude crear {}: {error}", parent.display()))?;
    }
    std::fs::write(path, text)
        .map_err(|error| format!("no pude escribir {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario(id: &str) -> Scenario {
        Scenario {
            id: id.to_string(),
            stem: format!("{id}-x"),
            name: String::new(),
            timeout: 60.0,
            data: json!({"id": id, "settings": {"silence_secs": 1.0}}),
        }
    }

    #[test]
    fn settings_mix_base_and_scenario_values() {
        let settings = scenario("01").settings(9000, 9001, Path::new("C:/fakes"));
        assert_eq!(settings["silence_secs"], json!(1.0));
        assert_eq!(settings["llm_base_url"], json!("http://127.0.0.1:9000/v1"));
        assert_eq!(
            settings["whisper_url"],
            json!("http://127.0.0.1:9001/inference")
        );
        assert_eq!(settings["chat_voice"], json!(true));
        assert_eq!(settings["hud_hidden"], json!(false));
    }

    #[test]
    fn setup_writes_skills_permissions_and_fake_agent_bins() {
        let base = std::env::temp_dir().join(format!("ez-replay-setup-{}", std::process::id()));
        let home = base.join("home");
        let fakes = base.join("fakes");
        std::fs::create_dir_all(&fakes).expect("fakes");
        std::fs::write(fakes.join("fake-agent.exe"), b"exe").expect("fake-agent");
        let data = json!({
            "setup": {
                "permissions": "[web]
block_private_ips = false
",
                "files": {home.join("nota.txt").display().to_string(): "hola"},
                "agent_bins": ["opencode"],
                "skills": {
                    "saludo": {
                        "description": "Saluda",
                        "body": "Devuelve un saludo.",
                        "scripts": {"scripts/saludo.py": "print('Hola')"}
                    }
                }
            }
        });
        let scenario = Scenario {
            id: "x".to_string(),
            stem: "x".to_string(),
            name: String::new(),
            timeout: 60.0,
            data,
        };
        scenario.setup_environment(&home, &fakes).expect("setup");
        assert_eq!(
            std::fs::read_to_string(home.join("EzAgent").join("permissions.toml")).expect("perm"),
            "[web]
block_private_ips = false
"
        );
        assert_eq!(
            std::fs::read_to_string(home.join("nota.txt")).expect("nota"),
            "hola"
        );
        assert_eq!(
            std::fs::read(home.join("bin").join("opencode.exe")).expect("bin"),
            b"exe"
        );
        let skill = home.join("EzAgent").join("skills").join("saludo");
        assert_eq!(
            std::fs::read_to_string(skill.join("SKILL.md")).expect("skill"),
            "---
name: saludo
description: Saluda
---

Devuelve un saludo.
"
        );
        assert_eq!(
            std::fs::read_to_string(skill.join("scripts").join("saludo.py")).expect("script"),
            "print('Hola')"
        );
    }

    #[test]
    fn substitution_walks_every_string() {
        let value = json!({"a": "{home}/x", "b": ["{workspace}"]});
        let tokens = BTreeMap::from([
            ("home".to_string(), "H".to_string()),
            ("workspace".to_string(), "W".to_string()),
        ]);
        assert_eq!(substitute(&value, &tokens), json!({"a": "H/x", "b": ["W"]}));
    }
}

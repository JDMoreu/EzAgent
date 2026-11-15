//! Estado de `fake-hc` compartido con el arnés (F7-01b).
//!
//! Porta `hc_lock`, `read_hc`, `edit_hc`, `find_terminal` y `apply_hc_scene`
//! de `parity/replay/run_py.py`: el guion de `agent.scenes` y el paso `hc_set`
//! editan el mismo JSON con el cerrojo `<estado>.lock` que usa `fake-hc`.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ez_core::pyjson::{dumps_with, DumpOptions};
use ez_core::pyvalue::{python_str, truthy};
use serde_json::{json, Value};

/// Cerrojo entre procesos (`hc_lock`): `<estado>.lock` creado en exclusiva.
struct HcLock {
    path: PathBuf,
}

impl HcLock {
    fn acquire(target: &Path) -> Result<Self, String> {
        let mut name = target
            .file_name()
            .map_or_else(std::ffi::OsString::new, |name| name.to_os_string());
        name.push(".lock");
        let path = target.with_file_name(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("no pude crear {}: {error}", parent.display()))?;
        }
        for _ in 0..400 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => {
                    return Err(format!(
                        "no pude bloquear el estado {}: {error}",
                        target.display()
                    ))
                }
            }
        }
        Err(format!("el estado {} sigue bloqueado", target.display()))
    }
}

impl Drop for HcLock {
    fn drop(&mut self) {
        // El `lock.unlink()` del Python ignora los fallos.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// `read_hc`: tres intentos y estado vacío si no se puede leer.
pub fn read_hc(path: &Path) -> Value {
    for _ in 0..3 {
        if let Ok(text) = std::fs::read_to_string(path) {
            if let Ok(value) = ez_core::pyloads::loads(&text) {
                return value;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    json!({"terminals": []})
}

/// `edit_hc`: lee, muta y guarda con el cerrojo de `fake-hc`.
pub fn edit_hc(
    path: &Path,
    mutate: impl FnOnce(&mut Value) -> Result<(), String>,
) -> Result<(), String> {
    let _lock = HcLock::acquire(path)?;
    let mut data = if path.exists() {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("no pude leer {}: {error}", path.display()))?;
        ez_core::pyloads::loads(&text)
            .map_err(|error| format!("{} no es JSON válido: {error}", path.display()))?
    } else {
        json!({"terminals": []})
    };
    mutate(&mut data)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("no pude crear {}: {error}", parent.display()))?;
    }
    std::fs::write(path, dumps_with(&data, &DumpOptions::utf8()))
        .map_err(|error| format!("no pude escribir {}: {error}", path.display()))
}

/// `find_terminal` en mutable: primero `address`, después `name`.
pub fn find_terminal_mut<'a>(state: &'a mut Value, key: &str) -> Option<&'a mut Value> {
    let terminals = state.get_mut("terminals")?.as_array_mut()?;
    let field = |terminal: &Value, name: &str| {
        terminal
            .get(name)
            .filter(|value| truthy(value))
            .map_or_else(String::new, python_str)
    };
    let index = terminals
        .iter()
        .position(|terminal| field(terminal, "address") == key)
        .or_else(|| {
            terminals
                .iter()
                .position(|terminal| field(terminal, "name") == key)
        })?;
    terminals.get_mut(index)
}

/// Pantalla de una terminal como texto (`str(terminal.get("screen") or "")`).
fn screen_of(terminal: &Value) -> String {
    terminal
        .get("screen")
        .filter(|value| truthy(value))
        .map_or_else(String::new, python_str)
}

/// Añade una conclusión al final de la pantalla si no estaba ya.
fn append_conclusion(terminal: &mut Value, conclusion: &str) {
    let screen = screen_of(terminal);
    if let Some(object) = terminal.as_object_mut() {
        object.insert(
            "conclusion".to_string(),
            Value::String(conclusion.to_string()),
        );
        if !screen.contains(&format!("● {conclusion}")) {
            object.insert(
                "screen".to_string(),
                Value::String(format!("{screen}\n● {conclusion}")),
            );
        }
    }
}

/// `apply_hc_scene`: traduce una escena a lo que vería el vigía en Herdr.
pub fn apply_hc_scene(terminal: &mut Value, scene: &str, conclusion: &str) {
    if scene == "done" {
        set_state(terminal, "idle");
        if !conclusion.is_empty() {
            append_conclusion(terminal, conclusion);
        }
        return;
    }
    if scene == "blocked" {
        set_state(terminal, "blocked");
        let screen = screen_of(terminal);
        if !screen.contains("Do you trust") {
            if let Some(object) = terminal.as_object_mut() {
                object.insert(
                    "screen".to_string(),
                    Value::String(format!(
                        "{screen}\nDo you trust the files in this folder?\n\
                         ❯ 1. Yes, I trust\n  2. No, exit\nEnter to confirm · Esc to cancel"
                    )),
                );
            }
        }
        return;
    }
    set_state(terminal, scene);
}

fn set_state(terminal: &mut Value, state: &str) {
    if let Some(object) = terminal.as_object_mut() {
        object.insert("state".to_string(), Value::String(state.to_string()));
    }
}

/// `hc_set`: fija estado y conclusión de una terminal concreta.
pub fn hc_set(path: &Path, spec: &Value) -> Result<(), String> {
    let key = spec
        .get("address")
        .or_else(|| spec.get("name"))
        .filter(|value| truthy(value))
        .map_or_else(String::new, python_str);
    edit_hc(path, |state| {
        let terminal = find_terminal_mut(state, &key)
            .ok_or_else(|| format!("no hay terminal '{key}' en el estado de fake-hc"))?;
        let wanted = spec
            .get("state")
            .filter(|value| truthy(value))
            .map_or_else(|| "idle".to_string(), python_str);
        set_state(terminal, &wanted);
        let conclusion = spec
            .get("conclusion")
            .filter(|value| truthy(value))
            .map_or_else(String::new, python_str);
        if !conclusion.is_empty() {
            append_conclusion(terminal, &conclusion);
        }
        Ok(())
    })
}

/// Texto de un paso `hc_set` para `driver.step` (`_describe`).
pub fn describe_set(value: &Value) -> String {
    let name = value
        .get("name")
        .or_else(|| value.get("address"))
        .map_or_else(|| "None".to_string(), python_str);
    let state = value
        .get("state")
        .map_or_else(|| "None".to_string(), python_str);
    format!("hc_set:{name}:{state}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_file(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ez-replay-hc-{tag}-{}.json", std::process::id()))
    }

    #[test]
    fn scenes_match_the_python_screens() {
        let mut done = json!({"state": "working", "screen": "❯ tarea"});
        apply_hc_scene(&mut done, "done", "Listo.");
        assert_eq!(done["state"], "idle");
        assert_eq!(done["screen"], "❯ tarea\n● Listo.");
        let mut blocked = json!({"state": "working", "screen": "x"});
        apply_hc_scene(&mut blocked, "blocked", "");
        assert_eq!(blocked["state"], "blocked");
        assert!(blocked["screen"]
            .as_str()
            .unwrap_or("")
            .contains("Do you trust"));
    }

    #[test]
    fn hc_set_finds_by_name_and_adds_the_conclusion() {
        let path = state_file("set");
        std::fs::write(
            &path,
            r#"{"terminals":[{"address":"t1","name":"opencode","state":"working","screen":""}]}"#,
        )
        .expect("estado");
        hc_set(
            &path,
            &json!({"name": "opencode", "state": "idle", "conclusion": "Hecho."}),
        )
        .expect("hc_set");
        let state = read_hc(&path);
        assert_eq!(state["terminals"][0]["state"], "idle");
        assert!(state["terminals"][0]["screen"]
            .as_str()
            .unwrap_or("")
            .contains("● Hecho."));
        assert_eq!(state["terminals"][0]["address"], "t1");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_terminal_is_an_error_with_the_python_text() {
        let path = state_file("missing");
        std::fs::write(&path, r#"{"terminals":[]}"#).expect("estado");
        let error = hc_set(&path, &json!({"name": "nadie"})).expect_err("sin terminal");
        assert_eq!(error, "no hay terminal 'nadie' en el estado de fake-hc");
        let _ = std::fs::remove_file(&path);
    }
}

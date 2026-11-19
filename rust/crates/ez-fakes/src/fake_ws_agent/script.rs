//! Guion del agente principal falso: borradores y comandos `ez` (ECO-8).

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::Value;

use super::process;

const WAIT_TIMEOUT: u64 = 30;
const DEADLINE: Duration = Duration::from_secs(600);
const RETRY_PAUSE: Duration = Duration::from_millis(500);

/// Archivo que el agente principal escribe antes de empezar el guion.
#[derive(Debug, Clone, Deserialize)]
pub struct Draft {
    /// Ruta relativa a la carpeta de trabajo del agente principal.
    pub ruta: String,
    /// Contenido literal del archivo.
    pub contenido: String,
}

/// Paso del guion: un comando `ez`, una espera por un evento, o ambos.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Step {
    /// Argumentos del comando `ez` (sin el nombre del programa).
    #[serde(default)]
    pub ez: Vec<String>,
    /// Tipo de evento que debe traer `ez wait`.
    #[serde(default)]
    pub wait: Option<String>,
}

/// Guion completo, tal como lo escribe el ejemplo de punta a punta.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Script {
    /// Borradores que hay que escribir antes de ejecutar los pasos.
    #[serde(default, rename = "borrador")]
    pub drafts: Vec<Draft>,
    /// Pasos en orden.
    #[serde(default, rename = "paso")]
    pub steps: Vec<Step>,
}

impl Script {
    /// Lee el guion desde un archivo TOML.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("no pude leer el guion {}: {error}", path.display()))?;
        Self::from_toml(&text)
    }

    /// Parsea el guion desde texto (pruebas y generación del ejemplo).
    pub fn from_toml(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|error| format!("el guion no es TOML válido: {error}"))
    }
}

/// Ejecuta el guion completo con un tope de diez minutos para las esperas.
pub fn execute(path: &Path) -> Result<(), String> {
    let script = Script::load(path)?;
    for draft in &script.drafts {
        write_draft(draft)?;
    }
    let deadline = Instant::now() + DEADLINE;
    for step in &script.steps {
        if !step.ez.is_empty() {
            run_ez(&step.ez)?;
        }
        if let Some(event) = &step.wait {
            wait_for(event, deadline)?;
        }
    }
    Ok(())
}

fn write_draft(draft: &Draft) -> Result<(), String> {
    let path = Path::new(&draft.ruta);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("no pude crear {}: {error}", parent.display()))?;
    }
    std::fs::write(path, &draft.contenido)
        .map_err(|error| format!("no pude escribir {}: {error}", path.display()))
}

fn run_ez(args: &[String]) -> Result<(), String> {
    let output = process::ez()
        .args(args)
        .output()
        .map_err(|error| format!("no pude ejecutar ez {}: {error}", args.join(" ")))?;
    if output.status.success() {
        return Ok(());
    }
    let reason = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "ez {} fue rechazado: {}",
        args.first().map(String::as_str).unwrap_or(""),
        reason.trim().replace('\n', " | ")
    ))
}

fn wait_for(expected: &str, deadline: Instant) -> Result<(), String> {
    while Instant::now() < deadline {
        let output = process::ez()
            .args(["--json", "wait", "--timeout", &WAIT_TIMEOUT.to_string()])
            .output()
            .map_err(|error| format!("no pude ejecutar ez wait: {error}"))?;
        if output.status.success() && event_is(&output.stdout, expected) {
            return Ok(());
        }
        sleep(RETRY_PAUSE);
    }
    Err(format!("no vi el evento «{expected}» en diez minutos"))
}

fn event_is(stdout: &[u8], expected: &str) -> bool {
    let text = String::from_utf8_lossy(stdout);
    let Some(line) = text.lines().find(|line| line.trim_start().starts_with('{')) else {
        return false;
    };
    let Ok(reply) = serde_json::from_str::<Value>(line) else {
        return false;
    };
    reply["data"]["event"]["kind"].as_str() == Some(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "\
[[borrador]]
ruta = \".ez/borrador/encargo.md\"
contenido = \"Hola\"

[[paso]]
ez = [\"task\", \"new\"]

[[paso]]
wait = \"task_approved\"
";

    #[test]
    fn parses_drafts_and_steps_in_order() {
        let script = Script::from_toml(SCRIPT).expect("guion");
        assert_eq!(script.drafts.len(), 1);
        assert_eq!(script.drafts[0].ruta, ".ez/borrador/encargo.md");
        assert_eq!(script.steps.len(), 2);
        assert_eq!(script.steps[0].ez, ["task", "new"]);
        assert_eq!(script.steps[1].wait.as_deref(), Some("task_approved"));
    }

    #[test]
    fn an_empty_script_is_valid() {
        let script = Script::from_toml("").expect("vacío");
        assert!(script.drafts.is_empty());
        assert!(script.steps.is_empty());
        assert!(Script::from_toml("no = [").is_err());
    }

    #[test]
    fn recognizes_only_the_expected_event() {
        let reply = r#"{"ok":true,"message":"","data":{"event":{"kind":"task_approved"}}}"#;
        assert!(event_is(reply.as_bytes(), "task_approved"));
        assert!(!event_is(reply.as_bytes(), "team_approved"));
        assert!(!event_is(b"no es json", "task_approved"));
        let none = r#"{"ok":true,"data":{"event":null}}"#;
        assert!(!event_is(none.as_bytes(), "task_approved"));
    }

    #[test]
    fn writes_a_draft_in_its_own_folder() {
        let dir = std::env::temp_dir().join(format!("ez-fakes-script-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(".ez").join("borrador").join("encargo.md");
        write_draft(&Draft {
            ruta: path.to_string_lossy().into_owned(),
            contenido: "Hola".to_string(),
        })
        .expect("escribir");
        assert_eq!(std::fs::read_to_string(&path).expect("leer"), "Hola");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

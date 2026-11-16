//! Paridad de la CLI de `ezagent-voice` contra las salidas congeladas del
//! Python (PYOUT-1): la prueba ya no ejecuta Python en vivo.
//!
//! `--help` y `--write-config` se comparan con `parity/frozen/*.json`, sacados
//! de `python -m ezagent.voice.app` antes de retirar la app Python.
//! `--list-mics` y `--list-agents` dependen del equipo: su formato se comprueba
//! con dobles en `src/app/main/tests.rs` (`mic_row`, `agent_row`), así que aquí
//! no hay comparación en vivo.
//!
//! DECISION: `--write-config` parte de una configuración semilla con los valores
//! del LLM fijados (`llm_pinned`), para que el `voice.json` resultante no
//! dependa del `.env` ni de la carpeta desde la que se ejecuta la prueba.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const MANIFEST: &str = env!("CARGO_MANIFEST_DIR");
const BIN: &str = env!("CARGO_BIN_EXE_ezagent-voice");
const HELP_FROZEN: &str = "../../../parity/frozen/cli_help.json";
const WRITE_CONFIG_FROZEN: &str = "../../../parity/frozen/cli_write_config.json";

struct Run {
    code: i32,
    stdout: Vec<u8>,
}

fn frozen(relative: &str) -> Value {
    let path = Path::new(MANIFEST).join(relative);
    let text = std::fs::read_to_string(&path).expect("salida congelada legible");
    serde_json::from_str(&text).expect("salida congelada es JSON")
}

fn run(args: &[&str]) -> Run {
    let result = Command::new(BIN)
        .args(args)
        .current_dir(MANIFEST)
        .output()
        .expect("lanzar el binario");
    Run {
        code: result.status.code().unwrap_or(-1),
        stdout: result.stdout,
    }
}

fn code(value: &Value) -> i32 {
    value["code"].as_i64().unwrap_or_default() as i32
}

fn text(value: &Value) -> &str {
    value["stdout"].as_str().unwrap_or_default()
}

#[test]
fn help_matches_the_frozen_python_output() {
    let expected = frozen(HELP_FROZEN);
    let actual = run(&["--help"]);
    assert_eq!(actual.code, code(&expected), "--help: código de salida");
    assert_eq!(
        String::from_utf8_lossy(&actual.stdout),
        text(&expected),
        "--help: salida"
    );
}

/// `--write-config` con el destino en `--config`: mismo texto impreso y mismo
/// `voice.json` que el Python congelado.
#[test]
fn write_config_matches_the_frozen_python_output() {
    let expected = frozen(WRITE_CONFIG_FROZEN);
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).expect("carpeta temporal");
    let path = dir.join("voice.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&expected["seed"]).expect("semilla a JSON"),
    )
    .expect("semilla escrita");
    let path_text = path.to_string_lossy().into_owned();
    let actual = run(&["--config", path_text.as_str(), "--write-config"]);
    let written = std::fs::read(&path).expect("voice.json del binario");
    let _ = std::fs::remove_file(&path);
    assert_eq!(
        actual.code,
        code(&expected),
        "--write-config: código de salida"
    );
    assert_eq!(
        String::from_utf8_lossy(&actual.stdout).replace(path_text.as_str(), "{config}"),
        text(&expected),
        "--write-config: salida"
    );
    assert_eq!(
        String::from_utf8_lossy(&written),
        expected["config"].as_str().unwrap_or_default(),
        "--write-config: contenido de voice.json"
    );
}

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("ez-app-cli-parity-{}", std::process::id()))
}

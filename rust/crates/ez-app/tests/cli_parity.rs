//! Paridad de la CLI de `ezagent-voice` contra el Python real (P-APP).
//!
//! Ejecuta el binario y `python -m ezagent.voice.app` con los mismos argumentos
//! y compara la salida y el código de salida. El intérprete del proyecto es
//! opcional: si no existe, la prueba se marca como omitida con un mensaje.
//!
//! DECISION: `sys.argv[0]` se fija al nombre del binario Rust para que
//! `argparse` genere el mismo `prog` (y el mismo ancho de `usage:`); si no, la
//! única diferencia sería el nombre del programa. `PYTHONIOENCODING=utf-8`
//! iguala la codificación de la tubería (Python usa la del sistema al no haber
//! consola); `-s` evita el `site` del usuario pero deja leer esa variable.

use std::path::{Path, PathBuf};
use std::process::Command;

const PYTHON: &str = "C:/Users/JDMoreu/Documents/EzAgent/.venv/Scripts/python.exe";
const MANIFEST: &str = env!("CARGO_MANIFEST_DIR");
const BIN: &str = env!("CARGO_BIN_EXE_ezagent-voice");

struct Run {
    code: i32,
    stdout: Vec<u8>,
}

fn run_python(args: &[&str]) -> Run {
    let prog = Path::new(BIN).file_name().map_or_else(
        || "ezagent-voice".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let bootstrap =
        format!("import sys; sys.argv[0] = {prog:?}; from ezagent.voice.app import main; main()");
    output(
        Command::new(PYTHON)
            .args(["-s", "-c", bootstrap.as_str()])
            .args(args)
            .env("PYTHONIOENCODING", "utf-8"),
    )
}

fn run_rust(args: &[&str]) -> Run {
    output(Command::new(BIN).args(args))
}

fn output(command: &mut Command) -> Run {
    let result = command
        .current_dir(MANIFEST)
        .output()
        .expect("lanzar el programa");
    Run {
        code: result.status.code().unwrap_or(-1),
        stdout: result.stdout,
    }
}

fn assert_same(what: &str, rust: &Run, python: &Run) {
    assert_eq!(rust.code, python.code, "{what}: código de salida");
    assert_eq!(
        String::from_utf8_lossy(&rust.stdout),
        String::from_utf8_lossy(&python.stdout),
        "{what}: salida"
    );
}

#[test]
fn cli_matches_python() {
    if !Path::new(PYTHON).exists() {
        eprintln!("omitida: no existe el intérprete del proyecto en {PYTHON}");
        return;
    }
    assert_same("--help", &run_rust(&["--help"]), &run_python(&["--help"]));
    assert_same(
        "--list-agents",
        &run_rust(&["--list-agents"]),
        &run_python(&["--list-agents"]),
    );
    compare_list_mics();
    compare_write_config();
}

/// `--list-mics`: mismo formato de línea y mismas cifras de PortAudio.
///
/// Rust reproduce el bloque de MME, que es el que numera `voice.json` y el que
/// encabeza la lista del Python; las otras APIs de host (DirectSound, WASAPI y
/// WDM-KS) repiten los mismos micrófonos con otros índices y no se pueden
/// enumerar con las dependencias del worktree, así que se comparan las líneas
/// que Rust sí enumera.
fn compare_list_mics() {
    let rust = run_rust(&["--list-mics"]);
    let python = run_python(&["--list-mics"]);
    assert_eq!(rust.code, python.code, "--list-mics: código de salida");
    let rust_lines = lines(&rust.stdout);
    let python_lines = lines(&python.stdout);
    assert!(!rust_lines.is_empty(), "--list-mics: sin micrófonos");
    let comparable = python_lines
        .get(..rust_lines.len())
        .unwrap_or(&python_lines);
    assert_eq!(rust_lines.as_slice(), comparable, "--list-mics: listado");
}

fn lines(text: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(text)
        .lines()
        .map(str::to_string)
        .collect()
}

/// `--write-config` con el destino en `--config`: misma ruta para los dos,
/// mismo texto impreso y mismo `voice.json`.
fn compare_write_config() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).expect("carpeta temporal");
    let path = dir.join("voice.json");
    let text = path.to_string_lossy().into_owned();
    let args = ["--config", text.as_str(), "--write-config"];
    let rust = run_rust(&args);
    let rust_file = std::fs::read(&path).expect("voice.json del binario");
    std::fs::remove_file(&path).expect("limpiar voice.json");
    let python = run_python(&args);
    let python_file = std::fs::read(&path).expect("voice.json del Python");
    assert_same("--write-config", &rust, &python);
    assert_eq!(
        String::from_utf8_lossy(&rust_file),
        String::from_utf8_lossy(&python_file),
        "--write-config: contenido de voice.json"
    );
    let _ = std::fs::remove_file(&path);
}

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("ez-app-cli-parity-{}", std::process::id()))
}

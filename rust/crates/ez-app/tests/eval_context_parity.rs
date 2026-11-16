//! `eval --mode context` (Rust) contra `provider.py` (Python), caso por caso
//! (F8-02).
//!
//! Se salta si no hay intérprete del proyecto (`EZ_EVAL_PYTHON` o `.venv`):
//! comparar exige el Python con `ezagent` instalado.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

/// Casos del contrato offline (el mismo abanico que `evals/tests/contract.yaml`).
const CASES: [&str; 3] = [
    "hola",
    "¿qué hora es?",
    "copia el informe a la carpeta de respaldo",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Intérprete del proyecto: `EZ_EVAL_PYTHON` o el `.venv` del repositorio.
fn python_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("EZ_EVAL_PYTHON") {
        return Some(PathBuf::from(path));
    }
    ["Scripts/python.exe", "bin/python"]
        .iter()
        .map(|relative| repo_root().join(".venv").join(relative))
        .find(|candidate| candidate.is_file())
}

fn run_rust_context(user: &str) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ezagent-voice"))
        .args(["eval", "--mode", "context"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("el binario del eval arranca");
    let request = json!({"user": user, "config": {}}).to_string();
    child
        .stdin
        .as_mut()
        .expect("stdin disponible")
        .write_all(request.as_bytes())
        .expect("la petición se escribe");
    let output = child.wait_with_output().expect("el eval termina");
    assert!(output.status.success(), "el eval falló: {output:?}");
    serde_json::from_slice(&output.stdout).expect("el eval devuelve JSON")
}

/// Una sola corrida de Python para todos los casos (importar `ezagent` es caro).
fn run_python_context(python: &PathBuf) -> Vec<Value> {
    let script = r#"
import json, sys
sys.path.insert(0, sys.argv[1])
import provider
users = json.loads(sys.argv[2])
out = []
for user in users:
    result = provider.call_api(user, {"config": {"mode": "context"}}, {"vars": {"user": user}})
    if "error" in result:
        sys.stderr.write(result["error"] + "\n")
        sys.exit(1)
    out.append(json.loads(result["output"]))
print(json.dumps(out, ensure_ascii=False))
"#;
    let evals = repo_root().join("evals");
    let output = Command::new(python)
        .args(["-c", script])
        .arg(&evals)
        .arg(json!(CASES).to_string())
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .expect("el proveedor Python arranca");
    assert!(
        output.status.success(),
        "provider.py falló: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("provider.py devuelve JSON")
}

/// La hora del prompt cambia entre dos procesos; el resto debe ser idéntico.
fn without_clock(prompt: &str) -> String {
    prompt
        .lines()
        .filter(|line| !line.starts_with("Hoy es "))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn context_matches_the_python_provider() {
    let Some(python) = python_binary() else {
        return;
    };
    let expected = run_python_context(&python);
    assert_eq!(expected.len(), CASES.len());
    for (user, reference) in CASES.iter().zip(expected) {
        let actual = run_rust_context(user);
        assert_eq!(
            actual["available_tools"], reference["available_tools"],
            "herramientas distintas para {user:?}"
        );
        assert_eq!(
            actual["tools"], reference["tools"],
            "esquemas distintos en {user:?}"
        );
        assert_eq!(
            without_clock(actual["system_prompt"].as_str().unwrap_or("")),
            without_clock(reference["system_prompt"].as_str().unwrap_or("")),
            "prompt de sistema distinto en {user:?}"
        );
    }
}

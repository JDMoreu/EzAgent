//! `eval --mode context` (Rust) contra la salida congelada de `provider.py`
//! (PYOUT-1): la prueba ya no ejecuta Python en vivo.
//!
//! Los tres casos se capturaron con `evals/provider.py` (modo `context`) y se
//! guardaron sin la línea del reloj en `parity/frozen/eval_context.json`.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

const FROZEN: &str = "../../../parity/frozen/eval_context.json";

/// Casos del contrato offline (el mismo abanico que `evals/tests/contract.yaml`).
const CASES: [&str; 3] = [
    "hola",
    "¿qué hora es?",
    "copia el informe a la carpeta de respaldo",
];

fn frozen_context() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FROZEN);
    let text = std::fs::read_to_string(&path).expect("salida congelada legible");
    serde_json::from_str(&text).expect("salida congelada es JSON")
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

/// La hora del prompt cambia entre dos procesos; el resto debe ser idéntico.
fn without_clock(prompt: &str) -> String {
    prompt
        .lines()
        .filter(|line| !line.starts_with("Hoy es "))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn context_matches_the_frozen_python_provider() {
    let expected = frozen_context();
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

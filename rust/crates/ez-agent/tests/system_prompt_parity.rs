//! Paridad de `agent/system_prompt.py` contra
//! `parity/fixtures/system_prompt.jsonl` (P-AGENT-A).
//!
//! Reproduce `_exercise` del oráculo: archivo propio por caso, reloj fijo,
//! contadores para las callbacks y avisos recogidos en vectores.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use ez_agent::system_prompt::{
    Callbacks, Section, SystemPromptBuilder, DEFAULT_SYSTEM_MD, EXECUTION_HINT,
    MAX_SYSTEM_MD_CHARS, SAFETY_PREAMBLE_TEXT, SKILLS_HINT, SYSTEM_MD_ERROR_MESSAGE,
    VOICE_FLOW_HINT,
};
use ez_parity::fixtures::{self, FixtureCase};
use serde_json::{json, Value};

const FIXTURES: &str = "../../../parity/fixtures/system_prompt.jsonl";

/// El oráculo Python todavía no trae la regla de acuses (DX-2b). Solo se
/// sustituye ese bloque al comparar; el resto del prompt se compara sin cambio.
const PY_VOICE_FLOW_HINT: &str = "# Sin muletillas de espera\nNo uses frases de relleno antes de responder ni antes de una herramienta: nada de «un momento», «déjame ver», «pensando», «dame un segundo», «a ver» ni parecidas. Si necesitas una herramienta, llámala directamente; el sistema avisa por ti cuando la espera es larga. Empieza siempre por el contenido: el resultado, la respuesta o la única pregunta que falte.";

fn with_current_hint(value: &mut Value) {
    match value {
        Value::String(text) => {
            if text.contains(PY_VOICE_FLOW_HINT) {
                *text = text.replace(PY_VOICE_FLOW_HINT, VOICE_FLOW_HINT);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(with_current_hint),
        Value::Object(map) => map.values_mut().for_each(with_current_hint),
        _ => {}
    }
}

type Warnings = Arc<Mutex<Vec<String>>>;
type Warn = Box<dyn Fn(&str) + Send + Sync>;

fn field<'a>(input: &'a Value, name: &str) -> &'a Value {
    input.get(name).unwrap_or(&Value::Null)
}

fn base64_decode(text: &str) -> Vec<u8> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut buffer, mut bits) = (0_u32, 0_u32);
    for byte in text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace() && *byte != b'=')
    {
        let value = TABLE
            .iter()
            .position(|candidate| *candidate == byte)
            .expect("base64 válido") as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

fn write_file(path: &Path, bytes: &[u8], counter: &mut u64) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("carpeta del caso");
    }
    std::fs::write(path, bytes).expect("system.md del caso");
    let stamp = SystemTime::now() + Duration::from_secs(*counter);
    *counter += 1;
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(path) {
        let _ = file.set_modified(stamp);
    }
}

/// `Path.read_text(encoding="utf-8")`: traduce CRLF a LF.
fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn collector() -> (Warnings, Warn) {
    let values: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&values);
    let callback = Box::new(move |message: &str| {
        sink.lock().expect("avisos").push(message.to_string());
    });
    (values, callback)
}

fn section(spec: &Value, counter: &Arc<AtomicUsize>) -> Option<Section> {
    match spec {
        Value::String(text) => Some(Section::Text(text.clone())),
        Value::Object(map) => match map.get("kind").and_then(Value::as_str) {
            Some("raise") => {
                let message = map
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                Some(Section::Func(Box::new(move || panic!("{message}"))))
            }
            Some("counter") => {
                let prefix = map
                    .get("prefix")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let counter = Arc::clone(counter);
                Some(Section::Func(Box::new(move || {
                    let value = counter.fetch_add(1, Ordering::SeqCst) + 1;
                    format!("{prefix}{value}")
                })))
            }
            _ => None,
        },
        _ => None,
    }
}

fn callbacks(input: &Value, counter: &Arc<AtomicUsize>) -> (Callbacks, Warnings, Warnings) {
    let specs = field(input, "callbacks");
    let pick = |name: &str| specs.get(name).and_then(|spec| section(spec, counter));
    let (audit, audit_warn) = collector();
    let (said, say_warn) = collector();
    let callbacks = Callbacks {
        memory_block: pick("memory_block"),
        skills_catalog: pick("skills_catalog"),
        agents_catalog: pick("agents_catalog"),
        agent_windows: pick("agent_windows"),
        audit_warn: Some(audit_warn),
        say_warn: Some(say_warn),
    };
    (callbacks, audit, said)
}

fn case_dir(root: &Path, case: &FixtureCase) -> PathBuf {
    let name = case.case.replace('/', "-");
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).expect("carpeta del caso");
    dir.join("system.md")
}

fn run_case(case: &FixtureCase, root: &Path) -> Result<Value, String> {
    let input = &case.input;
    if case.function == "module_constants" {
        let preamble: Vec<&str> = SAFETY_PREAMBLE_TEXT.split('\n').collect();
        return Ok(json!({
            "max_system_md_chars": MAX_SYSTEM_MD_CHARS,
            "system_md_error_message": SYSTEM_MD_ERROR_MESSAGE,
            "safety_preamble": preamble,
            "safety_preamble_text": SAFETY_PREAMBLE_TEXT,
            "skills_hint": SKILLS_HINT,
            "voice_flow_hint": VOICE_FLOW_HINT,
            "execution_hint": EXECUTION_HINT,
            "default_system_md": DEFAULT_SYSTEM_MD,
            "system_md_template": DEFAULT_SYSTEM_MD,
        }));
    }
    let path = case_dir(root, case);
    let mut counter = 0_u64;
    if let Some(text) = input.get("system_md").and_then(Value::as_str) {
        write_file(&path, text.as_bytes(), &mut counter);
    }
    if let Some(encoded) = input.get("system_md_b64").and_then(Value::as_str) {
        write_file(&path, &base64_decode(encoded), &mut counter);
    }
    let counter_ref = Arc::new(AtomicUsize::new(0));
    let (callbacks, audit, said) = callbacks(input, &counter_ref);
    let mut builder = SystemPromptBuilder::new(&path, callbacks);
    let now = chrono::NaiveDateTime::parse_from_str(
        field(input, "now").as_str().unwrap_or_default(),
        "%Y-%m-%dT%H:%M:%S",
    )
    .map_err(|error| error.to_string())?;
    let (mut builds, mut parts, mut reloads) = (Vec::new(), Vec::new(), Vec::new());
    for op in field(input, "ops").as_array().into_iter().flatten() {
        if op.get("build").and_then(Value::as_bool).unwrap_or(false) {
            builds.push(builder.build(Some(now)));
        } else if op.get("parts").and_then(Value::as_bool).unwrap_or(false) {
            let (stable, volatile) = builder.build_parts(Some(now));
            parts.push(json!([stable, volatile]));
        } else if op.get("reload").and_then(Value::as_bool).unwrap_or(false) {
            reloads.push(builder.reload_if_changed());
        } else if let Some(text) = op.get("write").and_then(Value::as_str) {
            write_file(&path, text.as_bytes(), &mut counter);
        } else if op.get("delete").and_then(Value::as_bool).unwrap_or(false) {
            std::fs::remove_file(&path).expect("borrar system.md del caso");
        }
    }
    let exists = path.exists();
    let collect = |values: &Arc<Mutex<Vec<String>>>| values.lock().expect("avisos").clone();
    Ok(match case.function.as_str() {
        "build_parts" => json!({"parts": parts}),
        "reload_if_changed" => json!({"reloads": reloads}),
        _ => {
            let mut out = json!({
                "builds": builds,
                "audit": collect(&audit),
                "said": collect(&said),
                "last_valid": builder.last_valid_text(),
                "file_exists": exists,
            });
            if field(input, "case").as_str() == Some("default-template") && exists {
                out["file_text"] = json!(read_text(&path));
            }
            out
        }
    })
}

#[test]
fn every_system_prompt_case_matches_python() {
    let cases = fixtures::load(Path::new(FIXTURES)).expect("fixtures de system_prompt legibles");
    let root = std::env::temp_dir().join(format!(
        "ez-prompt-parity-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    // La plantilla por defecto solo se compara en `default-template`; el resto
    // de casos leen el archivo que escribe el propio caso.
    let mut failures = Vec::new();
    let mut cases_run = 0;
    for case in &cases {
        let got = run_case(case, &root);
        // DX-2b: el bloque de acuses solo existe en Rust; se alinea el esperado.
        let mut expected_output = case.output.clone();
        with_current_hint(&mut expected_output);
        let ok = match (&got, &case.error) {
            (Ok(value), None) => value == &expected_output,
            (Err(message), Some(expected)) => message == expected,
            _ => false,
        };
        if !ok {
            failures.push(format!(
                "{}: {got:?} != {:?} / {:?}",
                case.case, case.output, case.error
            ));
        }
        cases_run += 1;
    }
    assert!(
        failures.is_empty(),
        "{} fallos:\n{}",
        failures.len(),
        failures.join("\n")
    );
    let cases_in_file = cases.len();
    assert_eq!(cases_run, cases_in_file, "se saltó algún caso");
    assert!(cases_in_file >= 31, "faltan casos en el fixture");
}

#[test]
fn voice_flow_hint_includes_the_short_ack_rule() {
    assert!(VOICE_FLOW_HINT.starts_with(PY_VOICE_FLOW_HINT));
    assert!(VOICE_FLOW_HINT.contains("# Acuses cortos"));
    for reply in [
        "Listo, quedo atento.",
        "Me avisas si necesitas algo.",
        "Entendido.",
    ] {
        assert!(VOICE_FLOW_HINT.contains(reply), "{reply}");
    }
    assert!(VOICE_FLOW_HINT.contains("No repitas el aviso"));
}

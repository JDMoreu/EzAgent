use super::super::handoff::hex_id;
use super::super::*;
use serde_json::json;

#[test]
fn json_command_traduce_cada_fallo() {
    let completed = |stdout: &str, stderr: &str| process::Completed {
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        returncode: 0,
    };
    let error = parse_json_output("hc", &completed("", "boom")).expect_err("fallo");
    assert_eq!(error.message(), "boom");
    let error = parse_json_output("hc", &completed("  \n", "")).expect_err("vacío");
    assert_eq!(error.message(), "hc no devolvió nada");
    let error = parse_json_output("hc", &completed("no json", "")).expect_err("json");
    assert_eq!(error.message(), "hc devolvió algo que no es JSON: no json");
    let error = parse_json_output(
        "hc",
        &completed(
            r#"{"error": {"kind": "not_found", "message": "no existe"}}"#,
            "",
        ),
    )
    .expect_err("not found");
    assert!(error.is_not_found());
    assert_eq!(error.message(), "no existe");
    let error = parse_json_output("hc", &completed(r#"{"error": "fallo"}"#, "")).expect_err("err");
    assert!(!error.is_not_found());
    assert_eq!(error.message(), "fallo");
    let error = parse_json_output("hc", &completed(r#"{"error": {}}"#, "")).expect("error vacío");
    assert_eq!(error["error"], json!({}));
    let error = parse_json_output("hc", &completed(r#"{"error": {"kind": "otro"}}"#, ""))
        .expect_err("sin mensaje");
    assert_eq!(error.message(), "None");
    let value =
        parse_json_output("hc", &completed(r#"{"error": "", "ok": true}"#, "")).expect("sin error");
    assert_eq!(value["ok"], true);
}

#[test]
fn errores_de_herdr_muestran_su_mensaje() {
    let error = HerdrError::failed("fallo");
    assert_eq!(error.to_string(), "fallo");
    assert!(!error.is_not_found());
    let error = HerdrError::not_found("no está");
    assert!(error.is_not_found());
    assert_eq!(error.message(), "no está");
}

#[test]
fn encode_de_powershell_es_base64_utf16le() {
    assert_eq!(utf16le_base64(""), "");
    assert_eq!(utf16le_base64("abc"), "YQBiAGMA");
    assert_eq!(utf16le_base64("á"), "4QA=");
    let argv = powershell_script("Write-Output hola");
    assert_eq!(argv[0], "powershell.exe");
    assert_eq!(argv[4], "-EncodedCommand");
    assert_eq!(argv.len(), 6);
    assert!(argv[5]
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c)));
}

#[test]
fn hex_id_es_corto_y_unico() {
    let first = hex_id();
    assert_eq!(first.chars().count(), 8);
    assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
    let all: std::collections::HashSet<String> = (0..100).map(|_| hex_id()).collect();
    assert_eq!(all.len(), 100);
}

#[test]
fn decode_traduce_los_finales_de_linea() {
    assert_eq!(process::decode(b"a\r\nb\rc\n"), "a\nb\nc\n");
    assert_eq!(process::decode(b"caf\xc3\xa9"), "café");
    assert_eq!(process::decode(b"roto \xff"), "roto \u{fffd}");
}

#[test]
fn which_encuentra_rutas_explicitas() {
    let folder = std::env::temp_dir().join(format!(
        "ez-herdr-which-{}-{}",
        std::process::id(),
        hex_id()
    ));
    std::fs::create_dir_all(&folder).expect("carpeta");
    let file = folder.join("programa.exe");
    std::fs::write(&file, b"x").expect("archivo");
    assert_eq!(process::which(&file.to_string_lossy()), Some(file.clone()));
    assert_eq!(process::which(&folder.to_string_lossy()), None);
    assert_eq!(
        process::which(&folder.join("no-existe.exe").to_string_lossy()),
        None
    );
    assert_eq!(process::which(""), None);
    assert_eq!(process::which("ezagent-no-existe-en-path"), None);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn run_informa_de_un_ejecutable_que_falta() {
    let argv = vec!["ezagent-no-existe.exe".to_string(), "--x".to_string()];
    assert!(matches!(
        process::run(&argv, 5.0, false),
        Err(process::RunError::NotFound)
    ));
    assert!(matches!(
        process::run(&[], 5.0, false),
        Err(process::RunError::Os(_))
    ));
}

#[cfg(windows)]
#[test]
fn run_captura_salida_y_codigo() {
    let argv = powershell_script("Write-Output hola; exit 3");
    let result = process::run(&argv, 30.0, false).expect("powershell");
    assert_eq!(result.stdout.trim(), "hola");
    assert_eq!(result.returncode, 3);
}

#[cfg(windows)]
#[test]
fn run_corta_por_timeout() {
    let argv = powershell_script("Start-Sleep -Seconds 5");
    assert!(matches!(
        process::run(&argv, 0.2, false),
        Err(process::RunError::Timeout)
    ));
}

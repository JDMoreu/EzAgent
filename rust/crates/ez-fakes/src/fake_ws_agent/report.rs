//! Informe del subagente falso y su entrega con `ez report` (ECO-8).

use std::path::Path;

use super::process;

const BODY: &str =
    "## Veredicto\npass\n## Hallazgos\n## Evidencia\nfake-ws-agent terminó sin hallazgos.\n";

/// Escribe el informe en la ruta que traen las instrucciones.
pub(crate) fn write(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("no pude crear {}: {error}", parent.display()))?;
    }
    std::fs::write(path, BODY)
        .map_err(|error| format!("no pude escribir {}: {error}", path.display()))
}

/// Argumentos del comando `ez report` con el veredicto siempre limpio.
pub(crate) fn command(path: &Path, mutations: bool) -> Vec<String> {
    let mut args = vec![
        "report".to_string(),
        "--verdict".to_string(),
        "pass".to_string(),
        "--file".to_string(),
        path.to_string_lossy().into_owned(),
        "--blocking".to_string(),
        "0".to_string(),
        "--major".to_string(),
        "0".to_string(),
        "--minor".to_string(),
        "0".to_string(),
    ];
    if mutations {
        args.extend([
            "--mutations-tried".to_string(),
            "3".to_string(),
            "--mutations-caught".to_string(),
            "3".to_string(),
        ]);
    }
    args
}

/// Ejecuta `ez report` y falla si el árbitro rechaza el informe.
pub(crate) fn send(path: &Path, mutations: bool) -> Result<(), String> {
    let args = command(path, mutations);
    let output = process::ez()
        .args(&args)
        .output()
        .map_err(|error| format!("no pude ejecutar ez report: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let reason = String::from_utf8_lossy(&output.stderr);
    Err(format!(
        "ez report fue rechazado: {}",
        reason.trim().replace('\n', " | ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_has_the_mandatory_counts() {
        let args = command(Path::new("informe.md"), false);
        assert_eq!(
            args,
            [
                "report",
                "--verdict",
                "pass",
                "--file",
                "informe.md",
                "--blocking",
                "0",
                "--major",
                "0",
                "--minor",
                "0"
            ]
        );
    }

    #[test]
    fn mutations_add_the_two_counts() {
        let args = command(Path::new("informe.md"), true);
        assert_eq!(
            &args[11..],
            ["--mutations-tried", "3", "--mutations-caught", "3"]
        );
    }

    #[test]
    fn writes_the_report_with_its_sections() {
        let dir = std::env::temp_dir().join(format!("ez-fakes-report-{}", std::process::id()));
        let path = dir.join("ronda-1").join("qa-r1.informe.md");
        let _ = std::fs::remove_dir_all(&dir);
        write(&path).expect("escribir");
        let text = std::fs::read_to_string(&path).expect("leer");
        assert!(text.contains("## Veredicto\npass"));
        assert!(text.contains("## Hallazgos"));
        assert!(text.contains("## Evidencia"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

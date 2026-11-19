//! Informe de autocomprobación de la vista y su evaluación (brief ECO-5b).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Prefijo que debe tener la cabecera del agente principal en la demostración.
pub const HEADER_PREFIX: &str = "Agente principal · ";

/// Lo que la vista informa 6 s después de cargar en modo `--selfcheck`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelfcheckReport {
    /// Errores de JavaScript capturados desde el primer momento.
    pub errores: Vec<String>,
    /// Texto de la cabecera del agente principal.
    pub cabecera: String,
    /// Terminales xterm que recibieron al menos un bloque de bytes del PTY.
    pub terminales: usize,
    /// Tarjetas de subagente pintadas.
    pub peces: usize,
    /// Eventos de traza pintados.
    pub eventos: usize,
    /// Tarjetas de aprobación pintadas.
    pub tarjetas: usize,
    /// Si el formulario «Nuevo espacio» sigue abierto.
    pub formulario_abierto: bool,
}

/// Motivos por los que un informe no supera la autocomprobación.
pub fn failures(report: &SelfcheckReport) -> Vec<&'static str> {
    let mut out = Vec::new();
    if !report.errores.is_empty() {
        out.push("la vista registró errores de JavaScript");
    }
    if !report.cabecera.starts_with(HEADER_PREFIX) {
        out.push("la cabecera del agente principal no es la esperada");
    }
    if report.terminales != 3 {
        out.push("no se montaron 3 terminales con datos");
    }
    if report.peces != 2 {
        out.push("no se pintaron 2 subagentes");
    }
    if report.eventos != 5 {
        out.push("no se pintaron 5 eventos de traza");
    }
    if report.tarjetas != 1 {
        out.push("no se pintó 1 tarjeta de aprobación");
    }
    if report.formulario_abierto {
        out.push("el formulario «Nuevo espacio» está abierto");
    }
    out
}

/// Evalúa el informe sin efectos secundarios: `true` solo si todo coincide.
pub fn evaluate(report: &SelfcheckReport) -> bool {
    failures(report).is_empty()
}

/// Escribe cualquier valor como JSON legible en la ruta dada.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, text).map_err(|error| error.to_string())
}

/// Informe de la vista cuando no llegó a avisar en el plazo.
pub fn write_timeout(path: &Path) -> Result<(), String> {
    write_json(
        path,
        &serde_json::json!({ "errores": ["la vista no informó"] }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn good() -> SelfcheckReport {
        SelfcheckReport {
            cabecera: format!("{HEADER_PREFIX}PowerShell"),
            terminales: 3,
            peces: 2,
            eventos: 5,
            tarjetas: 1,
            ..SelfcheckReport::default()
        }
    }

    #[test]
    fn a_good_report_passes() {
        let report = good();
        assert!(evaluate(&report));
        assert!(failures(&report).is_empty());
    }

    #[test]
    fn javascript_errors_fail_the_check() {
        let mut report = good();
        report
            .errores
            .push("TypeError:Cannot read properties of undefined".to_string());
        assert!(!evaluate(&report));
        assert_eq!(failures(&report).len(), 1);
    }

    #[test]
    fn wrong_counts_fail_the_check() {
        let mut report = good();
        report.terminales = 1;
        report.peces = 0;
        report.eventos = 4;
        report.tarjetas = 2;
        let problems = failures(&report);
        assert_eq!(problems.len(), 4);
        assert!(!evaluate(&report));
    }

    #[test]
    fn wrong_header_and_open_form_fail_the_check() {
        let mut report = good();
        report.cabecera = "Agente principal".to_string();
        report.formulario_abierto = true;
        assert_eq!(failures(&report).len(), 2);
    }

    #[test]
    fn the_report_is_written_with_all_its_fields() {
        let dir = TempDir::new("selfcheck-write");
        let path = dir.path().join("informe.json");
        let report = good();
        write_json(&path, &report).expect("escribir informe");
        let text = fs::read_to_string(&path).expect("leer informe");
        let parsed: SelfcheckReport = serde_json::from_str(&text).expect("json válido");
        assert_eq!(parsed, report);
        for key in [
            "errores",
            "cabecera",
            "terminales",
            "peces",
            "eventos",
            "tarjetas",
            "formulario_abierto",
        ] {
            assert!(text.contains(key), "falta {key} en {text}");
        }
    }

    #[test]
    fn the_timeout_report_has_only_the_error() {
        let dir = TempDir::new("selfcheck-timeout");
        let path = dir.path().join("informe.json");
        write_timeout(&path).expect("escribir tiempo agotado");
        let text = fs::read_to_string(&path).expect("leer informe");
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("json válido");
        assert_eq!(parsed["errores"][0], "la vista no informó");
    }
}

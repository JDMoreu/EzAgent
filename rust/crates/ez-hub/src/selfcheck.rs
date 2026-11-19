//! Informe de autocomprobación de la vista y su evaluación (brief ECO-6b).

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Lo que la vista informa 6 s después de cargar en modo `--selfcheck`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelfcheckReport {
    /// Errores de JavaScript capturados desde el primer momento.
    pub errores: Vec<String>,
    /// Nodos del árbol pintados, incluido el espacio (7 en la demostración).
    pub nodos: usize,
    /// Pestañas de terminal abiertas (2 en la demostración).
    #[serde(rename = "pestañas")]
    pub pestanas: usize,
    /// Terminales que recibieron al menos un bloque de bytes del PTY.
    #[serde(rename = "terminales_con_datos")]
    pub terminales_con_datos: usize,
    /// Eventos de traza pintados (6 en la demostración).
    pub eventos: usize,
    /// Tarjetas pintadas (1 en la demostración).
    pub tarjetas: usize,
    /// Apariciones de «elefante», «pez» o «peces» en el texto visible.
    #[serde(rename = "textos_prohibidos")]
    pub textos_prohibidos: usize,
    /// `clientWidth` mínimo de los textos de la traza, en px.
    #[serde(rename = "traza_ancho_min")]
    pub traza_ancho_min: f64,
    /// Ancho del panel de traza, en px.
    #[serde(rename = "panel_ancho")]
    pub panel_ancho: f64,
    /// Etiquetas de las pestañas abiertas, en orden.
    #[serde(rename = "etiquetas_pestañas")]
    pub etiquetas_pestanas: Vec<String>,
    /// Alto del área de la terminal menos el alto de la pantalla de xterm, en px.
    #[serde(rename = "banda_terminal_px")]
    pub banda_terminal_px: f64,
    /// Alto de una celda de xterm, en px. Mide la banda permitida sin constantes.
    #[serde(rename = "alto_celda_px")]
    pub alto_celda_px: f64,
}

/// Etiquetas esperadas de las dos pestañas que abre el modo demostración:
/// el nombre del primer agente principal y el id de su primer subagente.
fn expected_tab_labels() -> Vec<String> {
    let tree = crate::demo::tree();
    let main = tree.nodes.first().expect("el demo tiene un principal");
    let child = main
        .children
        .first()
        .expect("el principal tiene un subagente");
    vec![main.name.clone(), child.id.clone()]
}

/// Motivos por los que un informe no supera la autocomprobación.
pub fn failures(report: &SelfcheckReport) -> Vec<&'static str> {
    let mut out = Vec::new();
    if !report.errores.is_empty() {
        out.push("la vista registró errores de JavaScript");
    }
    if report.nodos != 7 {
        out.push("el árbol no tiene 7 nodos");
    }
    if report.pestanas != 2 {
        out.push("no hay 2 pestañas abiertas");
    }
    if report.terminales_con_datos != 2 {
        out.push("no hay 2 terminales con datos");
    }
    if report.eventos != 6 {
        out.push("no se pintaron 6 eventos de traza");
    }
    if report.tarjetas != 1 {
        out.push("no se pintó 1 tarjeta");
    }
    if report.textos_prohibidos != 0 {
        out.push("hay textos prohibidos visibles");
    }
    if report.traza_ancho_min < 0.8 * report.panel_ancho - 40.0 {
        out.push("el texto de la traza no ocupa el ancho del panel");
    }
    if report.etiquetas_pestanas != expected_tab_labels() {
        out.push("las etiquetas de las pestañas no son las del demo");
    }
    if report.alto_celda_px <= 0.0 || report.banda_terminal_px >= report.alto_celda_px {
        out.push("la terminal deja una franja inferior");
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
            nodos: 7,
            pestanas: 2,
            terminales_con_datos: 2,
            eventos: 6,
            tarjetas: 1,
            traza_ancho_min: 320.0,
            panel_ancho: 320.0,
            etiquetas_pestanas: expected_tab_labels(),
            banda_terminal_px: 2.0,
            alto_celda_px: 12.0,
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
        let bad = SelfcheckReport {
            nodos: 6,
            pestanas: 1,
            terminales_con_datos: 1,
            eventos: 5,
            tarjetas: 2,
            textos_prohibidos: 3,
            ..good()
        };
        assert_eq!(failures(&bad).len(), 6);
        assert!(!evaluate(&bad));
    }

    #[test]
    fn the_report_is_written_with_all_its_fields() {
        let dir = TempDir::new("selfcheck-write");
        let path = dir.path().join("informe.json");
        let mut report = good();
        report.errores.push("aviso".to_string());
        write_json(&path, &report).expect("escribir informe");
        let text = fs::read_to_string(&path).expect("leer informe");
        let parsed: SelfcheckReport = serde_json::from_str(&text).expect("json válido");
        assert_eq!(parsed, report);
        for key in [
            "errores",
            "nodos",
            "pestañas",
            "terminales_con_datos",
            "eventos",
            "tarjetas",
            "textos_prohibidos",
            "traza_ancho_min",
            "panel_ancho",
            "etiquetas_pestañas",
            "banda_terminal_px",
            "alto_celda_px",
        ] {
            assert!(text.contains(key), "falta {key} en {text}");
        }
    }

    #[test]
    fn the_visual_measurements_fail_the_check() {
        let narrow = SelfcheckReport {
            traza_ancho_min: 40.0,
            panel_ancho: 320.0,
            ..good()
        };
        assert!(!evaluate(&narrow));
        let wrong_labels = SelfcheckReport {
            etiquetas_pestanas: vec!["Desarrollador".to_string(), "Desarrollador".to_string()],
            ..good()
        };
        assert!(!evaluate(&wrong_labels));
        let band = SelfcheckReport {
            banda_terminal_px: 20.0,
            alto_celda_px: 12.0,
            ..good()
        };
        assert!(!evaluate(&band));
        assert_eq!(expected_tab_labels(), ["Arquitecto", "dev-r1-1"]);
    }

    #[test]
    fn the_timeout_report_has_only_the_error() {
        let dir = TempDir::new("selfcheck-timeout");
        let path = dir.path().join("informe.json");
        write_timeout(&path).expect("escribir tiempo agotado");
        let text = fs::read_to_string(&path).expect("leer informe");
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("json válido");
        assert_eq!(parsed["errores"][0], "la vista no informó");
        assert!(!evaluate(&serde_json::from_str(&text).expect("informe")));
    }
}

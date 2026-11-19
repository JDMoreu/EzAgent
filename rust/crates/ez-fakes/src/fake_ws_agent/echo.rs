//! Modo `--echo`: el agente falso repite lo que recibe y guarda el registro.
//!
//! Es el agente de la prueba de punta a punta (HD-6): no usa modelos, solo
//! anota el argv, el entorno y cada línea que le llega, para que la prueba
//! compruebe qué se le entregó a cada tipo de agente.
//!
//! Con `--report` el agente falso también entrega su estado con
//! `ez report --state …` (§13 del diseño).

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Map, Value};

/// El cuerpo del informe que deja el agente falso.
const REPORT_BODY: &str = "# Informe\n\nEl agente falso terminó su tarea.\n";

/// Las variables que el registro guarda (solo las que estén presentes).
const ENV_VARS: [&str; 5] = [
    "OPENCODE_CONFIG_CONTENT",
    "XDG_STATE_HOME",
    "HC_TERMINAL_ID",
    "MINIMAX_DATA_DIR",
    "CODEX_HOME",
];

/// Lo que el agente falso declara al terminar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReportMode {
    /// No declara nada (por omisión).
    #[default]
    None,
    /// Declara que terminó.
    Done,
    /// Declara que falló.
    Failed,
}

impl ReportMode {
    /// El modo según su nombre (`terminado`, `fallido` o `none`).
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_lowercase().as_str() {
            "none" => Some(Self::None),
            "terminado" => Some(Self::Done),
            "fallido" => Some(Self::Failed),
            _ => None,
        }
    }

    /// El estado que se declara (`--state`); `None` si no declara nada.
    pub fn state(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Done => Some("terminado"),
            Self::Failed => Some("fallido"),
        }
    }
}

/// Lo que el modo eco necesita: dónde guardar el registro, qué declarar y los
/// argumentos libres que recibió (los flags del tipo de agente).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Echo {
    /// La carpeta donde queda el registro.
    pub folder: PathBuf,
    /// Lo que declara al terminar.
    pub report: ReportMode,
    /// La ruta de `ez` (por omisión, el que esté en el PATH).
    pub ez: Option<PathBuf>,
    /// Los argumentos libres (los que van después de la carpeta).
    pub extra: Vec<String>,
}

/// Analiza `--echo <carpeta> [--report estado] [--ez ruta] [argumentos…]`.
pub fn parse(args: &[String]) -> Result<Echo, String> {
    let Some(folder) = args.first() else {
        return Err("--echo necesita la carpeta donde guardar el registro.".to_string());
    };
    let mut rest = args.get(1..).unwrap_or(&[]);
    let mut report = ReportMode::None;
    let mut ez = None;
    loop {
        match rest.first().map(String::as_str) {
            Some("--report") => {
                let value = rest
                    .get(1)
                    .ok_or_else(|| "--report necesita el estado.".to_string())?;
                report = ReportMode::parse(value).ok_or_else(|| {
                    format!("--report debe ser terminado, fallido o none (no «{value}»).")
                })?;
                rest = rest.get(2..).unwrap_or(&[]);
            }
            Some("--ez") => {
                let value = rest
                    .get(1)
                    .ok_or_else(|| "--ez necesita la ruta de ez.".to_string())?;
                ez = Some(PathBuf::from(value));
                rest = rest.get(2..).unwrap_or(&[]);
            }
            _ => break,
        }
    }
    Ok(Echo {
        folder: PathBuf::from(folder),
        report,
        ez,
        extra: rest.to_vec(),
    })
}

/// Lo que el agente falso guarda: lo que le pasaron y lo que recibió.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    /// Los argumentos con los que arrancó.
    pub argv: Vec<String>,
    /// La carpeta desde la que arrancó.
    pub cwd: String,
    /// Las variables de entorno que importan (solo las que hay).
    pub env: Map<String, Value>,
    /// Las líneas que recibió.
    pub lines: Vec<String>,
}

impl Record {
    /// El registro de este proceso.
    pub fn current(argv: Vec<String>) -> Self {
        let mut env = Map::new();
        for name in ENV_VARS {
            if let Ok(value) = std::env::var(name) {
                env.insert(name.to_string(), Value::String(value));
            }
        }
        let cwd = std::env::current_dir()
            .map(|folder| folder.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            argv,
            cwd,
            env,
            lines: Vec::new(),
        }
    }

    /// El JSON del registro.
    pub fn to_value(&self) -> Value {
        json!({
            "argv": self.argv,
            "cwd": self.cwd,
            "env": Value::Object(self.env.clone()),
            "lines": self.lines,
        })
    }

    /// Escribe el registro entero (archivo temporal y renombrar).
    pub fn write(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.to_value())
            .map_err(|error| format!("no pude preparar el registro: {error}"))?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text)
            .map_err(|error| format!("no pude escribir {}: {error}", temporary.display()))?;
        std::fs::rename(&temporary, path)
            .map_err(|error| format!("no pude renombrar {}: {error}", path.display()))
    }
}

/// La ruta del registro: `<carpeta>/<HC_TERMINAL_ID o pid>.json`.
pub fn record_path(folder: &Path) -> PathBuf {
    folder.join(format!("{}.json", identifier()))
}

/// El identificador del agente falso: el de su terminal o el del proceso.
pub fn identifier() -> String {
    std::env::var("HC_TERMINAL_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| std::process::id().to_string())
}

/// El bucle del modo eco: anota, repite y vuelve a esperar.
pub fn run(echo: &Echo) -> Result<(), String> {
    std::fs::create_dir_all(&echo.folder)
        .map_err(|error| format!("no pude crear {}: {error}", echo.folder.display()))?;
    let mut record = Record::current(std::env::args().collect());
    let path = record_path(&echo.folder);
    record.write(&path)?;
    super::announce("idle")?;
    super::prompt()?;
    while let Some(line) = super::next_line()? {
        let line = line.trim_end().to_string();
        super::announce("working")?;
        record.lines.push(line.clone());
        record.write(&path)?;
        // DECISION: el estado se declara con la primera línea: es cuando el
        // subagente ya recibió el encargo.
        if record.lines.len() == 1 {
            declare(echo, &line)?;
        }
        write_line(&format!("recibido: {line}"))?;
        super::announce("idle")?;
        super::prompt()?;
    }
    Ok(())
}

/// Escribe el informe y lo declara con `ez report` (sin shell).
fn declare(echo: &Echo, line: &str) -> Result<(), String> {
    let Some(state) = echo.report.state() else {
        return Ok(());
    };
    let report = echo.folder.join(format!("{}-informe.md", identifier()));
    std::fs::write(&report, REPORT_BODY)
        .map_err(|error| format!("no pude escribir {}: {error}", report.display()))?;
    let summary = format!("eco {}", mark(line));
    let program = echo.ez.clone().unwrap_or_else(|| PathBuf::from("ez"));
    let output = ez_command(&program)
        .args(report_args(state, &report, &summary))
        .output()
        .map_err(|error| format!("no pude ejecutar {} report: {error}", program.display()))?;
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "ez report fue rechazado: {}",
            reason.trim().replace('\n', " | ")
        ));
    }
    Ok(())
}

/// Los argumentos de `ez report` que declara el estado.
pub fn report_args(state: &str, report: &Path, summary: &str) -> Vec<String> {
    vec![
        "report".to_string(),
        "--state".to_string(),
        state.to_string(),
        "--file".to_string(),
        report.to_string_lossy().into_owned(),
        "--summary".to_string(),
        summary.to_string(),
    ]
}

/// La primera marca `E2E-…` de la línea; si no hay, «sin marca».
fn mark(text: &str) -> String {
    let Some(index) = text.find("E2E-") else {
        return "sin marca".to_string();
    };
    let rest = &text[index..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    rest[..end].to_string()
}

/// El comando `ez` listo para recibir argumentos en lista (sin shell).
fn ez_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Escribe una línea en stdout.
fn write_line(text: &str) -> Result<(), String> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    out.write_all(text.as_bytes())
        .and_then(|()| out.write_all(b"\n"))
        .and_then(|()| out.flush())
        .map_err(|error| format!("no pude escribir en stdout: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn el_modo_eco_toma_la_carpeta_y_deja_los_argumentos_libres() {
        let echo = parse(&args(&["registros", "--model", "x", "--standalone"])).expect("análisis");
        assert_eq!(echo.folder, PathBuf::from("registros"));
        assert_eq!(echo.extra, args(&["--model", "x", "--standalone"]));
        assert_eq!(echo.report, ReportMode::None);
        assert_eq!(echo.ez, None);
    }

    #[test]
    fn el_modo_eco_toma_el_informe_y_la_ruta_de_ez() {
        let echo = parse(&args(&[
            "registros",
            "--report",
            "terminado",
            "--ez",
            "ez.exe",
            "--standalone",
        ]))
        .expect("análisis");
        assert_eq!(echo.report, ReportMode::Done);
        assert_eq!(echo.ez, Some(PathBuf::from("ez.exe")));
        assert_eq!(echo.extra, args(&["--standalone"]));
    }

    #[test]
    fn el_modo_eco_rechaza_un_estado_desconocido() {
        let error = parse(&args(&["registros", "--report", "casi"]));
        assert!(error.is_err(), "{error:?}");
    }

    #[test]
    fn el_comando_del_informe_arma_el_report() {
        let args = report_args(
            "terminado",
            Path::new("registros/t12-informe.md"),
            "eco E2E-1",
        );
        assert_eq!(
            args,
            [
                "report",
                "--state",
                "terminado",
                "--file",
                "registros/t12-informe.md",
                "--summary",
                "eco E2E-1"
            ]
        );
    }

    #[test]
    fn la_marca_toma_la_primera_e2e() {
        assert_eq!(mark("trabaja E2E-7 algo"), "E2E-7");
        assert_eq!(mark("nada que ver"), "sin marca");
    }

    #[test]
    fn sin_informe_no_se_declada_nada() {
        let folder = std::env::temp_dir().join(format!("ez-fake-echo-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let echo = Echo {
            folder: folder.clone(),
            report: ReportMode::None,
            ez: None,
            extra: Vec::new(),
        };
        declare(&echo, "hola").expect("sin declarar");
        assert!(!folder.exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn el_modo_eco_sin_carpeta_es_un_error() {
        assert!(parse(&args(&[])).is_err());
    }

    #[test]
    fn el_registro_guarda_el_argv_el_entorno_y_las_lineas() {
        let mut record = Record {
            argv: args(&["fake-ws-agent", "--echo", "registros"]),
            cwd: "C:\\ws".to_string(),
            env: Map::new(),
            lines: Vec::new(),
        };
        record.env.insert(
            "HC_TERMINAL_ID".to_string(),
            Value::String("t6h".to_string()),
        );
        record.lines.push("hola".to_string());
        let value = record.to_value();
        assert_eq!(value["argv"][0], "fake-ws-agent");
        assert_eq!(value["cwd"], "C:\\ws");
        assert_eq!(value["env"]["HC_TERMINAL_ID"], "t6h");
        assert_eq!(value["lines"][0], "hola");
    }

    #[test]
    fn el_registro_se_escribe_todo_y_se_renombra() {
        let folder = std::env::temp_dir().join(format!("ez-fake-echo-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("carpeta");
        let path = folder.join("t6h.json");
        let mut record = Record {
            argv: args(&["fake-ws-agent"]),
            cwd: "C:\\ws".to_string(),
            env: Map::new(),
            lines: vec!["primera".to_string()],
        };
        record.write(&path).expect("registro");
        record.lines.push("segunda".to_string());
        record.write(&path).expect("registro");
        let text = std::fs::read_to_string(&path).expect("leer");
        let value: Value = serde_json::from_str(&text).expect("json");
        assert_eq!(value["lines"].as_array().expect("líneas").len(), 2);
        assert_eq!(value["lines"][1], "segunda");
        assert!(!folder.join("t6h.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&folder);
    }
}

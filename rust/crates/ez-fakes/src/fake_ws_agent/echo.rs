//! Modo `--echo`: el agente falso repite lo que recibe y guarda el registro.
//!
//! Es el agente de la prueba de punta a punta (HD-6): no usa modelos, solo
//! anota el argv, el entorno y cada línea que le llega, para que la prueba
//! compruebe qué se le entregó a cada tipo de agente.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

/// Las variables que el registro guarda (solo las que estén presentes).
const ENV_VARS: [&str; 5] = [
    "OPENCODE_CONFIG_CONTENT",
    "XDG_STATE_HOME",
    "HC_TERMINAL_ID",
    "MINIMAX_DATA_DIR",
    "CODEX_HOME",
];

/// Lo que el modo eco necesita: dónde guardar el registro y los argumentos
/// libres que recibió (los flags del tipo de agente).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Echo {
    /// La carpeta donde queda el registro.
    pub folder: PathBuf,
    /// Los argumentos libres (los que van después de la carpeta).
    pub extra: Vec<String>,
}

/// Analiza `--echo <carpeta> [argumentos…]`.
pub fn parse(args: &[String]) -> Result<Echo, String> {
    let Some(folder) = args.first() else {
        return Err("--echo necesita la carpeta donde guardar el registro.".to_string());
    };
    Ok(Echo {
        folder: PathBuf::from(folder),
        extra: args.get(1..).unwrap_or(&[]).to_vec(),
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
    let name = std::env::var("HC_TERMINAL_ID")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| std::process::id().to_string());
    folder.join(format!("{name}.json"))
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
        write_line(&format!("recibido: {line}"))?;
        super::announce("idle")?;
        super::prompt()?;
    }
    Ok(())
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

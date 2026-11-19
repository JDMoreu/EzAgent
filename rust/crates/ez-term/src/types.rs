//! Tipos del esquema `hc` 0.1.21 usados por EzAgent.
//!
//! Todos los campos llevan `#[serde(default)]`: `hc` agrega campos nuevos entre
//! versiones y aquí se ignoran en vez de romper la lectura.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Una terminal de `hc`, viva o terminada (`objeto Terminal` del esquema).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terminal {
    /// Dirección estable en el núcleo (`t7`).
    #[serde(default)]
    pub address: String,
    /// Nombre visible, único entre terminales vivas.
    #[serde(default)]
    pub name: Option<String>,
    /// Programa y argumentos con que se lanzó.
    #[serde(default)]
    pub argv: Vec<String>,
    /// Carpeta de trabajo.
    #[serde(default)]
    pub cwd: String,
    /// Etiquetas libres (`key=value`).
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// `live` o `finished`.
    #[serde(default)]
    pub state: String,
    /// Lo que el programa dice estar haciendo (registro OSC 7501).
    #[serde(default)]
    pub status: Option<Status>,
    /// Código de salida cuando terminó.
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// Por qué terminó (`exited`, `killed`, `holder_lost`, ...).
    #[serde(default)]
    pub reason: Option<String>,
    /// Terminal que la lanzó, si tuvo padre.
    #[serde(default)]
    pub parent: Option<String>,
    /// Identificador del proceso en el sistema.
    #[serde(default)]
    pub pid: Option<u32>,
    /// Ancho en columnas.
    #[serde(default)]
    pub cols: u16,
    /// Alto en filas.
    #[serde(default)]
    pub rows: u16,
    /// Sube cada vez que el estado sale de `working`.
    #[serde(default)]
    pub turn: u64,
}

/// Un registro 7501: el estado que el programa reporta de sí mismo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// `idle`, `working`, `done`, `blocked` o `error`.
    #[serde(default)]
    pub state: String,
    /// Solo con `blocked`: `permission`, `question` o `auth`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Mensaje para una persona.
    #[serde(default)]
    pub msg: String,
    /// Título de la tarea en curso.
    #[serde(default)]
    pub title: String,
    /// Aplicación que reporta.
    #[serde(default)]
    pub app: String,
    /// Avance de 0 a 100 cuando el programa lo informa.
    #[serde(default)]
    pub progress: Option<u8>,
}

/// Lo que se pide a `terminal.run`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSpec {
    /// Programa y argumentos; sin shell.
    pub argv: Vec<String>,
    /// Carpeta de trabajo; si falta, la del núcleo de `hc`.
    pub cwd: Option<PathBuf>,
    /// Nombre de 1 a 32 caracteres (`a-z 0-9 - _ .`).
    pub name: Option<String>,
    /// Etiquetas con que se encontrará la terminal.
    pub labels: BTreeMap<String, String>,
    /// Variables de entorno que se agregan a las del núcleo.
    pub env: BTreeMap<String, String>,
    /// Tamaño inicial: `(columnas, filas)`.
    pub size: Option<(u16, u16)>,
    /// No colgar de la terminal que lanza (`no_parent`), por omisión `true`.
    pub no_parent: bool,
    /// No escribir notas en la terminal padre (`no_notify`), por omisión `true`.
    pub no_notify: bool,
    /// Terminal padre; con `Some`, el hijo cuelga de ella aunque `no_parent` sea `true`.
    pub parent: Option<String>,
    /// Que el hijo no muera con el padre (`outlive_parent`), por omisión `false`.
    pub outlive_parent: bool,
}

impl Default for RunSpec {
    fn default() -> Self {
        Self {
            argv: Vec::new(),
            cwd: None,
            name: None,
            labels: BTreeMap::new(),
            env: BTreeMap::new(),
            size: None,
            no_parent: true,
            no_notify: true,
            parent: None,
            outlive_parent: false,
        }
    }
}

impl RunSpec {
    /// Parámetros de `terminal.run` según el esquema.
    pub(crate) fn params(&self) -> Value {
        let mut params = Map::new();
        params.insert("argv".into(), json!(self.argv));
        params.insert("labels".into(), json!(self.labels));
        params.insert("env".into(), json!(self.env));
        // DECISION: con `parent`, `hc` exige `no_parent: false` para colgar el hijo.
        let no_parent = self.parent.is_none() && self.no_parent;
        params.insert("no_parent".into(), json!(no_parent));
        params.insert("no_notify".into(), json!(self.no_notify));
        params.insert("outlive_parent".into(), json!(self.outlive_parent));
        if let Some(parent) = &self.parent {
            params.insert("parent".into(), json!(parent));
        }
        if let Some(cwd) = &self.cwd {
            params.insert("cwd".into(), json!(cwd.to_string_lossy()));
        }
        if let Some(name) = &self.name {
            params.insert("name".into(), json!(name));
        }
        if let Some((cols, rows)) = self.size {
            params.insert("size".into(), json!({ "cols": cols, "rows": rows }));
        }
        Value::Object(params)
    }
}

/// Texto leído de una terminal (`objeto ReadResult` del esquema).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct ReadText {
    /// Pantalla más las líneas de historial pedidas.
    #[serde(default)]
    pub text: String,
    /// Falso cuando puede faltar parte de lo pedido.
    #[serde(default)]
    pub complete: bool,
}

/// Lo que pasó al enviar texto (`objeto SendResult` del esquema).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SendOutcome {
    /// Turno en que entró el texto.
    #[serde(default)]
    pub turn: u64,
    /// Si el programa pasó a `working` en unos segundos; `None` si no reporta estado.
    #[serde(default)]
    pub started: Option<bool>,
    /// Palabra de estado al entrar el texto; `None` si nunca la dijo.
    #[serde(default)]
    pub status: Option<String>,
}

/// Resultado de entregar la primera línea a un pez recién lanzado (§14.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// El pez pasó a `working`: recibió la primera línea.
    Delivered,
    /// `hc` no informó si arrancó; no se puede confirmar la entrega.
    Unconfirmed,
    /// No se pudo entregar; el texto explica el motivo.
    Failed(String),
}

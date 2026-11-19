//! Tipos públicos del árbitro: quien llama, comandos, launcher y reloj.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ez_team::{InstructionContext, Role, Seat};
use serde::{Deserialize, Serialize};

/// Reloj del árbitro, en segundos desde 1970. Las pruebas usan uno falso.
pub trait Clock: Send + Sync {
    /// Momento actual en segundos desde 1970.
    fn now(&self) -> f64;
}

/// Reloj del sistema.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs_f64())
            .unwrap_or(0.0)
    }
}

/// Lanza y habla con las terminales de los peces (lo implementa `ez-term`).
pub trait FishLauncher: Send + Sync {
    /// Lanza un pez y devuelve la dirección de su terminal en hc (`t7`).
    fn launch(&self, request: &LaunchRequest) -> Result<String, String>;
    /// Envía texto a la terminal de un pez.
    fn send(&self, terminal: &str, text: &str) -> Result<(), String>;
    /// Indica si la terminal del pez sigue viva.
    fn alive(&self, terminal: &str) -> bool;
}

/// Escribe las instrucciones de un pez desde las plantillas de `ez-team`.
pub trait InstructionWriter: Send + Sync {
    /// Texto completo de `<pez>.instrucciones.md`.
    fn render(&self, context: &InstructionContext) -> Result<String, String>;
    /// Primer mensaje de una sola línea que abre la sesión del pez.
    fn first_line(&self, role: Role, instructions: &Path) -> String;
}

/// Datos para lanzar un pez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    pub fish: String,
    pub hc_name: String,
    pub role: Role,
    pub seat: Seat,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub labels: BTreeMap<String, String>,
    pub first_line: String,
}

/// Espacio de trabajo registrado por la ventana.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub folder: PathBuf,
}

/// Quién ejecuta un comando: el elefante de un espacio o un pez concreto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caller {
    Elephant { space: String },
    Fish { fish: String },
}

/// Comandos del protocolo `ez` que llegan al árbitro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Status,
    TaskNew {
        title: String,
        brief: PathBuf,
        criteria: PathBuf,
    },
    /// El equipo ya viene validado por `ez-team`.
    TeamPropose {
        seats: Vec<Seat>,
        card: String,
    },
    RoundStart,
    FishNew {
        role: Role,
        brief: PathBuf,
    },
    FishSend {
        fish: String,
        file: PathBuf,
    },
    Verify,
    RoundClose,
    TaskClose,
    Pause {
        reason: String,
    },
    Report {
        verdict: Verdict,
        file: PathBuf,
        blocking: u32,
        major: u32,
        minor: u32,
        mutations_tried: Option<u32>,
        mutations_caught: Option<u32>,
    },
}

/// Resultado de un informe de pez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Fail,
}

impl Verdict {
    /// Id que usa el protocolo (`pass`, `fail`).
    pub fn id(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Fail => "fail",
        }
    }
}

/// Resultado correcto de un comando.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub message: String,
    pub data: serde_json::Value,
}

impl Outcome {
    /// Resultado con mensaje y datos vacíos.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            data: serde_json::Value::Null,
        }
    }
}

/// Rechazo por una regla (R1–R11). El texto lo genera el código, nunca el LLM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub rule: &'static str,
    pub message: String,
}

impl Rejection {
    /// Rechazo de una regla con su texto.
    pub fn new(rule: &'static str, message: impl Into<String>) -> Self {
        Self {
            rule,
            message: message.into(),
        }
    }
}

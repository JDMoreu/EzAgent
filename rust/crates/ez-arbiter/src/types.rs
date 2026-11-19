//! Tipos públicos del árbitro: identidad de peces, comandos, launcher y reloj.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ez_team::{InstructionContext, RoleDef, RoleId, Seat};
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

/// Identidad de un agente principal de un espacio (diseño §15.1).
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ElephantKey {
    pub space: String,
    /// `e` + 8 hex; `e00000000` es el agente principal por omisión del espacio.
    pub elephant: String,
}

impl ElephantKey {
    /// Agente principal por omisión: trabaja en la carpeta original del espacio.
    // DECISION: id fijo `e00000000` para que las tareas antiguas y las pruebas
    // puedan resolver al agente principal sin consultar `agentes.json`.
    pub fn default_for(space: &str) -> Self {
        Self {
            space: space.to_string(),
            elephant: "e00000000".to_string(),
        }
    }

    /// Agente principal nuevo, con id `e` + 8 hex de un uuid v4.
    pub fn new(space: &str) -> Self {
        let simple = uuid::Uuid::new_v4().simple().to_string();
        Self {
            space: space.to_string(),
            elephant: format!("e{}", &simple[..8]),
        }
    }

    /// Clave de texto para colas y registros en memoria (no se persiste).
    pub fn storage_key(&self) -> String {
        format!("{}|{}", self.space, self.elephant)
    }
}

/// Un agente principal y su carpeta de trabajo aislada (diseño §15.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainAgent {
    pub key: ElephantKey,
    pub name: String,
    pub worktree: PathBuf,
    pub branch: String,
    pub base_branch: String,
}

/// Identidad completa de un pez (diseño §14.1 y §15.1). Un pez solo actúa
/// mientras su clave coincida entera con un trabajo vigente de una tarea activa.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FishKey {
    pub space: String,
    pub task: String,
    /// Id del agente principal dueño de la tarea.
    #[serde(default)]
    pub elephant: String,
    pub fish: String,
    /// Empieza en 1; sube al relanzar un papel que falló.
    pub attempt: u32,
    /// Id del pez que lo delegó; `None` si lo lanzó el agente principal.
    #[serde(default)]
    pub parent: Option<String>,
}

impl FishKey {
    /// Clave de texto para el registro de tokens en memoria (no se persiste).
    pub fn storage_key(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.space, self.task, self.elephant, self.fish, self.attempt
        )
    }
}

/// Resultado de entregar el encargo a un pez (diseño §14.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// La terminal arrancó y recibió la primera línea.
    Delivered,
    /// No se pudo confirmar el arranque; el trabajo sigue adelante.
    Unconfirmed,
    /// Falló la entrega; el motivo se devuelve al elefante.
    Failed(String),
}

/// Terminal creada y resultado de la entrega.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launched {
    pub terminal: String,
    pub delivery: Delivery,
}

/// Lanza y habla con las terminales de los peces (lo implementa `ez-term`).
pub trait FishLauncher: Send + Sync {
    /// Lanza un pez; devuelve su terminal y si se confirmó la entrega.
    fn launch(&self, request: &LaunchRequest) -> Result<Launched, String>;
    /// Envía texto a la terminal de un pez.
    fn send(&self, terminal: &str, text: &str) -> Result<(), String>;
    /// Indica si la terminal del pez sigue viva.
    fn alive(&self, terminal: &str) -> bool;
    /// Revoca el token de un pez cuando deja de actuar.
    fn revoke(&self, token: &str);
}

/// Escribe las instrucciones de un pez desde las plantillas de `ez-team`.
pub trait InstructionWriter: Send + Sync {
    /// Texto completo de `<pez>.instrucciones.md`.
    fn render(&self, context: &InstructionContext) -> Result<String, String>;
    /// Primer mensaje de una sola línea que abre la sesión del subagente.
    fn first_line(&self, role_name: &str, instructions: &Path) -> String;
}

/// Datos para lanzar un pez.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRequest {
    pub fish: String,
    pub hc_name: String,
    pub role: RoleId,
    pub seat: Seat,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub labels: BTreeMap<String, String>,
    pub first_line: String,
    /// Token que el proceso necesita y que el lanzador registra antes de abrirlo.
    pub token: String,
}

/// Espacio de trabajo registrado por la ventana.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub folder: PathBuf,
}

/// Quién ejecuta un comando: un agente principal o un pez concreto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caller {
    Elephant(ElephantKey),
    Fish(FishKey),
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
    /// El equipo y sus definiciones de papel ya vienen validados por `ez-team`.
    TeamPropose {
        seats: Vec<Seat>,
        roles: Vec<RoleDef>,
        card: String,
    },
    RoundStart,
    FishNew {
        role: RoleId,
        brief: PathBuf,
    },
    FishSend {
        fish: String,
        file: PathBuf,
    },
    /// Un pez con un papel que puede delegar lanza uno de sus hijos (§15.5).
    AgentDelegate {
        role: RoleId,
        brief: PathBuf,
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

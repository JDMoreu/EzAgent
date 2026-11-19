# ECO-4 — `ez-team`: agentes, suscripciones, inventario, equipo y textos de los peces

**Antes de nada lee `rust-migration/briefs/_COMMON-ECO.md` y `docs/designs/espacio-elefante.md`
(sobre todo §2, §6 y §9).**

## Archivos que se tocan (solo estos)
- `rust/crates/ez-team/**`, salvo los **contratos**: `src/roles.rs`, `src/contract.rs` y
  `src/roles/*.md` (plantillas escritas por el elefante; inclúyelas con `include_str!`).
  En `src/lib.rs` solo añades tus módulos y reexportaciones.
- El `Cargo.toml` ya declara `ez-core`, `serde`, `serde_json`, `toml` y `tracing`.

## Módulos y API
- `profiles`: `AgentProfile { id, name, argv, models_cmd: Option<Vec<String>>, models:
  Vec<String>, family: Option<String>, effort_args: Vec<String> }` (en TOML: `id`,
  `nombre`, `argv`, `modelos_cmd`, `modelos`, `familia`, `esfuerzo_args`).
  `pub fn load_or_init(dir: &Path) -> Result<Vec<AgentProfile>, TeamError>`: lee
  `dir/agentes.toml`; si no existe, escribe los valores por omisión de §6 **tal cual**
  (con sus comentarios) y los devuelve.
- `subscriptions`: `Subscription { name, agents }` (`nombre`, `agentes`);
  `pub fn load(dir: &Path) -> Result<Vec<Subscription>, TeamError>` (sin archivo → vacío).
- `family`: `pub fn family(profile: &AgentProfile, model: &str) -> String` con la regla
  de §6 (prueba los cuatro ejemplos de §6 y `hy4-flash-free` → `hy`).
- `inventory`:
  ```rust
  pub trait System: Send + Sync {
      fn var(&self, name: &str) -> Option<String>;          // PATH, PATHEXT
      fn is_file(&self, path: &Path) -> bool;
      fn run(&self, argv: &[String], timeout: Duration) -> Option<String>; // stdout
  }
  pub struct RealSystem;                                     // std::process, sin shell, sin consola
  pub struct Inventory { pub agents: Vec<AvailableAgent> }   // AvailableAgent { profile, path, models, families }
  pub fn detect(profiles: &[AgentProfile], subs: &[Subscription], system: &dyn System) -> Inventory;
  impl Inventory { pub fn text(&self) -> String; pub fn json(&self) -> serde_json::Value; }
  ```
  Disponibilidad y modelos según §6 (tope 20 s para `modelos_cmd`; una línea por modelo,
  sin líneas vacías). Guarda en memoria del proceso la salida de `modelos_cmd` por agente.
  `text()` es lo que imprime `ez inventory`: una sección por agente con su nombre, id y
  modelos agrupados por familia; si no hay ninguno: «No encontré agentes disponibles.
  Revisa agentes.toml y suscripciones.toml.».
- `team`:
  ```rust
  pub fn parse(text: &str) -> Result<Vec<Seat>, TeamError>;          // equipo.toml
  pub struct TeamCheck { pub errors: Vec<String>, pub warnings: Vec<String> }
  pub fn validate(seats: &[Seat], inventory: &Inventory) -> TeamCheck; // reglas de §6
  pub fn card(task_title: &str, seats: &[Seat], check: &TeamCheck, inventory: &Inventory) -> String;
  ```
  `parse`: papel desconocido, `cantidad` 0 o mayor que 1 fuera de `developer`, o papel
  repetido → error con la fila. `card` con el formato exacto de §6 (nombres de papel con
  `Role::label` y del agente con `nombre` del perfil).
- `launch`:
  ```rust
  pub fn argv(profile: &AgentProfile, seat: &Seat) -> Vec<String>;   // {modelo}/{esfuerzo}
  pub fn render_instructions(context: &InstructionContext) -> Result<String, TeamError>;
  pub fn first_line(role: Role, instructions: &Path) -> String;     // texto exacto de §6
  ```
  `render_instructions` = `roles/_comun.md` + línea en blanco + `roles/<papel>.md`, con
  los marcadores `{papel_nombre}` (`Role::label`), `{pez}`, `{ronda}`, `{carpeta}`,
  `{encargo}`, `{criterios}`, `{plan}`, `{encargo_pez}`, `{informe}`, `{informe_qa}`
  (solo validador; error si falta) y `{mutaciones_minimas}`. Error si queda un marcador
  `{…}` sin sustituir en el resultado (las llaves de los bloques de código de las
  plantillas no son marcadores: solo cuentan nombres de esa lista).
- `TeamError` con mensajes en español.

## Pruebas
- `load_or_init` crea el archivo por omisión y lo vuelve a leer igual; TOML inválido → error.
- `detect` con un `System` falso: agente en `PATH` con `PATHEXT`, ruta absoluta, agente
  sin suscripción, sin suscripciones declaradas (cuentan todos), `modelos_cmd` que falla.
- `validate`: equipo completo válido; falta un papel obligatorio; agente no disponible;
  modelo fuera de la lista; QA con el mismo modelo que el desarrollador (error); misma
  familia con otra disponible (aviso) y sin otra disponible (sin aviso: una sola
  suscripción); validador con el mismo modelo que el QA (aviso).
- `card` exacta para un equipo de ejemplo.
- `render_instructions` para los cinco papeles sin marcadores pendientes; validador sin
  `qa_report` → error. `argv` con y sin esfuerzo.

## Criterios de aceptación
fmt, clippy y test de `ez-team` en verde; contratos sin cambios (`git diff` vacío en ellos).

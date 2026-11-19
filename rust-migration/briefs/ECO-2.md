# ECO-2 — `ez-arbiter`: el árbitro (tareas, rondas, reglas, verificación)

**Antes de nada lee `rust-migration/briefs/_COMMON-ECO.md` y `docs/designs/espacio-elefante.md`
completo (sobre todo §2, §4, §5, §8 y §9). Las reglas R1–R12 de §8 son la especificación.**

## Archivos que se tocan (solo estos)
- `rust/crates/ez-arbiter/**` (el `Cargo.toml` ya declara lo que puedes usar: `ez-core`,
  `ez-team`, `ez-herdr`, `serde`, `serde_json`, `toml`, `sha2`, `chrono`, `uuid`,
  `crossbeam-channel`, `parking_lot`, `tracing`).

Contrato de solo lectura: `rust/crates/ez-team/src/{roles.rs,contract.rs}` (`Role`,
`Seat`, `InstructionContext`). Usa `ez_herdr::permission::denied(&str)` para la lista
de denegación.

## API pública
```rust
pub trait Clock: Send + Sync { fn now(&self) -> f64; }              // segundos desde 1970
pub trait FishLauncher: Send + Sync {
    fn launch(&self, request: &LaunchRequest) -> Result<String, String>; // devuelve la dirección hc (`t7`)
    fn send(&self, terminal: &str, text: &str) -> Result<(), String>;
    fn alive(&self, terminal: &str) -> bool;
}
pub trait InstructionWriter: Send + Sync {
    fn render(&self, context: &InstructionContext) -> Result<String, String>;
    fn first_line(&self, role: Role, instructions: &Path) -> String;
}
pub struct LaunchRequest {
    pub fish: String, pub hc_name: String, pub role: Role, pub seat: Seat,
    pub cwd: PathBuf, pub env: BTreeMap<String, String>, pub labels: BTreeMap<String, String>,
    pub first_line: String,
}
pub struct Space { pub id: String, pub name: String, pub folder: PathBuf }
pub enum Caller { Elephant { space: String }, Fish { fish: String } }
pub enum Command {
    Status,
    TaskNew { title: String, brief: PathBuf, criteria: PathBuf },
    TeamPropose { seats: Vec<Seat>, card: String },   // ya validado por ez-team
    RoundStart,
    FishNew { role: Role, brief: PathBuf },
    FishSend { fish: String, file: PathBuf },
    Verify,
    RoundClose,
    TaskClose,
    Pause { reason: String },
    Report { verdict: Verdict, file: PathBuf, blocking: u32, major: u32, minor: u32,
             mutations_tried: Option<u32>, mutations_caught: Option<u32> },
}
pub enum Verdict { Pass, Fail }
pub struct Outcome { pub message: String, pub data: serde_json::Value }
pub struct Rejection { pub rule: &'static str, pub message: String }   // rule: "R1".."R11"

pub struct Arbiter { /* estado bajo un Mutex; nunca retenido durante E/S */ }
impl Arbiter {
    pub fn new(data_dir: PathBuf, launcher: Arc<dyn FishLauncher>,
               writer: Arc<dyn InstructionWriter>, clock: Arc<dyn Clock>) -> Result<Arbiter, String>;
    pub fn register_space(&self, space: Space);
    pub fn execute(&self, caller: &Caller, command: Command) -> Result<Outcome, Rejection>;
    // Acciones de la persona (desde la ventana):
    pub fn approve_task(&self, space: &str) -> Result<Outcome, Rejection>;
    pub fn reject_task(&self, space: &str, comment: &str) -> Result<Outcome, Rejection>;
    pub fn approve_team(&self, space: &str) -> Result<Outcome, Rejection>;
    pub fn reject_team(&self, space: &str, comment: &str) -> Result<Outcome, Rejection>;
    pub fn resume(&self, space: &str) -> Result<Outcome, Rejection>;
    pub fn stop(&self, space: &str) -> Result<Outcome, Rejection>;
    // Señales de la ventana (estado de hc):
    pub fn fish_blocked(&self, fish: &str, kind: Option<&str>);
    pub fn fish_exited(&self, fish: &str);
    // Para el elefante y la vista:
    pub fn wait_for_elephant(&self, space: &str, timeout: Duration) -> Option<ElephantEvent>;
    pub fn subscribe(&self) -> crossbeam_channel::Receiver<ArbiterEvent>;
    pub fn snapshot(&self, space: &str) -> Option<serde_json::Value>;   // estado para la vista
}
```
- `data_dir` es `%APPDATA%\EzAgent\hub` en producción; las pruebas usan una carpeta temporal.
- `ElephantEvent`: los de §8 «Eventos para el elefante», con `message` en español y datos
  (`fish`, `verdict`, `file`, `kind`, `reason`, `comment` según el caso).
- `ArbiterEvent`: todo lo de R12 (`task_proposed` con la tarjeta, `task_approved`,
  `team_proposed` con la tarjeta, `round_started`, `fish_launched` con `fish`, `role`,
  `terminal` y **`token`**, `fish_reported`, `verification`, `round_closed` con `clean` y
  motivos, `paused`, `resumed`, `task_closed`, `rejected` con regla y mensaje…), con
  `space`, `task` y `ts`. La ventana registra el token del pez en el servidor (ola 2).

## Comportamiento
- **Tarea, rondas, peces, informes y reglas: exactamente §8 (R1–R12).** Una tarea activa
  por espacio. `Status` funciona para elefante y pez y resume el estado en texto.
- **Tarjetas** (`task_proposed`): título, encargo (primeras 40 líneas) y una línea por
  criterio `- {id}: {descripcion} → {comando unido con espacios}`.
- **Lanzar un pez (R5):** id según §9 (`dev-r{n}-{k}`, `qa-r{n}`…), nombre hc
  `ez-{4 últimos de la tarea}-{pez}`, etiquetas `ez=1`, `ez_space`, `ez_task`, `ez_role`,
  `ez_fish`; entorno `EZ_ROLE=fish`, `EZ_FISH=<pez>`, `EZ_TOKEN=<token nuevo>` (uuid v4
  simple). `EZ_URL` y `PATH` los añade el lanzador de la ventana. Copia el encargo del
  pez a `ronda-<n>/<pez>.encargo.md`, escribe `<pez>.instrucciones.md` con
  `writer.render` y lanza con `writer.first_line`.
- **Corrección (R6):** copia el archivo a `ronda-<n>/<pez>.correccion-<k>.md` y envía
  por `launcher.send` una línea: `Corrección de la ronda {n}: lee {ruta} y vuelve a informar con ez report.`
- **Git (R4, R7, R9, R11):** `git -C <carpeta> rev-parse HEAD` y
  `git -C <carpeta> status --porcelain=v1 -z --untracked-files=all`. Cambios entre dos
  fotos = rutas cuya línea de estado cambió, más `git diff --name-only <head1> <head2>`
  si HEAD se movió. Se excluye todo lo que empieza por `.ez/`.
- **Verificación (R8):** hilo por criterio con nombre `ez-arbiter-check`, salida y error
  leídos en hilos, tope con `try_wait` cada 50 ms y `kill` al vencer (código `None`,
  falla). Últimos 4000 caracteres (por carácter, no por byte).
- **Persistencia:** `tareas/<tarea>/estado.json` (temporal + renombrar),
  `bitacora.jsonl` (una línea por `ArbiterEvent`) y `criterios.congelados.toml`.
  `Arbiter::new` recarga las tareas no finales (los espacios se vuelven a registrar).
- **Textos:** todos en `texts.rs`, generados por el código. Los de §8 son literales; los
  demás dicen qué falló y qué hacer, en una o dos frases.

## Pruebas (obligatorias)
Con `git init` real en carpetas temporales (configura `user.name` y `user.email` locales),
lanzador, escritor y reloj falsos:
- R1: un pez no puede usar comandos del elefante; un pez no informa por otro.
- R2: criterios inválidos (sin criterios, id repetido, comando vacío, carpeta fuera,
  comando denegado), carpeta sin git, segunda tarea activa.
- R3: cambiar `criterios.toml` tras aprobar → `paused` con `criterios_modificados`.
- R4/R5: orden (qa antes de que informe el desarrollador → rechazo; validador antes del
  QA → rechazo); máximo un QA por ronda; desarrolladores ≤ `cantidad`; papel fuera del equipo.
- R6: `send` a un QA → rechazo; a un desarrollador vivo → corrección enviada.
- R7: informe doble de un verificador; validador sin mutaciones.
- R8: criterio que pasa (`git --version`, contiene `git version`), que falla por código,
  por `contiene` y por tope (`ping -n 6 127.0.0.1` con `segundos = 1`).
- R9: ronda limpia; ronda sucia por cada causa; archivo protegido tocado por el desarrollador;
  pausas `tope_rondas`, `mismo_error` y `tope_tiempo` (reloj falso).
- R10: cerrar con 1 ronda limpia → rechazo; con 2 seguidas → `done`.
- R11: cambio fuera de `.ez/` entre rondas → `paused`.
- R12: la bitácora tiene un evento por acción; `wait_for_elephant` entrega en orden y
  respeta el tope; recarga tras `Arbiter::new`.

## Criterios de aceptación
fmt, clippy y test de `ez-arbiter` en verde; cada regla con al menos una prueba que la
rompe y otra que la cumple; ningún lock retenido durante git, verificación o lanzamiento.

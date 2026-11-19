# HD-7 — Registro del estado de cada subagente y textos de aviso

**Lee `rust-migration/briefs/_COMMON-HD.md` y `docs/designs/subagentes-herdr.md` (§3 y
§13 completo).** Es lógica pura con archivos: sin hc, sin Herdr, sin red. En paralelo,
HD-2 y HD-3 trabajan en `ez-herdr/src/herdr/*` y `ez-herdr/src/driver*`: no los toques.

## Archivos que se tocan (solo estos)
- Nuevo `rust/crates/ez-herdr/src/subagents.rs` (+ `subagents/…`) y sus pruebas.
- `rust/crates/ez-herdr/src/lib.rs`: **solo** la línea `pub mod subagents;`.

## Contrato (nombres exactos)
```rust
/// Estado de una tarea delegada (§13). Serializa como "en_proceso", "terminado",
/// "fallido", "validado".
pub enum TaskState { InProgress, Done, Failed, Validated }
impl TaskState { pub fn as_str(self) -> &'static str; pub fn parse(t: &str) -> Option<Self>;
                 pub fn is_final(self) -> bool; }
/// Un cambio de estado.
pub struct Change { pub state: TaskState, pub at_ms: u64, pub by: String, pub detail: String }
/// Resultado de una validación.
pub struct Validation { pub by: String, pub verdict: bool, pub summary: String,
                        pub file: Option<String>, pub at_ms: u64 }
/// Lo que se guarda de cada subagente (campos de §13, serde con esos nombres).
pub struct Record { pub name: String, pub terminal: String, pub parent: String,
    pub agent: String, pub label: String, pub model: Option<String>,
    pub effort: Option<String>, pub role: String, pub cwd: String, pub brief_head: String,
    pub state: TaskState, pub since_ms: u64, pub history: Vec<Change>,
    pub summary: Option<String>, pub report: Option<String>,
    pub validation: Option<Validation>, pub notified: Option<TaskState> }
pub struct Registry { /* raíz */ }
impl Registry {
    /// `EZ_SUBAGENTS_DIR` o `%LOCALAPPDATA%\EzAgent\subagentes`.
    pub fn from_env() -> Registry;
    pub fn at(root: impl Into<PathBuf>) -> Registry;
    pub fn root(&self) -> &Path;
    /// Crea el registro (error si ya existe ese nombre bajo ese padre).
    pub fn create(&self, record: &Record) -> Result<(), RegistryError>;
    pub fn get(&self, parent: &str, name: &str) -> Result<Option<Record>, RegistryError>;
    pub fn list(&self, parent: &str) -> Result<Vec<Record>, RegistryError>;
    /// Busca en todos los padres el registro de esa terminal.
    pub fn find_by_terminal(&self, terminal: &str) -> Result<Option<Record>, RegistryError>;
    /// Cambia el estado si la transición está permitida (§13), añade a `history`,
    /// aplica `edit` (resumen, informe, validación) y guarda; todo bajo el candado.
    pub fn transition(&self, parent: &str, name: &str, to: TaskState, by: &str,
        detail: &str, edit: impl FnOnce(&mut Record)) -> Result<Record, RegistryError>;
    pub fn mark_notified(&self, parent: &str, name: &str, state: TaskState)
        -> Result<(), RegistryError>;
}
pub enum RegistryError { Io(String), Invalid(String), NotFound(String), Exists(String),
                         Busy(String) }  // Display en español
/// Avisos pendientes: estado ≠ `notified` y último cambio no hecho por el padre.
pub fn pending_notices(records: &[Record]) -> Vec<&Record>;
/// Texto exacto del aviso de §13 para el estado actual del registro.
pub fn notice_text(record: &Record) -> String;
/// Una línea, sin saltos ni caracteres de control, máx. 300 letras (con «…» si se cortó).
pub fn one_line(text: &str, max: usize) -> String;
```

## Reglas
- Carpeta del padre: la dirección con `/` → `_` (y sin `..` ni separadores: si la
  dirección no es `[A-Za-z0-9_./-]+`, `Invalid`). Nombre: el de hc (§6); otro → `Invalid`.
- Escritura atómica: archivo temporal en la misma carpeta + renombrar. Candado:
  `<nombre>.lock` con creación exclusiva; reintento cada 50 ms hasta 3 s → `Busy`; se
  borra al terminar (también si falla), y un candado de más de 30 s se considera
  abandonado y se reemplaza.
- Transiciones permitidas: `en_proceso` → `terminado`|`fallido`; `terminado` →
  `validado`|`fallido`. Cualquier otra → `Invalid` con «<nombre> está «<estado>»; no
  puede pasar a «<nuevo>».». `create` admite `en_proceso` o `fallido` como estado inicial.
- `notice_text`: los tres formatos de §13, con `label` y `model` («OpenCode ·
  deepseek-v4.1-flash», o solo la etiqueta si no hay modelo), el resumen pasado por
  `one_line(…, 300)` y el detalle del último cambio como motivo de `fallido`.

## Pruebas
Transiciones (todas las permitidas y varias prohibidas), `create` duplicado, `list` y
`find_by_terminal` con dos padres (uno `maquina/t7`), dirección inválida, escritura
atómica (el archivo nunca queda a medias: lee mientras otro hilo escribe), dos hilos
que hacen `transition` a la vez sobre el mismo registro (uno gana, el otro ve el estado
nuevo o `Invalid`, nunca se pierde `history`), candado abandonado, `pending_notices`
(excluye cambios del padre), `notice_text` exacto de los tres casos, `one_line`.
Carpetas temporales únicas bajo `std::env::temp_dir()`.

## Criterios de aceptación
fmt, clippy y test de `ez-herdr` en verde. Informe en `rust-migration/reports/HD-7-r1.md`.

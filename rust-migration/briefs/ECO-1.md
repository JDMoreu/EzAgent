# ECO-1 — `ez-term`: cliente JSON-RPC de herdr-core

**Antes de nada lee `rust-migration/briefs/_COMMON-ECO.md`, `docs/designs/espacio-elefante.md`
(sobre todo §3 y §9), `docs/herdr/hc-protocolo.md` y, del esquema
`docs/herdr/hc-schema-0.1.21.json`, los métodos y tipos que se nombran aquí.**

## Archivos que se tocan (solo estos)
- `rust/crates/ez-term/**` (el `Cargo.toml` ya declara `ez-core`, `serde`, `serde_json`,
  `crossbeam-channel` y `tracing`; puedes añadir `uuid` y `parking_lot` del workspace).

## Qué construir
Un cliente síncrono (hilos, sin async) de `hc connect`: JSON-RPC 2.0, un mensaje por línea.

### Localizar hc
`pub fn locate_hc(configured: Option<&Path>) -> Option<PathBuf>`: en orden, la ruta
configurada si existe; `%LOCALAPPDATA%\Programs\herdr-core\hc.exe`; `hc.exe` en `PATH`.

### Cliente de peticiones
```rust
pub struct HcClient { /* Arc interno: Clone + Send + Sync */ }
impl HcClient {
    pub fn connect(hc: &Path) -> Result<HcClient, TermError>;   // lanza `hc connect` y hace hello
    pub fn from_io(reader: impl BufRead + Send + 'static,
                   writer: impl Write + Send + 'static) -> Result<HcClient, TermError>; // pruebas
    pub fn core_version(&self) -> String;                       // de `hello`
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, TermError>;
    pub fn run(&self, spec: &RunSpec) -> Result<Terminal, TermError>;
    pub fn list(&self, labels: &BTreeMap<String, String>) -> Result<Vec<Terminal>, TermError>;
    pub fn get(&self, terminal: &str) -> Result<Terminal, TermError>;
    pub fn read(&self, terminal: &str, lines: u32) -> Result<ReadText, TermError>;
    pub fn send(&self, terminal: &str, text: &str) -> Result<SendOutcome, TermError>;
    pub fn keys(&self, terminal: &str, keys: &[String]) -> Result<(), TermError>;
    pub fn interrupt(&self, terminal: &str) -> Result<(), TermError>;
    pub fn kill(&self, terminal: &str) -> Result<(), TermError>;
    pub fn rm(&self, terminal: &str) -> Result<(), TermError>;
    pub fn wait(&self, terminal: &str, timeout_s: f64) -> Result<Value, TermError>;
}
```
- `hello` con `{"generation": 1}`; guarda `version`.
- Un hilo lector con nombre `ez-term-reader` reparte respuestas por `id` (las respuestas
  pueden llegar en otro orden). Tope por llamada: 30 s; para `send` y `wait`, el tope de
  la operación + 10 s.
- Fin de flujo (EOF) o error de lectura: todas las llamadas pendientes y futuras devuelven
  `TermError::Disconnected`.
- Al soltar el último `HcClient` creado con `connect`, se cierra stdin y se termina el
  proceso hijo.
- Errores JSON-RPC → `TermError::Rpc { code, kind, message }` (`kind` = `error.data.kind`).

### Tipos
- `Terminal`: `address`, `name: Option<String>`, `argv`, `cwd`, `labels: BTreeMap`,
  `state` (`"live"`/`"finished"`), `status: Option<Status>`, `exit_code: Option<i32>`,
  `reason: Option<String>`, `parent: Option<String>`, `pid: Option<u32>`, `cols`, `rows`,
  `turn: u64`. Todos con `#[serde(default)]`; los campos desconocidos se ignoran.
- `Status` (objeto `Status` del esquema): `state`, `kind`, `msg`, `title`, `app`,
  `progress`, con `#[serde(default)]`.
- `RunSpec`: `argv`, `cwd: Option<PathBuf>`, `name: Option<String>`, `labels`, `env`
  (`BTreeMap<String, String>`), `size: Option<(u16, u16)>` (columnas, filas → objeto
  `InitialSize` del esquema), `no_parent` y `no_notify` (por omisión `true`).
- `ReadText { text, complete }`, `SendOutcome { turn, started: Option<bool>, status: Option<String> }`.
- `TermError` (Display en español): `NotFound`, `Spawn(String)`, `Disconnected`,
  `Timeout { method }`, `Rpc { code, kind: Option<String>, message }`, `Decode(String)`.

### Eventos (conexión aparte)
En una conexión con `events.subscribe`, hc deja de responder otras peticiones: los
eventos van siempre en una conexión propia.
```rust
pub struct EventStream { /* dueño del proceso hijo y del hilo `ez-term-events` */ }
impl EventStream {
    pub fn subscribe(hc: &Path, labels: &BTreeMap<String, String>) -> Result<EventStream, TermError>;
    pub fn from_io(reader, writer, labels) -> Result<EventStream, TermError>;  // pruebas
    pub fn snapshot(&self) -> &[Terminal];          // result.terminals de la suscripción
    pub fn events(&self) -> &crossbeam_channel::Receiver<TermEvent>;
}
pub enum TermEvent {
    Created(Terminal),
    Status { terminal: String, status: Status },
    Finished { terminal: String, exit_code: Option<i32>, reason: Option<String> },
    Removed { terminal: String },
    Lost { terminal: String },
    Restarted { terminal: String },
    Other { terminal: String, kind: String, data: Value },   // turn, title, foreground…
    Missed { snapshot: Vec<Terminal> },
    Heartbeat,
    Closed,                                                  // fin de la conexión
}
```
- `status`: si `data` trae `state`, es `Status`; si no, `Other`.
- `missed` llega seguido de una foto: entrega `Missed { snapshot }` con esa foto (lee
  el esquema y el protocolo para su forma; si no la puedes confirmar, entrega
  `Missed { snapshot: vec![] }` y márcalo `// DECISION`).
- Al soltar `EventStream` se termina el proceso hijo y el hilo acaba solo.

## Pruebas
Un hc falso en memoria (lectores y escritores sobre canales) que responde guiones:
hello; `run` + `get` + `rm`; respuestas fuera de orden; error JSON-RPC con `kind`;
tope de tiempo; EOF a mitad de una llamada (→ `Disconnected`); suscripción con foto
inicial, `created`, `status`, `finished`, `removed`, `heartbeat`, `missed` y EOF
(→ `Closed`); deserialización de los ejemplos reales de `docs/herdr/hc-protocolo.md`.

## Ejemplo contra el hc real
`examples/hc_smoke.rs` (`cargo run -p ez-term --example hc_smoke`): localiza hc;
suscribe con la etiqueta `ez_test=<pid>`; lanza `["hostname"]` con nombre
`ez-smoke-<pid>` y esa etiqueta; espera su evento `Finished` (tope 15 s); lee su texto;
la borra con `rm`; imprime un resumen con `tracing` y sale con código 0 solo si todo
funcionó. Ejecútalo una vez y pega su salida en el informe.

## Criterios de aceptación
- fmt, clippy y test de `ez-term` en verde; el ejemplo funciona contra el hc instalado.
- Ningún lock retenido mientras se espera una respuesta.

# ECO-3 — `ez-ctl`: protocolo, servidor local y comando `ez`

**Antes de nada lee `rust-migration/briefs/_COMMON-ECO.md` y `docs/designs/espacio-elefante.md`
(sobre todo §7; §8 para entender qué hay detrás del `Handler`).**

## Archivos que se tocan (solo estos)
- `rust/crates/ez-ctl/**`, salvo `src/skill.md`, que es **contrato** (el manual del
  elefante: inclúyelo con `include_str!` y no lo edites).
- El `Cargo.toml` ya declara `serde`, `serde_json`, `tiny_http`, `reqwest` (bloqueante),
  `uuid`, `crossbeam-channel`, `parking_lot` y `tracing`, y el binario `ez`
  (`src/bin/ez.rs`).

`ez-ctl` **no depende** de `ez-arbiter`: define el `Handler` y la integración (ola 2)
lo implementa con el árbitro.

## API pública
```rust
pub enum Caller { Elephant { space: String }, Fish { space: String, fish: String } }
pub struct Request { pub command: String, pub args: serde_json::Value }
pub struct Reply { pub ok: bool, pub message: String, pub data: serde_json::Value }
pub trait Handler: Send + Sync { fn handle(&self, caller: &Caller, request: &Request) -> Reply; }
pub struct Tokens { /* token → Caller, con lock */ }
impl Tokens { pub fn new() -> Tokens; pub fn register(&self, token: &str, caller: Caller);
              pub fn revoke(&self, token: &str); pub fn lookup(&self, token: &str) -> Option<Caller>; }
pub fn new_token() -> String;                              // uuid v4 en forma simple
pub struct CtlServer { /* … */ }
impl CtlServer {
    pub fn start(handler: Arc<dyn Handler>, tokens: Arc<Tokens>) -> std::io::Result<CtlServer>;
    pub fn url(&self) -> String;                           // http://127.0.0.1:<puerto>
    pub fn stop(self);
}
pub fn main_with(argv: &[String], env: &dyn Fn(&str) -> Option<String>,
                 cwd: &Path, out: &mut dyn Write, err: &mut dyn Write) -> i32;
```
`src/bin/ez.rs` solo llama a `main_with` con los argumentos, el entorno, la carpeta
actual, stdout y stderr del proceso, y sale con su código.

## Servidor
- `tiny_http` en `127.0.0.1:0` (puerto del sistema). Ocho hilos de atención
  (`ez-ctl-http-<n>`) para que un `wait` largo no bloquee al resto.
- Solo `POST /v1/command`. Cabecera `Authorization: Bearer <token>`; token ausente o
  desconocido → 401 con `{"ok":false,"message":"Token inválido o ausente.","data":null}`.
  JSON inválido o sin `command` → 400. Otra ruta o método → 404. Cuerpo > 1 MiB → 413.
- Con token válido llama a `handler.handle(caller, request)` y responde 200 con el `Reply`
  como JSON (`ok`, `message`, `data`).
- `stop` cierra el servidor y espera a los hilos.

## Comando `ez`
- `ez --skill` imprime `skill.md` (sin servidor). `ez --help` imprime el uso en español.
  `ez --version` imprime la versión del crate.
- Los demás comandos son los de la tabla de §7, con los nombres de `command` de esa
  tabla. Argumentos en `args` (JSON):
  - `task.new` → `{"title", "brief", "criteria"}`; `team.propose` → `{"file"}`;
    `fish.new` → `{"role", "brief"}`; `fish.send` → `{"fish", "file"}`;
    `pause` → `{"reason"}`; `wait` → `{"timeout"}` (segundos, 170 por omisión);
    `inventory` → `{"json": bool}`; `report` → `{"verdict", "file", "blocking", "major",
    "minor", "mutations_tried", "mutations_caught"}` (números ausentes = 0, salvo
    las mutaciones, que van `null` si no se dan); el resto → `{}`.
- Rutas: relativas a `cwd` → absolutas. Si el archivo no existe: salida 2 con
  `No existe el archivo: <ruta>`.
- Faltan `EZ_URL` o `EZ_TOKEN`: salida 3 con `Este comando solo funciona dentro de una
  terminal abierta por EzAgent (faltan EZ_URL y EZ_TOKEN).`
- Cliente: `reqwest::blocking`. Tope: `wait` → su tope + 15 s; `verify`, `round.close` y
  `task.close` → 3600 s; el resto → 120 s.
- Salida: imprime `message` en `out` (o el `Reply` completo con `--json`). Códigos: 0 si
  `ok`; 1 si `!ok`; 2 uso incorrecto (mensaje de uso en `err`); 3 sin conexión o 401
  (`No pude conectar con EzAgent en {url}. ¿Está abierta la ventana de EzAgent?` o el
  mensaje del 401).

## Pruebas
- Servidor + `main_with` de ida y vuelta con un `Handler` falso que registra lo recibido,
  para **cada** comando de la tabla (argumentos y rutas absolutas incluidos).
- 401 (sin token y token desconocido), 400, 404, 413.
- Concurrencia: un `wait` que el `Handler` retiene 2 s no bloquea un `status` simultáneo.
- Uso incorrecto (comando desconocido, falta un argumento, papel vacío) → 2.
- `--skill` contiene «# Manual del elefante»; falta de entorno → 3; servidor apagado → 3.

## Criterios de aceptación
fmt, clippy y test de `ez-ctl` en verde; `cargo build -p ez-ctl --bin ez` produce `ez.exe`.

# HD-4 — `ez delegate --agent`: delegar en un hijo de cualquier tipo desde Herdr

**Lee `rust-migration/briefs/_COMMON-HD.md` y `docs/designs/subagentes-herdr.md` completo
(sobre todo §3–§7).** Parte de `eco/main` con HD-2 (`RunAgent` con padre, `Placement`,
`hc_slug`) y HD-3 (`AgentDriver::validate_choice`, `Choice`) ya integrados: úsalos tal cual.
En paralelo, HD-5 toca `ez-swarm`: no lo toques.

## Archivos que se tocan (solo estos)
- Nuevo `rust/crates/ez-herdr/src/herdr/delegate.rs` (+ `delegate/…`) y sus pruebas;
  `rust/crates/ez-herdr/src/herdr.rs`: **solo** `mod delegate;` y sus `pub use`.
- `rust/crates/ez-ctl/src/cli.rs` (análisis de `delegate` y elección de modo), nuevo
  `rust/crates/ez-ctl/src/local.rs` (modo local), `rust/crates/ez-ctl/src/lib.rs` (línea
  `mod`/`pub use`), `rust/crates/ez-ctl/src/bin/ez.rs` (lo mínimo),
  `rust/crates/ez-ctl/Cargo.toml` (dependencias de ruta `ez-herdr` y `ez-core`, como las
  declara `ez-swarm`) y pruebas de `ez-ctl`.
- `rust/crates/ez-fakes/src/fake_ws_agent.rs` (+ nuevo `fake_ws_agent/echo.rs`): modo
  `--echo`. **No** toques `fake_agent.rs`.
- `rust/crates/ez-ctl/src/skill.md` es **contrato** del elefante: no lo edites.

## Contrato (ez-herdr, nombres exactos)
```rust
/// Pedido de delegación ya analizado (el contenido del encargo, no su ruta).
pub struct DelegateRequest {
    pub role: String, pub agent: String, pub model: Option<String>,
    pub effort: Option<String>, pub brief: String, pub name: Option<String>,
    pub show: Placement, pub cwd: Option<String>, pub parent: String,
    pub notify: bool, pub outlive_parent: bool,
}
pub enum DelegateError { Rejected(String), Unavailable(String) } // códigos 1 y 3
impl Herdr {
    pub fn delegate<L: LaunchSupport + Clone + Send + 'static>(
        &self, launcher: L, driver_check: &dyn Fn(&str, Option<&str>, Option<&str>)
            -> Result<Choice, String>, request: &DelegateRequest,
    ) -> Result<Map<String, Value>, DelegateError>;
}
pub fn role_short(role: &str) -> String;                 // §6
pub fn default_name(role: &str, n: usize, agent: &str) -> String; // §6, máx. 32
pub fn first_message(role: &str, parent: &str, brief: &str) -> String; // §7 exacto
```
(Si la firma de `delegate` con `driver_check` te resulta incómoda, puedes recibir el
`AgentDriver` directamente; dilo en el informe.)

## Flujo de `delegate` (§4–§7)
1. `validate_choice` (vía `driver_check`): error → `Rejected` sin crear nada.
2. Nombre: `--name` (validado §6) o `default_name(role, n, key)` con `n` = 1 + cuántas
   terminales devuelve `hc ls --all --json --label ez.parent=<p> --label ez.role=<papel>`.
3. Etiquetas §3: `ez.agent`, `ez.role`, `ez.parent` y, si hay, `ez.model`, `ez.effort`.
4. `run_agent` con `parent`, `hc_name`, `name` (el mismo), `labels`, `show`, `notify`
   (**por omisión `false`**: los avisos los da EzAgent, §13; `--hc-notes` lo pone en
   `true`; `--no-notify` se acepta y no cambia nada), `outlive_parent`, `model`, `effort`,
   `cwd`, `confirmed: true` y `task = first_message(...)` (texto de §7 **actualizado**:
   pide `ez report` al terminar; ese comando llega en HD-8, no lo implementes aquí). Si vuelve `name_taken` y el nombre era el automático → `n+1`
   (hasta 5). Otro error de hc → `Rejected` con su mensaje; hc no encontrado →
   `Unavailable`.
5. Devuelve el mapa de §4 (`ok, terminal, name, agent, label, model, effort, role,
   parent, show, placed, task_sent, warnings`), con los avisos de `Choice` sumados.

## `ez delegate` (ez-ctl)
- Sintaxis exacta de §4. Modo árbitro (`EZ_URL` y `EZ_TOKEN`): igual que hoy; con
  opciones del modo local → código 2 y el texto de §4.
- Modo local: `HC_TERMINAL_ID` obligatorio (si falta, código 3 con el texto de §4);
  `--brief` existente, UTF-8, ≤ 200 000 bytes (si no, código 2); `--show` con
  `Placement::parse` (si no, código 2); `VoiceConfig::load(None)`; `Herdr::new`;
  `AgentDriver::new(Arc<Herdr>, cfg)` como hace `HerdrHost::new` en `ez-swarm`.
- Salida: la línea legible de §4 o, con `--json`, el mapa. Códigos 0/1/2/3.
- `ez --help` muestra la sintaxis nueva.

## `fake-ws-agent --echo <carpeta> [argumentos libres…]`
Agente falso para la prueba de punta a punta (HD-6), sin modelos:
- Acepta y guarda **cualquier** argumento después de la carpeta (los flags de cada tipo).
- Al arrancar: OSC 7501 `ESC]7501;:state=idle:app=ez-fake ESC\`, escribe
  `<carpeta>/<HC_TERMINAL_ID o pid>.json` con `{argv, cwd, env: {OPENCODE_CONFIG_CONTENT,
  XDG_STATE_HOME, HC_TERMINAL_ID, MINIMAX_DATA_DIR, CODEX_HOME} (solo las presentes),
  lines: []}` e imprime `fake listo> `.
- Por cada línea de stdin: `state=working`, la agrega a `lines` (reescribe el archivo
  entero vía archivo temporal + renombrar), imprime `recibido: <línea>` y vuelve a
  `state=idle` con `fake listo> `.
Pruebas unitarias del análisis de argumentos y de la escritura del registro.

## Pruebas
- `ez-herdr`: `role_short`, `default_name` (recorte a 32, sin forma de dirección),
  `first_message` (texto exacto de §7), `delegate` con runtime falso: rechazo sin
  ningún `hc run`; nombre automático con `n` del `hc ls`; reintento por `name_taken`;
  etiquetas y `--parent` en el `hc run`; `Unavailable` si falta hc.
- `ez-ctl`: análisis de todas las opciones, errores de uso (código 2), modo árbitro con
  opciones locales (código 2), falta de `HC_TERMINAL_ID` (código 3), brief demasiado
  grande.

## Criterios de aceptación
fmt, clippy y test de `ez-herdr`, `ez-ctl` y `ez-fakes` en verde. Informe en
`rust-migration/reports/HD-4-r1.md`.

## Corrección heredada de la revisión de HD-2 (R2-1)
En `rust/crates/ez-herdr/src/herdr/child.rs` (`child_where_row`), `visible` debe ser
`true` cuando el subagente quedó en pestaña o panel (`placed != tree`) y `background`
su contrario. Autorizado tocar solo esas líneas, con su prueba.

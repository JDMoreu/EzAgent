# HD-4 — `ez delegate --agent`: delegar en un hijo de cualquier tipo desde Herdr

## Resultado
- Brief/tarea: HD-4
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/herdr/delegate.rs` (nuevo): `DelegateRequest` (el pedido ya analizado), `DelegateError` (`Rejected` = código 1, `Unavailable` = código 3) y `Herdr::delegate`, que valida tipo/modelo/esfuerzo con `driver_check` (si falla, no se crea nada), comprueba que hay `hc`, elige el nombre (§6), arma las etiquetas (§3), lanza el subagente con padre (`hc run --parent`) y le entrega el primer mensaje (§7). Reintenta con `n+1` cuando el nombre automático está ocupado y devuelve el mapa de §4.
- `rust/crates/ez-herdr/src/herdr/delegate/names.rs` (nuevo): `role_short` (papeles con abreviatura fija; para los demás, sus caracteres `[a-z0-9]` en minúsculas hasta 8), `default_name` (`<corto><n>-<tipo>`, recortado a 32 y sin forma de dirección) y `valid_name` (`^[a-z0-9][a-z0-9_.-]{0,31}$` y sin forma de dirección).
- `rust/crates/ez-herdr/src/herdr/delegate/message.rs` (nuevo): `first_message`, el texto de §7 (pide `ez report` al terminar; ese comando llega en HD-8).
- `rust/crates/ez-herdr/src/herdr/delegate/tests.rs` (nuevo): pruebas con un `HerdrRuntime` falso que registra cada argv: rechazo sin ningún `hc run`, nombre automático con el `n` del `hc ls`, reintento por `name_taken`, nombre pedido ocupado (rechazo), etiquetas y `--parent` en el `hc run`, primer mensaje entregado, `Unavailable` sin `hc` y el mapa de §4.
- `rust/crates/ez-herdr/src/herdr.rs`: `mod delegate;` y los `pub use` (`delegate`, `DelegateError`, `DelegateRequest`, `default_name`, `first_message`, `role_short`, `valid_name`).
- `rust/crates/ez-herdr/src/herdr/child.rs`: corrección heredada R2-1 en `child_where_row`: `visible` es `true` cuando el subagente quedó en pestaña o panel (`placed != tree`) y `background` lo contrario.
- `rust/crates/ez-herdr/src/herdr/child/tests/launch.rs`: dos pruebas de R2-1 (en el árbol y en pestaña).
- `rust/crates/ez-ctl/src/cli.rs`: análisis de `delegate` (papel, `--agent`, `--model`, `--effort`, `--brief`, `--name`, `--show`, `--cwd`, `--no-notify`, `--outlive-parent`, `--hc-notes`), elección de modo (árbitro o local), opciones sin valor en el análisis genérico, `ez --help` con la sintaxis nueva y `main_with` partido en funciones cortas.
- `rust/crates/ez-ctl/src/local.rs` (nuevo): el modo local: `VoiceConfig::load(None)`, `Herdr::new`, `AgentDriver::new(Arc<Herdr>, cfg)` (como `HerdrHost::new` en `ez-swarm`), la llamada a `Herdr::delegate` y el texto legible de §4 (`Subagente dev1-opencode (t12) lanzado: OpenCode · deepseek-v4.1-flash · high, hijo de t6h, en una pestaña.`).
- `rust/crates/ez-ctl/src/lib.rs`: `mod local;` y los `pub use` del modo local y de `parse_delegate_options`.
- `rust/crates/ez-ctl/Cargo.toml`: `ez-core` y `ez-herdr` por ruta (como declara `ez-swarm`).
- `rust/crates/ez-ctl/tests/delegate_local.rs` (nuevo): análisis de todas las opciones, nombre/ubicación/tipo inválidos, opciones locales dentro de una tarea del árbitro (código 2 con el texto de §4), `delegate` del árbitro igual que hoy, falta de `HC_TERMINAL_ID` (código 3), encargo demasiado grande y no UTF-8 (código 2) y la ayuda con la sintaxis nueva.
- `rust/crates/ez-fakes/src/fake_ws_agent.rs`: modo `--echo <carpeta> [argumentos libres…]` (acepta y guarda cualquier argumento posterior a la carpeta) y su línea en el uso.
- `rust/crates/ez-fakes/src/fake_ws_agent/echo.rs` (nuevo): el registro `<carpeta>/<HC_TERMINAL_ID o pid>.json` con `argv`, `cwd`, `env` (solo `OPENCODE_CONFIG_CONTENT`, `XDG_STATE_HOME`, `HC_TERMINAL_ID`, `MINIMAX_DATA_DIR` y `CODEX_HOME`, si están) y `lines`; se escribe entero por archivo temporal y renombrar, y cada línea de stdin se anota (`state=working` → `recibido: <línea>` → `state=idle`).

## Decisiones
- **`driver_check` como cierre en vez del `AgentDriver` completo**: `Herdr::delegate` recibe `&dyn Fn(&str, Option<&str>, Option<&str>) -> Result<Choice, String>`, la firma que da el brief (con el alias `DriverCheck` para que el linter no la marque como tipo complejo). Así `ez-herdr` no necesita el catálogo para delegar y las pruebas no tocan la configuración real; `ez-ctl` le pasa `|a, m, e| driver.validate_choice(a, m, e)`.
- **Texto de §7 en tres párrafos**: el diseño lo muestra con los cortes de línea del documento; aquí van como párrafos (el corte cae a mitad de frase). Las frases y los comandos (`ez report --state terminado --file <informe> --summary "<una frase>"`, `hc send {padre} "<pregunta>"`) son los del diseño.
- **`--hc-notes` como opción local**: el brief lo menciona (enciende los avisos de `hc` al padre) pero no está en la sintaxis de §4; se acepta como opción sin valor, solo en el modo local, y en una tarea del árbitro da el mismo código 2 con el texto de §4.
- **Avisos al padre apagados por omisión**: `notify` es `false` salvo `--hc-notes` (§13: los avisos los da EzAgent). `--no-notify` se acepta y no cambia nada.
- **`hc` ausente = `Unavailable`**: antes de crear nada se comprueba el binario con el `which` del runtime; si no está, `Unavailable` (código 3) con «No encuentro `hc`. Revisa voice.json.». Un fallo posterior de `hc` es `Rejected` (código 1) con su mensaje.
- **Reintento del nombre automático**: hasta 5 intentos en total (`n`, `n+1`… `n+4`), como dice §6 («se prueba `n+1` (hasta 5 veces)»). Con `--name` pedido, un choque es un rechazo y no se reintenta.
- **Contar los subagentes anteriores no es fatal**: si `hc ls --all --json --label ez.parent=… --label ez.role=…` falla, se usa `n = 1` y se añade un aviso; la delegación sigue.
- **El nombre que se informa es el real**: el resultado toma el nombre y el padre que devuelve `hc get` (§8) y solo cae al pedido si `hc` no respondió.
- **Entorno del árbitro a medias**: con solo `EZ_URL` o solo `EZ_TOKEN`, `ez delegate` no delega en Herdr: avisa con el texto de siempre y código 3, como antes.
- **Orden de las comprobaciones del modo local**: primero `HC_TERMINAL_ID` (código 3) y luego el encargo (UTF-8 y 200 000 bytes, código 2), como enumera el brief; la existencia del archivo se comprueba al analizar los argumentos (código 2, como el resto del CLI).
- **El análisis de `delegate` vive en `local.rs`**: `cli.rs` pasó de 600 líneas al añadir el modo local; el análisis y las opciones de `delegate` se movieron al módulo nuevo (que el brief sí permite crear) y `cli.rs` quedó en 495.
- **`valid_name` reimplementado en `delegate/names.rs`**: no se tocó `child.rs` (fuera del alcance salvo R2-1) y su `hc_valid_name` es privado; se repuso la regla de §6 en el módulo nuevo.
- **El registro del falso se escribe con `serde_json`**: `ez-fakes` ya depende de `serde_json`; el archivo se escribe entero (temporal + renombrar) en cada línea, como pide el brief.

## Verificación
- `cargo fmt -p ez-herdr -p ez-ctl -p ez-fakes -- --check`: sin diferencias (sin salida).
- `cargo clippy -p ez-herdr -p ez-ctl -p ez-fakes --all-targets -- -D warnings`: sin avisos (`Finished dev profile in 3.37s`).
- `cargo test -p ez-herdr -p ez-ctl -p ez-fakes`: todo en verde (`test result: ok.` en todos los binarios).
  - `ez-herdr` (lib): `test result: ok. 230 passed; 0 failed`; `tests/permission.rs`: `27 passed`; `tests/metrics_parity.rs`: `2 passed`; `tests/agent_driver_parity.rs`, `tests/herdr_parity.rs` y `tests/tasks_activity_parity.rs`: `1 passed` cada uno.
  - `ez-ctl` (lib): `4 passed`; `tests/cli_errors.rs`: `7 passed`; `tests/delegate_local.rs`: `9 passed`; `tests/roundtrip.rs`: `6 passed`; `tests/server_errors.rs`: `7 passed`; `tests/waits.rs`: `2 passed`.
  - `ez-fakes` (lib): `test result: ok. 62 passed; 0 failed`.
- Métricas: `ez-herdr` 209 → 230 pruebas de unidad (+21) y 241 → 262 en total; `ez-ctl` 26 → 35 (+9); `ez-fakes` 57 → 62 (+5).
- No se tocó Python: `pytest` no aplica en este brief.

## Pendiente o riesgos
- **Prueba de humo con un agente real (incidencia propia)**: al comprobar el binario ya compilado (`target\debug\ez.exe delegate developer --agent opencode --brief …`) se lanzó una delegación de verdad, porque la terminal de este subagente tiene `HC_TERMINAL_ID` y el `hc` de la máquina está instalado. El programa `opencode` arrancó y terminó de inmediato (código 1) **sin recibir el encargo** (aviso: «No pude enviar la tarea: t12j has finished»), así que no hubo llamada a ningún modelo ni gasto de cuota; tampoco abrió pestaña ni panel (`keep: false`, ubicación `tree`). El registro se borró con `hc rm t12j` y no queda terminal ni pestaña de la prueba. La lección: para probar el binario fuera de HD-6 hay que quitar `HC_TERMINAL_ID` del entorno.
- **`ez report` todavía no existe**: el primer mensaje de §7 lo pide, pero ese comando llega en HD-8; un subagente que lo intente ahora recibe «comando desconocido» y su estado no cambia (§13 lo implementa más adelante).
- **`--hc-notes` no está en la sintaxis de §4**: queda como opción local documentada aquí; si el elefante prefiere otra forma, se cambia en una línea (`local.rs`).
- **Prueba de punta a punta pendiente (HD-6)**: aquí solo se preparó el agente falso (`--echo`) y la capa de delegación; no se volvieron a lanzar agentes reales ni se tocó la ventana de Herdr de la persona.
- **Flakiness previa en `ez-ctl`**: `tests/server_errors.rs::stop_closes_the_port_and_url_is_loopback` falla a veces cuando otro binario de prueba toma el mismo puerto efímero en paralelo; no tiene que ver con este cambio (pasa al correrlo solo).

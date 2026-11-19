# HD-8 — Estado y avisos: `ez report`, `ez validate`, `ez tasks` y el vigilante

## Resultado
- Brief/tarea: HD-8
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/subagents/watch.rs` (nuevo): el vigilante de §13. `WatchRuntime` (trait: `hc`, reloj y espera, inyectables), `HcRuntime` (el `hc` real a través del runtime de Herdr), `WatchConfig` (padre, intervalo, inactividad y margen), `Watcher` (el bucle: `tick` + `run`), `Tick` (qué se falló, qué se avisó y si el padre sigue vivo), `WatchStop` (`ParentGone`/`Idle`), `watcher_name`/`watcher_args` (el `hc run --bg --json --no-parent --no-notify --name ez-watch-<slug> --label ez.watch=<padre> -- <ez> watch --parent … --dir … --hc …`) y `ensure_watcher` (`name_taken` = ya corre, no es error).
- `rust/crates/ez-herdr/src/subagents/watch/tests.rs` (nuevo): pruebas del vigilante con `hc` y reloj falsos: terminal terminada sin informe → `fallido` + aviso; `terminado` → un aviso y no se repite; cambio hecho por el padre → sin aviso; fallo de `hc send` → reintento en la vuelta siguiente; salida por padre muerto y salida por 10 min sin nada pendiente; y la etiqueta `ez.parent=<p>` que se le pregunta a `hc`.
- `rust/crates/ez-herdr/src/subagents.rs`: `mod watch;` y los `pub use` del vigilante (incluidos `DelegateState` y los tres tiempos por omisión).
- `rust/crates/ez-herdr/src/herdr/delegate.rs`: `Herdr::delegate` recibe `Option<&DelegateState>`; tras `run_agent` guarda el registro del subagente (`en_proceso` si el encargo se entregó o quedó sin confirmar; `fallido` con el motivo si no) y arranca el vigilante. Ninguno de los dos falla la delegación: si algo sale mal, va en `warnings`. El resultado lleva ahora `state`.
- `rust/crates/ez-herdr/src/herdr/delegate/record.rs` (nuevo): `build_record` (el registro de §13 con sus campos) y `register` (`Registry::create`).
- `rust/crates/ez-herdr/src/herdr/delegate/tests/state.rs` (nuevo): el registro se guarda `en_proceso`/`fallido`, el vigilante se arranca con su nombre y sus etiquetas, y un vigilante que ya corre no es un error.
- `rust/crates/ez-herdr/src/herdr/child.rs`: R4-1, `hc_valid_name` y `looks_like_address` pasan a `pub(crate)`.
- `rust/crates/ez-herdr/src/herdr/delegate/names.rs`: R4-1, `valid_name` ya no reimplementa la regla: usa `child::hc_valid_name`.
- `rust/crates/ez-ctl/src/cli.rs`: `ez report --state …` (modo local), `ez validate <nombre> --pass|--fail`, `ez tasks` y `ez watch`; `--pass`/`--fail` como opciones sin valor; `Parsed::has`; el reparto arbitro/local de `report`; y la ayuda con la sintaxis nueva.
- `rust/crates/ez-ctl/src/local/state.rs` (nuevo): el análisis y la ejecución de `report`, `validate` y `tasks`: el registro por terminal (`find_by_terminal`), la transición con `by` = la terminal, la validación (solo el padre o un hermano; `terminado` → `validado`/`fallido` con `validation` lleno), la tabla legible y el JSON.
- `rust/crates/ez-ctl/src/local/watch.rs` (nuevo): el comando `ez watch` (el bucle del vigilante), `start_watcher` (arranca el vigilante del padre) e `is_sibling` (el padre en `hc` de una terminal).
- `rust/crates/ez-ctl/src/local.rs`: `mod state;`/`mod watch;`, el estado de la delegación (registro + vigilante) y el texto legible de `ez delegate` con el estado del subagente.
- `rust/crates/ez-ctl/tests/local_state.rs` (nuevo): análisis y códigos de `report`/`validate`/`tasks`/`watch` (16 pruebas).
- `rust/crates/ez-fakes/src/fake_ws_agent.rs` y `fake_ws_agent/echo.rs`: `--echo CARPETA [--report terminado|fallido|none] [--ez RUTA] [libres…]`; con `--report` distinto de `none`, al recibir la primera línea escribe `CARPETA/<id>-informe.md` y ejecuta `<ez> report --state <estado> --file <informe> --summary "eco <marca E2E-…>"` (sin shell, con `CREATE_NO_WINDOW` en Windows). Por omisión `none`.

## Decisiones
- **`task_sent` en falso = encargo no entregado**: si la clave no viene, no se puede confirmar y el registro queda `en_proceso` («se entregó o quedó sin confirmar»). Si viene en falso, el registro nace `fallido` con el motivo.
- **`by` del alta = `ez delegate`**: el cambio que hace el padre al delegar se anota así (no con la dirección del padre), para que el aviso «está en proceso» llegue al padre (§13: el padre recibe el aviso sin pedirlo). El cambio que hace el padre al validar sí lleva su dirección, y por eso no se avisa (es la regla de «no avisa los cambios que hizo el propio padre»).
- **`hc` no responde → el vigilante no cambia nada**: una vuelta fallida no marca `fallido` ni avisa; se reintenta en la siguiente. Marcar por un fallo de `hc` sería tirar el trabajo del subagente por un problema ajeno.
- **Margen de 6 s antes de marcar `fallido`**: una terminal recién creada puede tardar un instante en aparecer en `hc ls`; con el margen, el vigilante no la declara muerta antes de tiempo.
- **El aviso sale del estado nuevo**: tras marcar `fallido`, el vigilante relee los registros y avisa del estado que quedó (no del anterior).
- **`hc send` que falla → reintento**: el registro sigue sin `notified` y el aviso se manda en la vuelta siguiente.
- **El padre se comprueba con `hc get`**: `hc ls --label ez.parent=<p>` solo trae los hijos; para saber si el padre sigue vivo hace falta `hc get <p>` (`state: live`/`finished`).
- **Filtrado por etiqueta de `hc`**: el vigilante usa `hc ls --all --json --label ez.parent=<p>` como dice el diseño (comprobado en `hc` 0.1.21: el filtro por etiqueta funciona) y, además, el campo `parent` de cada terminal.
- **El estado sale al final de la línea de `ez delegate`**: el texto de §4 no lo incluye, pero el brief pide que la salida añada `state`; se añade como «Estado: en_proceso.» al final (y en el mapa JSON).
- **`ez report` con `--state` es local y sin él es del árbitro**: así `report` sigue siendo exactamente la orden de siempre para el árbitro (y el análisis de `--verdict` no cambia). En una tarea del árbitro, `report --state`, `validate`, `tasks` y `watch` son un uso incorrecto (código 2).
- **`EZ_HC_PATH` y el entorno inyectado**: los comandos locales leen la ruta de `hc` y la raíz del registro del entorno que recibe el comando (no del proceso), para que las pruebas apunten a una carpeta temporal y a un `hc` que no existe: no se toca Herdr ni la configuración real.
- **Tiempos del vigilante desde el entorno** (`EZ_WATCH_INTERVAL_MS`, `EZ_WATCH_IDLE_MS`, `EZ_WATCH_GRACE_MS`): por omisión 3 s, 10 min y 6 s; una prueba puede esperarlo sin tardar diez minutos.
- **`DelegateState` vive en `subagents/watch.rs`**: `herdr.rs` (fuera del alcance) no se puede tocar para exportar un tipo nuevo de `herdr::delegate`, así que el tipo se exporta desde `subagents` (la línea `pub use` que el brief autoriza).
- **La raíz del registro de `ez delegate` sale del entorno del proceso** (`Registry::from_env`) y la de los comandos nuevos del entorno inyectado: en producción son el mismo entorno; `ez_ctl::delegate` no puede cambiar de firma porque su `pub use` está en `lib.rs` (fuera del alcance).
- **El agente falso declara al recibir la primera línea**: es el momento en que el subagente ya tiene el encargo; el resumen es «eco <primera marca E2E-…>» o «eco sin marca».

## Verificación
- `cargo fmt -p ez-herdr -p ez-ctl -p ez-fakes -- --check`: sin diferencias (sin salida), código 0.
- `cargo clippy -p ez-herdr -p ez-ctl -p ez-fakes --all-targets -- -D warnings`: sin avisos (`Finished dev profile [unoptimized + debuginfo] target(s) in 1.72s`), código 0.
- `cargo test -p ez-herdr -p ez-ctl -p ez-fakes`: todo en verde.
  - `ez-herdr` (lib): `test result: ok. 262 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`; `tests/permission.rs`: `27 passed`; `tests/metrics_parity.rs`: `2 passed`; `tests/agent_driver_parity.rs`, `tests/herdr_parity.rs` y `tests/tasks_activity_parity.rs`: `1 passed` cada uno.
  - `ez-ctl` (lib): `4 passed`; `tests/cli_errors.rs`: `7 passed`; `tests/delegate_local.rs`: `9 passed`; `tests/local_state.rs`: `16 passed`; `tests/roundtrip.rs`: `6 passed`; `tests/server_errors.rs`: `7 passed`; `tests/waits.rs`: `2 passed`.
  - `ez-fakes` (lib): `test result: ok. 69 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.
- Métricas: `ez-herdr` 262 → 294 pruebas (+32); `ez-ctl` 35 → 51 (+16); `ez-fakes` 62 → 69 (+7).
- Prueba de humo del binario (`target\debug\ez.exe`) con un registro de mentira y `EZ_HC_PATH` a un `hc` inexistente: `ez tasks` muestra la tabla, `ez validate` de una tarea en proceso da código 1 con «solo se validan las terminadas», `ez report` desde una terminal sin registro da código 1 con «Esta terminal no tiene una tarea de EzAgent.» y `ez tasks --json` devuelve los registros.
- No se tocó Python: `pytest` no aplica en este brief.

## Pendiente o riesgos
- **Archivos nuevos fuera de la lista literal del brief** (todos dentro de los módulos que el brief sí permite, y por la regla de 600 líneas): `ez-herdr/src/subagents/watch/tests.rs` (las pruebas del vigilante; `watch.rs` quedó en 488 líneas), `ez-herdr/src/herdr/delegate/record.rs` y `.../delegate/tests/state.rs` (`delegate/tests.rs` pasó de 640 líneas), `ez-ctl/src/local/state.rs`, `ez-ctl/src/local/watch.rs` y `ez-ctl/tests/local_state.rs`. No se tocó ningún archivo de otro crate ni `herdr.rs`, `lib.rs`, `registry.rs` ni `Cargo.toml`.
- **No se probó contra el `hc` real**: ni `ez delegate`, ni `ez report`, ni `ez validate`, ni `ez watch` se ejecutaron contra Herdr (hubieran lanzado agentes de verdad). Solo se usaron `hc ls --all --json`, `hc ls --label …` y `hc get` (solo lectura) para confirmar la forma del JSON (`state: live|finished`, `exit_code`, `parent`) y que el filtro por etiqueta funciona en `hc` 0.1.21. No se creó ni se cerró ninguna terminal ni pestaña de la persona.
- **La prueba de punta a punta con agentes reales sigue pendiente (HD-6)**: aquí solo se preparó el agente falso con su modo de informe; el ciclo completo (hijo → `ez report` → aviso al padre → `ez validate`) no se probó con Herdr de verdad.
- **El subagente tiene que llamar a `ez report`**: el primer mensaje de §7 ya lo pide (`ez report --state terminado --file <informe> --summary "<una frase>"`), pero solo funciona si el subagente lo ejecuta: si no, el vigilante lo marca `fallido` («la terminal terminó sin informe») cuando su terminal muere.
- **Un registro escrito con BOM no se lee**: el registro lo escribe el propio EzAgent (sin BOM), pero un archivo hecho a mano con PowerShell (`Set-Content -Encoding UTF8` en Windows) lleva BOM y el lector da «registro inválido». El archivo `registry.rs` (fuera del alcance) es el que lee.
- **El vigilante depende de `hc`**: si `hc` no responde, el vigilante no hace nada y se queda esperando hasta que vuelve (o hasta que pasan los 10 min de inactividad y termina). `ez delegate` y `ez report` lo vuelven a arrancar cuando pueden.
- **Dos carpetas de prueba quedaron en `%TEMP%`** (`ez-smoke-2` y `ez-smoke-<uuid>`): la papelera del entorno no acepta rutas fuera del espacio de trabajo y no se pudo borrarlas con un comando permanente.

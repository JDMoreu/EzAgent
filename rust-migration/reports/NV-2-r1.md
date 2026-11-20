# NV-2 — `ez retry`: retomar un subagente bloqueado o fallido

## Resultado
- Brief/tarea: NV-2 (con su adenda del 19-nov)
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/subagents.rs`: campos `brief_path` y `retry_of` en `Record` con `#[serde(default)]`, módulo `retry` y sus exportaciones.
- `rust/crates/ez-herdr/src/subagents/retry.rs`: núcleo de `ez retry`: nombre `<nombre>-r<n>` (continúa la serie y recorta a 32), nota de §4 en el primer mensaje, registro nuevo `en_proceso` con `retry_of`, registro viejo a `fallido` («reemplazado por …»), cierre de la terminal vieja tras el arranque y rasgo `AutoRetry` para el vigilante.
- `rust/crates/ez-herdr/src/subagents/retry/tests.rs`: pruebas con `HerdrRuntime` y lanzador falsos: manual, fallido, `-r2`, terminal vieja cerrada/conservada, fallo de arranque y carrera de nombre.
- `rust/crates/ez-herdr/src/subagents/watch.rs`: `Tick.retried`, campo `auto_retry`, `with_auto_retry` y `trigger_auto_retry` (sin locks durante el lanzamiento).
- `rust/crates/ez-herdr/src/subagents/watch/novelties.rs`: tras un bloqueo por límite (y después de anotar `uso.json`) se pregunta al gancho de reintento automático.
- `rust/crates/ez-herdr/src/subagents/watch/tests.rs`, `.../watch/tests/auto.rs`: disparo en límite, política apagada y diálogo sin disparo.
- `rust/crates/ez-herdr/src/herdr/delegate.rs`, `.../delegate/record.rs`: `DelegateRequest.brief_path` y su guardado en el registro (los registros viejos siguen leyéndose por `#[serde(default)]`).
- `rust/crates/ez-herdr/src/herdr/delegate/tests/state.rs`: comprueba que `ez delegate` guarda `brief_path`.
- `rust/crates/ez-herdr/src/subagents/tests.rs`: el registro de prueba incluye los campos nuevos.
- `rust/crates/ez-ctl/src/cli.rs`, `rust/crates/ez-ctl/src/cli/parsed.rs`: ruta y ayuda de `ez retry`, banderas `--auto/--same/--keep`; el análisis de argumentos se movió a `cli/parsed.rs` para respetar el tope de 600 líneas.
- `rust/crates/ez-ctl/src/local.rs`: módulo `retry` y `brief_path` al construir la delegación.
- `rust/crates/ez-ctl/src/local/retry.rs`: `ez retry` del modo local: `--auto` con `ez_scan::recommend` (agotado excluido), `--same`, opciones manuales y `--keep`; gancho `WatchAutoRetry` que lee `planes.toml`.
- `rust/crates/ez-ctl/src/local/retry/tests.rs`: análisis, `--same`, `--auto` evitando el modelo agotado, encargo guardado/respaldo y política encendida/apagada.
- `rust/crates/ez-ctl/src/local/watch.rs`: instala el gancho de reintento automático en `ez watch`; `planes.toml` se lee de `%APPDATA%\EzAgent`.
- `rust/crates/ez-ctl/tests/local_state.rs`: el registro de prueba incluye los campos nuevos.
- `rust/crates/ez-ctl/Cargo.toml`, `rust/crates/ez-ctl/examples/e2e_retry.rs`: ejemplo de punta a punta con `hc` real y `fake-ws-agent` (limpia todo lo que abre).
- `rust-migration/reports/NV-2-r1.md`: este informe.

## Decisiones
- `// DECISION` en `retry.rs`: una terminal nueva es una sesión nueva; no hay `--session` que retomar en este camino (la lección de OpenCode del 19-nov queda cubierta).
- `// DECISION` en `retry.rs`: la nota de §4 va delante del encargo; `brief_head` guarda solo el encargo original.
- `// DECISION` en `retry.rs`: el hijo nuevo se coloca en el árbol para no mover el foco de la persona.
- `// DECISION` en `retry.rs`: los avisos los da EzAgent; el reintento no activa `hc` notify.
- `// DECISION` en `retry.rs`: si el registro viejo ya estaba `fallido` no se reescribe (estado final); el enlace queda en el `retry_of` del nuevo.
- `// DECISION` en `local/retry.rs`: sin `--auto` ni opciones manuales se conservan tipo, modelo y esfuerzo del registro; `--same` hace lo mismo de forma explícita y `--auto` es el que cambia de modelo.
- `// DECISION` en `local/retry.rs`: la causa del aviso sale del último cambio del registro; si no hay, «una novedad».
- `// DECISION` en `local/watch.rs`: la política se consulta en cada novedad, no al arrancar el vigilante.
- Fuera de esa marca: `brief_path` es la ruta guardada y, si el archivo ya no se puede leer (o el registro es anterior a NV-2), se usan las primeras 200 letras de `brief_head` con aviso; la serie de nombres quita el `-rN` previo (`dev1-r1` → `dev1-r2`); la terminal vieja no se cierra si el encargo no llegó al hijo nuevo; `--auto` se resuelve en `ez-ctl` y a `ez-herdr` solo llega la elección hecha.
- Archivos fuera de la lista literal de la adenda, todos dentro de los crates del brief y necesarios: `herdr/delegate.rs` y `herdr/delegate/record.rs` (para que `ez delegate` guarde `brief_path`), `ez-ctl/src/local.rs` (módulo y campo), `ez-ctl/src/local/watch.rs` (implementa el rasgo inyectado, como pide la adenda), `cli/parsed.rs` (el tope de 600 líneas de `cli.rs`), `Cargo.toml` (ejemplo) y las pruebas de fixtures tocadas por los campos nuevos.

## Verificación
- `cargo fmt -p ez-herdr -p ez-ctl -- --check`: sin diferencias.
- `cargo clippy -p ez-herdr -p ez-ctl --all-targets -- -D warnings`:
  `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.55s` (código 0).
- `cargo test -p ez-herdr -p ez-ctl`: código 0; 428 pruebas en verde (356 de `ez-herdr` y 72 de `ez-ctl`, incluidas las de paridad y las nuevas). Líneas finales:
  `test result: ok. 324 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` (`ez-herdr` lib)
  `test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` (`ez-ctl` lib)
  `test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` (`ez-herdr` permission)
- Punta a punta (`cargo run -p ez-ctl --example e2e_retry`, `hc` y `herdr-app` reales, `fake-ws-agent`): 17/17 en verde. Evidencia clave: el vigilante dejó `retry1-opencode` en `bloqueado`; `ez retry retry1-opencode --auto` lanzó `retry1-opencode-r1` con `opencode-go/deepseek-v4.1-flash` (el `glm-5.3` agotado quedó fuera), padre `t1zn`, el viejo quedó `fallido` con «reemplazado por retry1-opencode-r1» y `closed_old=true`; al final no quedaron terminales de la prueba.
- No se ejecutaron agentes reales ni se leyó configuración del usuario; la prueba usó carpetas temporales y agentes falsos, y cerró su espacio de Herdr.

## Pendiente o riesgos
- El reintento automático con `planes.toml` encendido está cubierto con pruebas de unidad (política y gancho falso del vigilante), no con una corrida real de `hc`; el camino manual `--auto` sí está probado de punta a punta.
- El gancho automático vive en el proceso de `ez watch` y usa el entorno que hereda; si `hc` no le pasara `APPDATA`, leería la configuración por omisión. En el uso normal el vigilante lo arranca `ez delegate`/`ez report` desde la sesión de la persona.
- La prueba de punta a punta tarda unos segundos y necesita `hc`, `herdr-app` y la ventana de Herdr abiertos; no está en `cargo test` para no depender de ellos.

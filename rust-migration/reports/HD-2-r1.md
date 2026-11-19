# HD-2 — ez-herdr: lanzar un agente como hijo de otro y ubicarlo en Herdr

## Resultado
- Brief/tarea: HD-2
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/herdr/child.rs` (nuevo): `Placement` (tree/tab/right/down) y
  `hc_slug` (nombre que `hc` acepta), la ruta de lanzamiento con padre
  (`hc run --json --bg --parent … --name … --cwd … --label … -- <argv>`), la ubicación
  (`terminal.keep` con `--place`, `pane.focus` del padre, reintentos de 0,5 s hasta 15 s
  mientras Herdr responda «no running terminal») y la relectura con `hc get`.
- `rust/crates/ez-herdr/src/herdr/child/tests.rs` y `rust/crates/ez-herdr/src/herdr/child/tests/launch.rs`
  (nuevos): doble de `HerdrRuntime` que registra cada argv y reparte respuestas preparadas
  (`hc:run`, `hc:get`, `app:terminal.keep`, `app:pane.focus`); pruebas de `hc_slug`,
  `Placement`, del argv con padre, de cada ubicación y del resultado.
- `rust/crates/ez-herdr/src/herdr/actions.rs`: `RunAgent` gana `parent`, `hc_name`, `labels`,
  `show`, `notify` y `outlive_parent` (por omisión `None`/`false`/`Tree`); `open_launch`
  desvía a la ruta nueva cuando hay padre (ignora `background`); `launch_background` pasa a
  `hc` el nombre válido (`hc_slug(label)`) en vez de la etiqueta legible; `wrapped_argv`
  compartido entre el lanzamiento en segundo plano y el de los subagentes; `launch_texts`
  extraído para que `run_agent` siga por debajo de 60 líneas.
- `rust/crates/ez-herdr/src/herdr.rs`: `mod child;` y `pub use child::{hc_slug, Placement};`.
- `rust/crates/ez-herdr/tests/herdr_parity/launch.rs`: el literal `RunAgent` usa
  `..RunAgent::default()` (dejaba de compilar con los campos nuevos).
- `rust/crates/ez-app/src/app/activity/tasks.rs` y
  `rust/crates/ez-app/src/app/activity/tasks/prelaunch.rs`: literales `RunAgent` con
  `..RunAgent::default()` (dejaban de compilar con los campos nuevos).
- `parity/fixtures/herdr.jsonl`: los 7 casos de `run_agent` en segundo plano esperan ahora el
  nombre que ve `hc` (`claude-voz`, `mi-pestana`, `opencode-voz`) en vez de la etiqueta
  legible, que `hc` rechaza.

## Comportamiento implementado (§8 del diseño)
1. Con `parent` (no vacío) el lanzamiento va siempre por `hc run --json --bg --parent <p>
   --name <hc_name o hc_slug(name)> --cwd <carpeta> [--no-notify] [--outlive-parent]
   --label agent=<tipo> [--label k=v …] -- <argv>`; nunca `--no-parent` ni `tab.new`. Con
   entorno o `path_prepend`, `<argv>` es la misma envoltura de PowerShell que usa
   `launch_background`. Sin `parent`, todo sigue igual que hoy.
2. Ubicación: `Tree` no llama a `herdr-app`; `Tab` → `terminal.keep --place tab`;
   `Right`/`Down` → `pane.focus --terminal <padre>` y luego `terminal.keep --place
   right|down`. Si `pane.focus` falla → pestaña con el aviso «El agente padre no tiene
   panel; abrí el subagente en una pestaña.». Si `terminal.keep` responde «no running
   terminal» se reintenta cada 0,5 s hasta 15 s; cualquier otro error o el fin de los
   reintentos deja al subagente en el árbol con el aviso «Herdr no pudo ubicar el subagente
   (<motivo>); quedó en el árbol bajo su agente.». La ubicación nunca hace fallar el
   lanzamiento.
3. Tras ubicar, `hc get <t>`: el resultado añade `parent` (el real), `name` (el de `hc`),
   `placed`, `labels` y, si `terminal.keep` respondió, `tab`/`pane`. Si el padre real no es el
   pedido, aviso «El padre de <t> es <real>, no <pedido>.».
4. Después, `finish_run` como siempre. Los avisos de la ubicación se suman a `warnings`.
5. Nombre ocupado: `hc` responde `{"error":{"kind":"name_taken",…}}`; la ruta con padre
   devuelve ese caso como error con `"name_taken": true` en el mapa (HD-4 reintenta).

## Decisiones
- `// DECISION` en `actions.rs`: el lanzamiento en segundo plano pasa a `hc` el nombre
  válido (`hc_slug(label)`) porque `hc` rechaza «claude · voz»
  (`invalid_params: bad name`); la etiqueta legible de la pestaña (`tab.new --name`) no
  cambia. Es la «corrección incluida» del brief.
- `// DECISION` en `child.rs`: si `hc get` respondió, `parent` es el real (vacío si la
  terminal no tiene padre); si no respondió, se conserva el pedido como mejor dato.
- `// DECISION` en `child.rs`: las etiquetas del resultado son las que devuelve `hc get` y,
  si no las hay, las que se pidieron a `hc` (`agent=<tipo>` + las extra).
- `// DECISION` en `child.rs`: `hc_name` se usa tal cual cuando `hc` lo acepta
  (`^[a-z0-9][a-z0-9_.-]{0,31}$` y sin forma de dirección) y, si no, se pasa por `hc_slug`.
- `// DECISION` (implícito en el código): la cadencia de reintentos la marca el reloj del
  runtime (`HerdrRuntime::monotonic`), no una espera fija; con el reloj real son 0,5 s y
  el tope es 15 s, y con un doble de pruebas no se espera de verdad.
- `// DECISION` en `child/tests.rs`: el doble de `HerdrRuntime` devuelve en `which` el
  mismo nombre que se le pide, para que el argv se lea tal cual y no se busque el Herdr
  instalado en la máquina.

## Desviaciones del alcance (archivos fuera de la lista)
- `parity/fixtures/herdr.jsonl`: **fuera de la lista de archivos del brief**. Sin tocarlo la
  prueba de paridad queda en rojo (7 fallos: el `--name` que ve `hc` ahora es el slug), y
  el brief exige «Estado: completado» solo con las pruebas en verde y pide explícitamente
  la corrección del nombre. Se cambian solo las 7 expectativas `--name` de los casos de
  `run_agent` en segundo plano; el resto del archivo (incluidos los casos visibles, que
  siguen con la etiqueta legible) queda igual. Si el elefante regenera el fixture desde el
  oráculo de Python, esta diferencia se repite salvo que el Python aplique el mismo
  `hc_slug`.
- `rust/crates/ez-app/...` y `rust/crates/ez-herdr/tests/herdr_parity/launch.rs`: literales
  `RunAgent` que dejaban de compilar; solo se añade `..RunAgent::default()`, como permite el
  brief.

## Verificación
Desde `rust/`:
- `cargo fmt -p ez-herdr -p ez-app` y `cargo fmt -p ez-herdr -- --check` / `-p ez-app`:
  sin cambios pendientes (salida: `fmt ok`).
- `cargo clippy -p ez-herdr --all-targets -- -D warnings`: termina con
  `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 2m 15s` (sin avisos).
- `cargo test -p ez-herdr`: `198 passed; 0 failed` (lib), `1 passed` (agent_driver_parity),
  `1 passed` (herdr_parity), `2 passed` (metrics_parity), `27 passed` (permission),
  `1 passed` (tasks_activity_parity).
- `cargo test -p ez-app`: ver abajo.
- `cargo build --workspace`: `Finished \`dev\` profile … in 8m 07s` (todo el espacio de
  trabajo compila con los campos nuevos).

## Pendiente o riesgos
- La paridad con el oráculo de Python se aparta en el `--name` del lanzamiento en segundo
  plano (ver desviaciones): si el Python no aplica `hc_slug`, el fixture vuelve a exigir la
  etiqueta legible.
- `hc_slug` no es el validador de nombres de `hc`: solo garantiza el forma que `hc`
  acepta (1-32 de `a-z 0-9 - _ .`, empezando por letra o número, sin forma de dirección).
  `hc_name` con un nombre que `hc` rechace por otra razón devuelve el error de `hc`.
- La ubicación depende de `herdr-app` 0.1.16 (`terminal.keep`, `pane.focus`); no se probó
  contra el Herdr real en esta tarea (HD-6 lo hace con agentes falsos).
- `watcher::tests::arrancar_y_parar_no_se_cuelga` es inestable por temporización (falló una
  vez y pasó en las demás corridas, también antes de este cambio).

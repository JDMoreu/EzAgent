# Informe ECO-6b-r1 — La ventana en árbol (como Herdr)

## Resultado
- Brief/tarea: ECO-6b (rehecho: ventana de §15.6 con nombres de §15.9)
- Estado: completado

## Cambios
- `rust/crates/ez-hub/ui/index.html`: sustituye las tres columnas fijas por árbol a la izquierda, pestañas de terminal + barra en el centro y panel plegable de traza/tarjetas a la derecha; sin textos prohibidos en el HTML.
- `rust/crates/ez-hub/ui/css/app.css`: rejilla de tres columnas con panel derecho plegable, filas de árbol con punto de estado, pestañas desplazables, dos ranuras de terminal con 8 px de margen y sin la barra blanca del viewport de xterm.
- `rust/crates/ez-hub/ui/js/texts.js`: único lugar de los textos visibles y de la tabla de traducción de los 18 tipos de evento de la traza (idéntica a la de Rust).
- `rust/crates/ez-hub/ui/js/tree.js` (nuevo): árbol plegable y anidado sin límite de niveles; conserva plegado y selección entre refrescos de `hub://changed`; muestra estado, id y papel.
- `rust/crates/ez-hub/ui/js/tabs.js` (nuevo): abrir/enfocar/cerrar pestañas, divider único, y envío a la pestaña activa (`agent_send` real, `view_send` en demostración).
- `rust/crates/ez-hub/ui/js/panel.js`: traza traducida y tarjetas con acciones por tipo (tarea/equipo, pausa, integración); el rechazo del árbitro aparece en la propia tarjeta.
- `rust/crates/ez-hub/ui/js/main.js`: orquestación, formularios de «+ Espacio» y «+ Agente principal» (agente/modelo desde `inventory`), franja de estado degradado, nota de voz y autocomprobación con los siete campos del brief.
- `rust/crates/ez-hub/ui/js/spaces.js`: eliminado; su lectura del formato viejo (`space.elephant.agent`) ya no la usa nadie.
- `rust/crates/ez-hub/src/demo.rs` (nuevo): árbol de ejemplo (2 principales, 4 subagentes, uno anidado), 6 eventos y 1 tarjeta de equipo con la forma `{eventos, tarjetas, tarea}` del motor.
- `rust/crates/ez-hub/src/texts.rs`: `event_text(kind, data)` con la tabla de traducción y pruebas de cada tipo.
- `rust/crates/ez-hub/src/selfcheck.rs`: informe con `errores, nodos, pestañas, terminales_con_datos, eventos, tarjetas, textos_prohibidos` y su evaluación (7/2/2/6/1/0).
- `rust/crates/ez-hub/src/runtime_commands.rs`: `space_tree` y `hub_state` devuelven los datos de ejemplo en demostración, sin herdr-core.
- `rust/crates/ez-hub/src/commands.rs` y `lib.rs`: comando `view_send` (barra en demostración) y módulo `demo` registrado.

## Decisiones
- `texts.rs:3`: la tabla está en `texts.js` (único lugar visible) y se duplica en Rust porque el brief pide además pruebas Rust de cada tipo; el espejo rellena el campo `texto` de los eventos de ejemplo.
- `commands.rs:send_view_text`: `agent_send` exige el motor, inexistente en `--demo`; se añade `view_send` para que la barra siga escribiendo en la terminal de ejemplo (pegado entre corchetes, como los PTY directos).
- `demo.rs:tree`: el subagente con hijo recibe agente y modelo para que la pestaña abierta por defecto tenga una cabecera completa.
- Los eventos de muestra llevan `agente`, `modelo`, `blocking` y `major`: el `EventKind` del árbitro no incluye esos campos, así que los eventos reales muestran vacíos/ceros donde falten.
- `hub::demo_state` (5 eventos, forma antigua) permanece solo para las pruebas en memoria de `commands.rs`; la vista de demostración ya no lo consume.

## Verificación
- `cargo fmt -p ez-hub --check`: `FMT_OK` (sin diferencias).
- `cargo clippy -p ez-hub --all-targets -- -D warnings`: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 2.02s`.
- `cargo test -p ez-hub`: `test result: ok. 90 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.35s`; integración `tests/neutral_names.rs`: `1 passed; 0 failed`; binario y doc-tests: 0.
- `cargo run -p ez-hub --bin ezagent-hub -- --demo --selfcheck <temporal>.json`: código de salida **0**; salida:
  ```
  {"errores":[],"nodos":7,"pestañas":2,"terminales_con_datos":2,"eventos":6,"tarjetas":1,"textos_prohibidos":0}
  ```
- Límites: ningún `.rs` nuevo supera 600 líneas (máximo 227) ni función 60; sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`/`eprintln!`, `unsafe` ni `.unwrap()` fuera de pruebas. Los ocho módulos JS pasan `node --input-type=module --check`.

## Pendiente o riesgos
- Sin herdr-core real en esta ola: las pestañas normales se arman con `["hc", "attach", dirección]` pero no se comprobaron contra un `hc` en marcha (las pruebas usan dobles, conforme al alcance).
- La división de terminales conserva cada vista en su propio hueco; al cambiar de tamaño se reajusta con `fit`, pero xterm no recuerda el desplazamiento al mover una pestaña entre ranuras.
- El formulario «+ Agente principal» usa `inventory` en caché del motor; si el inventario está vacío (demostración) ofrece la lista fija de agentes conocidos.

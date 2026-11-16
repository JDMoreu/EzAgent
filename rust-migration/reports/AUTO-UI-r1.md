# AUTO-UI — Modo autónomo, fase UI: fila en la vista rápida, marca en la barra y parar desde la fila

## Resultado
- Brief/tarea: AUTO-UI (10.19 y la parte de 9.8 que es de la interfaz; manda la 10)
- Estado: completado

## Cambios
- `rust/crates/ez-hud/src/paint/marks.rs`: estado `autonomous` en la tabla de estilos de la órbita, como punto del color de acento (MINT, que sigue a la paleta y al tema personalizado), más su prueba de dibujo.
- `rust/crates/ez-hud/src/paint/peek.rs`: la fila `autonomous` no es descartable (no lleva ×) y el campo `agent` documenta que en esa fila lleva el detalle de la segunda línea.
- `rust/crates/ez-hud/src/paint/peek/text.rs`: `meta_line` devuelve el detalle tal cual para el estado `autonomous`, sin agente ni tiempo.
- `rust/crates/ez-hud/src/paint/peek/draw.rs`: icono de la marca de la fila (`sparkle` en menta) para el estado `autonomous`.
- `rust/crates/ez-hud/src/paint/peek/tests.rs`: pruebas del detalle sin edad y de que la fila no es descartable; se amplió la prueba de iconos con el estado nuevo.
- `rust/crates/ez-hud/src/win/peek/tests.rs`: la fila autónoma no tiene zona de × y pulsarla abre la fila (que pide confirmación).
- `rust/crates/ez-app/src/app/autonomous/wiring.rs`: `AUTONOMOUS_KEY`, `STOP_QUESTION` («¿Detengo el modo autónomo?»), `peek_row` (título, detalle y si lleva marca) y `stop_autonomous_from_row` (pregunta fuera de todo cerrojo y, con «sí», `stop_autonomous`).
- `rust/crates/ez-app/src/app/autonomous/mod.rs`: reexporta `peek_row` y `AUTONOMOUS_KEY` para la capa de actividad.
- `rust/crates/ez-app/src/app/activity/log.rs`: `activity_rows` compone la foto de la barra y antepone la fila fija del modo autónomo desde el resumen del bucle; `refresh_activity` publica esa foto; `open_activity` manda la fila autónoma a `ezagent-autonomo-stop`, que copia la `ConfirmationGate` y pregunta sin retener ningún cerrojo.
- `rust/crates/ez-app/src/app/activity.rs`: deja `activity_rows` visible para las pruebas del crate.
- `rust/crates/ez-app/src/app/autonomous/tests.rs` y `tests/ui.rs`: pruebas de la fila en cada estado, de la desaparición al parar, de la marca solo con la tarea activa y del ciclo pulsar → confirmación → «sí» detiene / «no» no detiene, con un `AgentRuntime` real y una `ConfirmationGate` de prueba.
- `rust-migration/reports/AUTO-UI-r1.md`: este informe.

## Decisiones
- `// DECISION` (`marks.rs`): 9.8 admite «un anillo o un punto»; se dibuja un punto de acento (peso 1.2 contra 1.5 de `done`) para que sea discreto y distinguible.
- `// DECISION` (`wiring.rs`): la fila se parte en las dos líneas de la vista rápida (título «Autónomo · ronda {n}» y detalle con el estado o el motivo); unidas con « · » son exactamente `Summary::describe`, y una prueba lo comprueba en todos los estados.
- `// DECISION` (`log.rs`): el detalle de la fila viaja en `PeekItem.agent` porque la vista rápida pinta ahí su segunda línea; añadir un campo obligaría a tocar la prueba de paridad y el mapeo de `ez-herdr`, fuera del brief.
- `// DECISION` (`wiring.rs`): la marca de la órbita acompaña a `running` y `verifying` (9.8); en `preparing` se ve la fila pero no la marca, y en `paused` no hay marca.
- `// DECISION` (`peek.rs`): la fila autónoma no es descartable; solo desaparece cuando la tarea termina o se detiene (10.19).
- `// DECISION` (`log.rs`): la confirmación se pide desde el hilo `ezagent-autonomo-stop`, nunca desde el de la barra; la puerta se copia y se suelta antes de `ask`, que puede tardar lo que tarde el usuario.
- `// DECISION` (`wiring.rs`): la confirmación de parada se pide con `strong = false` (el diseño solo pide `strong = true` para activar el modo).
- `// DECISION` (`tests/ui.rs`): la prueba del clic usa la puerta de verdad (`ConfirmationGate`) sobre un `AgentRuntime` con sus carpetas en un directorio temporal; sin runtime, pulsar la fila no hace nada, como en el arnés de reproducción.

## Verificación
- `cargo fmt --all`: limpio.
- `cargo clippy --workspace --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-hud -p ez-app`: **828 pruebas en verde**, 0 fallos (507 de `ez-hud`, 310 de `ez-app`, más las integraciones y las de paridad).
- `hud_peek_parity` y `hud_bar_parity`: en verde, sin tocar ninguna PNG; sin tarea autónoma el dibujo es idéntico.
- Pruebas nuevas (7):
  - `paint::marks::tests::the_autonomous_mark_is_an_accent_dot`
  - `paint::peek::tests::the_autonomous_row_is_not_dismissible_and_has_no_age`
  - `win::peek::tests::the_autonomous_row_has_no_close_zone_and_still_opens`
  - `app::autonomous::tests::ui::the_row_shows_the_round_the_state_and_the_reason`
  - `app::autonomous::tests::ui::the_row_asks_the_code_question_and_a_yes_stops_the_task`
  - `app::autonomous::tests::ui::a_no_leaves_the_task_running`
  - `app::autonomous::tests::ui::the_paused_row_keeps_the_reason_but_loses_the_orbit_mark`
- La prueba de iconos `paint_marks_uses_the_state_icon` se amplió con el estado `autonomous` (sigue en verde).
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`, `remove_dir_all`, `unsafe`, `.unwrap()` fuera de pruebas, `"cmd"`, `"/c"` ni `-Command`. Ningún `.rs` pasa de 600 líneas (el mayor tocado: `win/peek/tests.rs`, 546).

## Fixtures consumidos
- `parity/fixtures/hud_peek.jsonl`: 154 casos, consumidos por `hud_peek_parity` (incluye `peek.meta_line` y PNG calculados con el Python; ningún caso usa el estado nuevo porque la funcionalidad no existe en el Python).
- `parity/fixtures/hud_bar.jsonl`: 361 casos, consumidos por `hud_bar_parity`.
- Sin fixtures propios del dominio autónomo en esta fase (0 casos de paridad nuevos).

## Pruebas portadas (`parity/test-map.csv`)
- Ninguna fila nueva: es funcionalidad nueva aprobada por el usuario, sin Python de referencia en `src/`; `test-map.csv` no tiene entradas del modo autónomo. Las pruebas propias cubren los cinco puntos del brief: texto en cada estado, desaparición al terminar o detenerse, marca solo con tarea activa, confirmación al pulsar y «sí» detiene.

## Pendiente o riesgos
- **Prioridad de la marca en la órbita**: `orbit_states` (`rust/crates/ez-hud/src/win/drop/ui/render.rs`, fuera de los archivos del brief) ordena por `state_order` y deja `autonomous` en el grupo desconocido (9); con cinco o más marcas de otras tareas, `ORBIT_LIMIT = 5` la recorta. Arreglo de una línea cuando se pueda tocar ese archivo: `"autonomous" => 0`. La fila de la vista rápida no se ve afectada.
- La fila se refresca con el latido de actividad de la app (1,5 s): tras cambiar de ronda o de estado, el texto puede tardar hasta 1,5 s en actualizarse. Al detenerse desde la fila, la fila desaparece en el siguiente latido (la petición de parada se aplica antes, en el hilo `ezagent-autonomo`).
- Un motivo de pausa muy largo se recorta con «…» en la segunda línea (10,5 px); el texto completo sigue en `autonomous_status` y en `privado/estado.json`.
- `PeekItem.agent` lleva el detalle en la fila autónoma: cualquier consumidor futuro de ese campo para este estado debe saberlo.
- No se ejecutó la app real del HUD (ventanas Win32); las pruebas son del controlador y del dibujo, como el resto del crate.

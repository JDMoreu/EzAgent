# DX-4b r1 — El arranque falla cerrado y respeta la pausa previa del enjambre

Rama: `work/DX-4`, sobre la entrega DX-4. Brief: `rust-migration/briefs/DX-4b.md`.
Sin ventanas, sin la app de la persona, sin `whisper-server`, sin agentes reales.

## Resultado
- Brief/tarea: DX-4b
- Estado: completado

## 1. Fallo cerrado en la retención del enjambre

Antes, `hold_saved_swarms` anotaba el error de `store().live()` o de
`service.pause()` y seguía; después `start_pending_resume` arrancaba el
supervisor y un enjambre que seguía `active` podía despertar solo.

Ahora:

- `watch/resume.rs` gana el rasgo `SwarmHold` (inventario, pausa, reanudación,
  parada, `poke`, arranque completo y arranque solo del servidor). Producción lo
  cumple `StackHold` sobre el `SwarmStack`; las pruebas inyectan un doble en
  `AgentsState.swarm_hold` (vacío en producción).
- `hold_saved_swarms` devuelve `(enjambres, fallo)`. Si falla `live()`, el
  inventario queda vacío y el fallo se conserva. Si falla `pause()` de un
  enjambre activo, se sigue intentando el resto pero el fallo también se
  conserva.
- `collect_pending_resume` guarda ese fallo en `PendingResume.blocked`; el
  texto de la pregunta pasa a ser el aviso de fallo (código, sin LLM), que se
  escribe en el chat y se dice por voz. Las tareas y cadenas se conservan en el
  estado retenido y en `tasks.json`.
- `begin_pending_resume` devuelve si la retención es segura. Con fallo,
  `start_pending_resume_with` arranca **solo** el servidor local
  (`start_server`), nunca el supervisor (`start`). Así `ezs` sigue existiendo,
  pero ningún enjambre guardado se despierta.
- `answer_pending_resume`: con fallo, «sí», «dale» o «reanuda» no reanudan;
  se contesta con una frase del código («No reanudo nada…») y el estado sigue
  retenido. «No» lo deja anotado como declinado y «descártalo» sigue borrando
  tareas, cadenas y los enjambres conocidos.
- La reanudación aceptada corre en `ezagent-resume-pending` y sigue siendo la
  única ruta que llama a `resume_pending_tasks`/`check_task_now`.

## 2. La pausa previa se conserva

`HeldSwarm` gana `was_paused`. `live()` marca `was_paused` para los que ya
estaban en `paused` antes de abrir; solo se llama a `pause()` en los `active`.
Al aceptar, `resume_pending_resume` salta los `was_paused`: el «sí» recupera
los que la app pausó al abrir, no los que la persona ya tenía en pausa. Los
saltados tampoco reciben `poke`.

## 3. Pruebas (3 nuevas)

En `rust/crates/ez-app/src/app/watch/resume/tests/holds.rs`, con un `FakeHold`
que registra arranque, pausa, reanudación, parada y `poke`, y el `FakeHc` que
registra `hc run`/`hc do`/`hc send`:

1. `a_broken_board_blocks_the_start_and_keeps_everything`: `live()` falla.
   El supervisor no arranca (`started == 0`), solo el servidor
   (`server_started == 1`); el aviso de fallo queda en el chat; «dale» no
   reanuda; cero `run`/`do`/`send`; tarea y cadena siguen en `tasks.json`.
2. `a_swarm_that_cannot_be_paused_blocks_the_start`: falla `pause()` de un
   enjambre activo. Mismo cierre: supervisor sin arrancar, «sí» no reanuda,
   cero lanzamientos/envíos. «Descártalo» sí lo detiene (el doble recibe
   `stop`).
3. `the_yes_only_resumes_the_swarms_that_were_active`: un enjambre `active` y
   otro `paused`; al abrir solo se pausa el activo; el «sí» reanuda solo ese y
   hace un `poke`; el que ya estaba en pausa no se toca.

Las otras 9 pruebas de DX-4 siguen en `tests.rs` (436 líneas) y
`tests/holds.rs` (226); `resume.rs` queda en 583 líneas, todo bajo 600.

## 4. Cambios

- `rust/crates/ez-app/src/app/watch/resume.rs`: `SwarmHold` y `StackHold`,
  `HeldSwarm.was_paused`, `PendingResume.blocked`, aviso de fallo, arranque
  solo del servidor con fallo, reanudación filtrada por la pausa previa y
  `start_pending_resume_with` (costura de prueba del arranque).
- `rust/crates/ez-app/src/app/watch/resume/tests/holds.rs`: los dobles y las 3
  pruebas nuevas.
- `rust/crates/ez-app/src/app/watch/resume/tests.rs`: el `mod holds;` y las
  pruebas de DX-4 sin cambios de conducta.
- `rust/crates/ez-app/src/app/core/state.rs` y `core/init.rs`: campo
  `swarm_hold` (vacío en producción; solo lo llenan las pruebas).

## 5. Decisiones

- `// DECISION` en `begin_pending_resume`: si no se puede inventariar o pausar
  el enjambre, la app falla cerrada; informa el fallo, conserva lo pendiente y
  no reanuda nada.
- `// DECISION` en `start_pending_resume`/`start_pending_resume_with`: con fallo,
  solo arranca el servidor local; el supervisor no arranca. Es lo que mantiene
  `ezs` sin despertar enjambres guardados.
- Con un fallo, un «sí» posterior tampoco reanuda: la persona tendría que
  reiniciar (o descartar). Es deliberado: no se puede asegurar la pausa.
- El motivo del fallo se recorta a 160 caracteres y lo escribe el código; nunca
  el LLM.
- Si falla `live()`, el inventario de enjambres queda vacío; «descártalo» no
  puede detener los que no se conocen. No hay supervisor, así que la app no los
  despierta; la próxima apertura vuelve a inventariarlos.
- Si falla `pause()` de uno, se intenta pausar los demás antes de devolver el
  fallo (minimiza el daño sin ocultarlo).

## 6. Verificación

- `cargo fmt -p ez-app -- --check`: sin salida, código 0.
- `cargo clippy -p ez-app --all-targets -- -D warnings`: código 0;
  `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.79s`.
  (Los 14 avisos de `freetype-sys` son del crate vendorizado, no de `ez-app`.)
- `cargo test -p ez-app`: código 0.
  ```
  test result: ok. 381 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 733.96s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.06s
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.33s
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.10s
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.78s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- No se abrió ninguna ventana, no se lanzó la app, no se ejecutaron agentes
  reales y no se tocó la app, el `whisper-server` ni la ventana de Herdr de la
  persona. Las pruebas usan `APPDATA`, `EZAGENT_LOGS_DIR` y carpetas temporales
  propias.

## 7. Pendiente o riesgos

- Con un fallo de retención, la sesión queda sin supervisor del enjambre: un
  enjambre nuevo que se cree en esa sesión no lanzará a sus agentes hasta
  reiniciar. Es el precio de no despertar los guardados; queda dicho en el
  aviso («No reanudo nada»).
- Si falló `pause()` de un enjambre, su registro sigue `active` en el tablero;
  la app no lo despierta (sin supervisor). Agentes externos que ya estuvieran
  vivos podrían seguir actuando por su cuenta, como antes; la app no mata
  procesos que no lanzó.
- Si falla `live()`, «descártalo» no conoce los enjambres y no los detiene; las
  tareas y cadenas sí se borran. La próxima apertura los vuelve a inventariar.
- Si `service.resume` falla al aceptar, ese enjambre se queda en pausa (seguro)
  y se registra el error; no se reintenta solo.

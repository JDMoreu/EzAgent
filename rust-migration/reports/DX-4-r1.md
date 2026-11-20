# DX-4 r1 — Al abrir la app, nada pendiente se reanuda solo: se pregunta

Rama: `work/DX-4` (copia de `eco/main`). Brief: `rust-migration/briefs/DX-4.md`.

## Resultado
- Brief/tarea: DX-4
- Estado: completado

## 1. Inventario: qué podía actuar solo tras reiniciar

Líneas de esta rama (después del cambio; las de `startup.rs` y `watch.rs` se
movieron al añadir la retención).

| # | Camino | Archivo:línea | Qué hacía | Cómo queda |
|---|---|---|---|---|
| 1 | Arranque | `ez-app/src/app/startup.rs:189-199` | Llamaba a `swarm.start()` y a `hooks.resume_pending_tasks()` en un hilo. | Llama a `start_pending_resume`: retiene y pregunta; no reanuda. |
| 2 | Enjambre guardado | `ez-swarm/src/stack.rs:160-166` + `supervisor.rs:421-445` | `swarm.start()` abría el servidor y el supervisor; la primera pasada procesaba los enjambres `active` y la recuperación volvía a abrir/despertar sus agentes. | El servidor y el supervisor arrancan (los necesita `ezs`), pero antes cada enjambre vivo se deja en `paused` (`service.rs:294`) y el supervisor no lo toca; el «sí» lo devuelve a `active` (`service.rs:301`) y hace `poke`. |
| 3 | Vigilancia restaurada | `ez-app/src/app/watch.rs:425-442` (`resume_pending_tasks`) | Volvía a vigilar todas las tareas y cadenas y llamaba a `check_task_now`. | Solo se llama con el «sí» (`watch/resume.rs:355-375`). |
| 4 | Cadena que sigue sola | `ez-app/src/app/activity/tasks.rs:366-421` (`check_task_now`) → `:119-149` (`launch_chains`) → `:239-285` (`spawn_chain`) → `:276` (`herdr.run_agent`) | Si la fuente ya estaba `done`, lanzaba el paso siguiente al arrancar. | No se llama al arrancar; con el «sí» se comporta como antes. |
| 5 | Aviso del vigilante de Herdr | `ez-app/src/app/watch.rs:80-127` (`on_watcher_event`), cadena en `:101-102` | Un `done` de una fuente vigilada lanzaba su cadena. | El vigilante arranca, pero `watched` está vacío hasta el «sí» (`core/callbacks.rs:147-151`), así que no tiene objetivos ni dispara cadenas. |
| 6 | Vigilante de subagentes `voz` (NV-3b) | `ez-app/src/app/subagents.rs:55-101`, aviso en `:107-132`, oferta en `:183-185`, respuesta en `:249-268`, reintento en `:301-330` y `:386` | Anuncia bloqueos/fallos y ofrece `ez retry`. | Igual; es pasivo. El reintento exige «sí»/«dale» (`answer_retry_offer`). El vigilante de la app no instala `AutoRetry` (`ez-herdr/src/subagents/watch.rs:211-215`): solo `ez watch` lo hace. |
| 7 | Reintento automático de `planes.toml` | `ez-ctl/src/local/retry.rs:395` (`automatic_retry_enabled`) y `ez-ctl/src/local/watch.rs` | `ez watch` puede relanzar tras una novedad si la persona lo permitió. | Es un proceso/gancho aparte que la app no arranca; no cambia con este brief. Si la persona lo instaló, actúa por su cuenta, no por abrir EzAgent. |
| 8 | Confirmación pendiente (F48) | `ez-agent/src/runtime.rs:501-505` (`arm_confirmation`), llamado desde `ez-app/src/app/events/branches.rs:163-167`; estado en `ez-agent/src/confirm.rs` | Rearma el plazo de la confirmación vigente al terminar su TTS. | Nada de eso se guarda en disco: un reinicio crea otro `AgentRuntime` y no hay confirmación que rearmar. La pregunta del arranque es un `announce`, nunca un `confirm`. No hizo falta tocar `ez-agent`. |
| 9 | Modo autónomo | `ez-app/src/app/autonomous/host/restore.rs:15-79` | Ya dejaba la tarea viva en `paused` (`Action::Pause`) al reiniciar. | Sin cambios; fue el modelo. |
| 10 | Historial del chat | `ez-app/src/app/core/init.rs:199-201` (contextos), `<pending_tasks>` en `ez-herdr/src/tasks.rs:485-510`, `<swarms>` en `ez-swarm/src/stack.rs:203-230` | El contexto viejo y las listas de pendientes llegan al LLM. | Nada llama al LLM al arrancar: la pregunta la contesta código (`events/branches.rs:67-71` por voz y `chat.rs:372-377` por escrito) antes del atajo y del LLM. Riesgo residual en §5. |

## 2. Cambios

- `rust/crates/ez-app/src/app/watch/resume.rs`: módulo nuevo. Retiene tareas, cadenas y enjambres; escribe la pregunta única en español (código, nunca el LLM); interpreta «sí / dale / reanuda», «no / déjalo / después» y «descártalo»; pausa/reactiva enjambres y borra lo pendiente.
- `rust/crates/ez-app/src/app/watch/resume/tests.rs`: 9 pruebas con los dobles (tarea, cadena, enjambre, «no», «descártalo», «reanuda lo pendiente», chat y texto de la pregunta).
- `rust/crates/ez-app/src/app/watch.rs`: declara el submódulo y reexporta `PendingResume`/`HeldSwarm`.
- `rust/crates/ez-app/src/app/core/state.rs` y `core/init.rs`: campo `pending_resume` en `AgentsState` (vacío al construir).
- `rust/crates/ez-app/src/app/startup.rs`: el arranque llama a `start_pending_resume`; ya no arranca el enjambre sin más ni reanuda tareas directamente.
- `rust/crates/ez-app/src/app/events/branches.rs`: la respuesta hablada a la pregunta se atiende antes de la oferta de reintento, del atajo y del LLM.
- `rust/crates/ez-app/src/app/chat.rs`: la misma respuesta escrita en el panel.
- `rust/crates/ez-app/src/app/subagents.rs`: `plain_key` pasa a `pub(crate)` para no duplicar el normalizador de respuestas.
- `rust/crates/ez-app/src/app/core/tests.rs`: el arranque ya no espera `resume_pending_tasks` del gancho.

## 3. Decisiones

- `// DECISION` en `resume.rs`: se pregunta como aviso (`announce`) y no como confirmación, para que `arm_confirmation` (F48) no rearme nada de una sesión anterior.
- `// DECISION` en `resume.rs`: no se vigila de forma pasiva hasta el «sí»; una cadena podría lanzar su paso siguiente sola, que es justo lo que se evita. (El brief lo permitía, no lo exigía.)
- `// DECISION` en `resume.rs`: el servidor y el supervisor del enjambre sí arrancan (los usa `ezs` durante la sesión), pero los enjambres guardados quedan en pausa antes de la primera pasada del supervisor; solo el «sí» los reactiva. Es la desviación deliberada del «no se arranca» literal: no arrancar el servidor dejaría sin `ezs` a la sesión.
- `// DECISION` en `resume.rs`: tras un «no», solo «reanuda»/«retoma» retoman lo guardado; un «sí» suelto posterior puede responder a otra pregunta y no se consume.
- `// DECISION` en `resume.rs`: la reanudación corre en el hilo `ezagent-resume-pending` porque `check_task_now` habla con Herdr y puede lanzar la cadena.
- `// DECISION` en `resume.rs`: con los avisos silenciados la pregunta queda solo en el chat, sin hablar.
- Sin cambios en `ez-agent`: las confirmaciones son en memoria y no sobreviven al reinicio (§1.8).
- Sin cambios en `ez-hud`, `ez-herdr`, `ez-swarm` ni `ez-ctl`: se usan sus APIs (`service.pause`, `service.resume`, `supervisor.poke`, `store().live()`).

## 4. Verificación

- `cargo fmt -p ez-app -- --check`: sin salida, código 0.
- `cargo clippy -p ez-app --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 5.72s`, código 0. (Los 14 avisos de `freetype-sys` son del crate vendorizado, no de `ez-app`.)
- `cargo test -p ez-app`: código 0.
  ```
  test result: ok. 378 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 256.65s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.26s
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- Pruebas nuevas: 9 en `app::watch::resume` (todas en verde).
- No se abrió ninguna ventana, no se lanzó la app ni `diag_chat_smoke`, no se ejecutaron agentes reales y no se tocó la app, el `whisper-server` ni la ventana de Herdr de la persona.
- Dos pruebas ajenas a este cambio (`subagents::tests::watch::…bloquea_y_avisa…` y `events::tests::a_second_ack…`) fallaron en corridas intermedias con la máquina muy cargada y pasaron solas y en la corrida final completa (§5).

## 5. Pendiente o riesgos

- **Historial del chat**: tras un «no», la petición vieja sigue en el historial y en `<pending_tasks>`. No se reanuda sola (no hay turno de LLM al arrancar), pero una instrucción ambigua *posterior* de la persona podría hacer que el LLM la retome con sus herramientas. No se borró el historial porque sería destructivo y no lo pidió el brief; si el elefante quiere, se puede archivar o marcar como «descartada» en el contexto.
- **Enjambre pausado a propósito**: el tablero no distingue «en pausa porque la persona quiso» de «en pausa por reinicio»; al abrir se pregunta por cualquiera que esté `paused`. Sigue en pausa hasta el «sí».
- **«Descártalo» en un enjambre**: detiene su registro (`stopped`), pero no mata terminales que sigan vivas: matar procesos externos no es tarea de la app y el contrato lo prohíbe.
- **Silencio**: con los avisos silenciados (`mute_until`) la pregunta no se dice; queda en el chat.
- Las dos pruebas flojas son de temporización y ajenas a DX-4 (el aviso del vigilante se marca en otro hilo y el acuse espera un `SpeakDone`). En la corrida final completa quedaron en verde; si vuelven a fallar con la máquina cargada, conviene darles una espera explícita, pero no se tocaron aquí para no invadir NV-3/DX-2.

## 6. Desviaciones del brief o del diseño

- El brief dice «enjambre guardado → no se arranca hasta el sí». Se interpretó como «no se despierta»: el servidor y el supervisor local sí arrancan para que `ezs` funcione en la sesión, pero los enjambres guardados se pausan antes de la primera pasada y solo el «sí» los reactiva. Queda probado que el estado sigue `paused` hasta el «sí» (`a_saved_swarm_waits_paused_for_the_yes`).
- No se tocó `ez-agent`: la confirmación pendiente de F48 es en memoria y no sobrevive al reinicio (§1.8).

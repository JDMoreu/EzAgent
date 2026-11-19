## Resultado
- Brief/tarea: ECO-2 — `ez-arbiter` (tareas, rondas, reglas, verificación).
- Estado: completado.

## Cambios
- `rust/crates/ez-arbiter/src/lib.rs`: API pública del crate (`Arbiter`, traits, comandos, eventos) y ejemplo de uso compilable.
- `rust/crates/ez-arbiter/src/types.rs`: `Clock`, `SystemClock`, `FishLauncher`, `InstructionWriter`, `LaunchRequest`, `Space`, `Caller`, `Command`, `Verdict`, `Outcome`, `Rejection`.
- `rust/crates/ez-arbiter/src/events.rs`: `ArbiterEvent` (traza R12, con token redactado en disco) y `ElephantEvent` (mensajes para `ez wait`).
- `rust/crates/ez-arbiter/src/texts.rs`: todos los textos visibles, en español neutro; literales de git, corrección y dos rondas limpias.
- `rust/crates/ez-arbiter/src/paths.rs`: rutas de tarea/ronda, escritura atómica, sha256, resolución de rutas dentro del espacio.
- `rust/crates/ez-arbiter/src/criteria.rs`: lectura y validación de `criterios.toml` (sección 5) y líneas de tarjeta.
- `rust/crates/ez-arbiter/src/git.rs`: fotos HEAD + `status --porcelain=v1 -z`, `diff --name-only`, exclusión de `.ez/`.
- `rust/crates/ez-arbiter/src/exec.rs`: `Command` sin shell y con `CREATE_NO_WINDOW` en Windows.
- `rust/crates/ez-arbiter/src/verify.rs`: R8, un hilo por criterio, lectura de salidas en hilos, `try_wait` cada 50 ms, `kill` en el tope y cola de 4000 caracteres.
- `rust/crates/ez-arbiter/src/state.rs`: estado persistente (tarea, rondas, peces, informes, atribuciones), instantánea para la vista.
- `rust/crates/ez-arbiter/src/arbiter/mod.rs`: `Arbiter::new` (recarga de tareas no finales), `register_space`, `execute`, acciones de la persona, señales de hc, `wait_for_elephant`, `subscribe`, `snapshot`.
- `rust/crates/ez-arbiter/src/arbiter/emit.rs`: `commit` sin candados durante E/S, `estado.json`, `bitacora.jsonl`, rechazos de R12.
- `rust/crates/ez-arbiter/src/arbiter/load.rs`: recarga de `tareas/*/estado.json` al abrir el árbitro.
- `rust/crates/ez-arbiter/src/arbiter/task_cmds.rs`: R2, R3, `Status`, aprobaciones, `pause`, `resume`, `stop`, R10.
- `rust/crates/ez-arbiter/src/arbiter/team.rs`: propuesta y aprobación del equipo.
- `rust/crates/ez-arbiter/src/arbiter/round.rs`: R4, R8, R9, R11 y paradas por rondas, error repetido y tiempo.
- `rust/crates/ez-arbiter/src/arbiter/fish.rs`: R5, R6, R7, id de pez, etiquetas, entorno y archivos de la ronda.
- `rust/crates/ez-arbiter/tests/arbiter.rs` y `tests/arbiter/*.rs`: 33 pruebas de integración con `git init` real, lanzador/escritor/reloj falsos y carpetas temporales únicas.

## Decisiones
- `// DECISION` en `events.rs`: el token viaja en el evento en vivo (`fish_launched`) pero la bitácora lo escribe como `***`, porque las claves no se registran en disco.
- `// DECISION` en `fish.rs`: el QA exige al menos un desarrollador informado; sin él se interpreta que «todos informaron» aún no se cumple.
- `// DECISION` en `round.rs`: las paradas posteriores al cierre de ronda (`tope_rondas`, `mismo_error`, `tope_tiempo`) se devuelven con `Ok` y se anuncian en el mensaje; la ronda ya quedó cerrada.
- `// DECISION` en `task_cmds.rs`: título vacío se sustituye por «Tarea» para que la tarjeta sea lega.
- `// DECISION` en `state.rs`: la atribución compara rutas exactas; el cambio de contenido de un archivo sin seguimiento no aparece en `git status`.
- Sin `// DECISION` en el resto: los estados de aprobación y propuesta usan las reglas más cercanas (`R2` para estado de tarea, `R5` para equipo/papel), porque el diseño numera R1–R11 y no da un número para esas transiciones.

## Verificación
- `cargo fmt --all --check`: `FMT_EXIT=0` (sin diferencias).
- `cargo clippy -p ez-arbiter --all-targets -- -D warnings`: `Finished \`dev\` profile ...`, `CLIPPY_EXIT=0`.
- `cargo test -p ez-arbiter`:
  - `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s` (unitarias).
  - `test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.42s` (integración R1–R12).
  - `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s` (doc-test).
- Cobertura de reglas: R1 `r1_*`, R2 `r2_*`, R3 `r3_*`, R4 `r4_*`, R5 `r5_*`, R6 `r6_*`, R7 `r7_*`, R8 `r8_*`, R9 `r9_*`, R10 `r10_*`, R11 `r11_*`, R12 `r12_*`. Cada regla tiene prueba de rechazo/fallo y de cumplimiento.
- Ningún `.rs` supera 600 líneas (máximo 407) ni ninguna función 60 líneas (salvo declaraciones de trait, sin cuerpo).

## Pendiente o riesgos
- No se tocó ningún archivo fuera de `rust/crates/ez-arbiter/**`; `Cargo.toml`, `Cargo.lock` y los contratos de `ez-team` quedaron intactos.
- `wait_for_elephant` clona el receptor de un canal por espacio: dos esperadores simultáneos del mismo espacio se repartirían los eventos. El diseño prevé un solo elefante por espacio.
- La detección de archivos protegidos depende de `git status`; un cambio de contenido en un archivo ya sin seguimiento no se atribuye. Queda para el elefante decidir si se refuerza con hashes.
- No se emite evento para el elefante en `stop` ni en `task_closed`: no figuran en la lista de §8 «Eventos para el elefante» (sí están en la traza de la vista).
- `EZ_URL` y `PATH` los añade el lanzador de la ventana, según el brief; el lanzador falso solo comprueba `EZ_ROLE`, `EZ_FISH`, `EZ_TOKEN`, etiquetas y nombre hc.
- Las pruebas de tope usan `ping -n 6 127.0.0.1` (Windows), como pide el brief; el tiempo real de esa prueba es ~1 s por el `kill`.

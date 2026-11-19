# Informe ECO-9-r1 — Correcciones de la prueba de punta a punta

## Resultado
- Brief/tarea: ECO-9 (rama `work/ECO-9`, copia de `eco/main`)
- Estado: completado

## Cambios
- `rust/crates/ez-hub/src/runtime/handler.rs`: `ArbiterApi::wait` recibe el `Caller` y enruta a `wait_for_main(&ElephantKey)` para el agente principal y a `wait_for_fish(&FishKey)` para el subagente; antes llamaba siempre a `wait_for_elephant(space)`.
- `rust/crates/ez-hub/src/runtime/tests/handler.rs`: el doble registra los `Caller` que llegan a `wait` y nueva prueba que comprueba el enrutamiento de ambos.
- `rust/crates/ez-arbiter/src/arbiter/api.rs` y `mod.rs`: se elimina `wait_for_elephant(space)` (ya sin usuarios de producto); las pruebas ahora usan `wait_for_main(&ElephantKey::default_for("s1"))`.
- `rust/crates/ez-arbiter/src/arbiter/fish/order.rs`: `lower_stage_waiting` excluye los papeles `first_round_only` de la espera por etapas en rondas posteriores a la 1, tanto en trabajos pendientes como en verificadores.
- `rust/crates/ez-arbiter/tests/arbiter/round.rs`: prueba con `plan_reviewer` en su etapa sugerida (2): la ronda 2 solo de verificación (qa + qa_validator) arranca y cierra limpia.
- `rust/crates/ez-arbiter/src/arbiter/agents.rs`: `exclude_ez` añade `.ez/` a `info/exclude` del repositorio común al crear cada agente principal; `commit_work` confirma el trabajo sin confirmar del worktree en `ez/<id>` con el mensaje del brief y devuelve si hubo cambios.
- `rust/crates/ez-arbiter/src/arbiter/agents.rs` (prueba unitaria): `commit_work` ignora `.ez/`, confirma una obra nueva y distingue el caso sin cambios.
- `rust/crates/ez-arbiter/src/texts.rs`: `integrate_card` avisa «No había cambios nuevos que confirmar en esa rama.» cuando no se confirmó nada.
- `rust/crates/ez-arbiter/src/arbiter/task_cmds.rs`: `task_close` confirma el trabajo del worktree antes de abrir la tarjeta de integración y le pasa el resultado.
- `rust/crates/ez-arbiter/tests/arbiter/main_agents.rs`: la integración sin commits previos del banco lleva los archivos del ejecutor a la rama base, `.ez/` no entra en el commit y `.ez/` queda en `info/exclude`.
- `rust/crates/ez-hub/examples/e2e_fake.rs`, `fixtures.rs`, `scenario.rs`: se dejan de sembrar `agentes.json`, `elefantes.json`, `principales.json` y el worktree fijo; el ejemplo usa `main_agent_create` normal (id aleatorio), `plan_reviewer` en su etapa sugerida, orden por etapas en la ronda 1 y ningún `git commit` del banco de pruebas.

## Decisiones
- `// DECISION` en `arbiter/agents.rs`: `commit_work` usa `:(exclude):(top).ez` en vez de la `:(exclude).ez` literal del brief. Con `.ez/` en `info/exclude`, git rechaza excluir explícitamente esa ruta («ignored by one of your .gitignore files»); la forma anidada funciona. `commit_work` llama antes a `exclude_ez` para garantizar la condición también en worktrees heredados.
- `wait_for_elephant(space)` se eliminó porque su único usuario de producto era el adaptador ya corregido; las pruebas restantes usan `wait_for_main` con la clave por omisión.
- El número de ronda del mensaje de commit es `task.round` (la ronda vigente al cerrar); el brief no fija otro criterio.

## Verificación
- `cargo fmt -p ez-arbiter -p ez-hub --check`: exit 0 (`FMT_OK`).
- `cargo clippy -p ez-arbiter -p ez-hub --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 4.48s`, exit 0, sin avisos.
- `cargo test -p ez-arbiter -p ez-hub`: **185 passed / 0 failed / 0 ignored** (17 de la unidad de `ez-arbiter`, 74 de `tests/arbiter.rs`, 92 de la unidad de `ez-hub`, 1 de `tests/neutral_names.rs` y 1 doc-test; el resto son 0 pruebas de binarios y doc-tests vacíos).
- `cargo run -p ez-hub --example e2e_fake`: exit **0**. Salida literal:
  ```
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.78s
       Running `target\debug\examples\e2e_fake.exe`
  Motor normal en http://127.0.0.1:47830; agente principal e6595a29b.
  Agente principal e6595a29b lanzado en t91.
  Aprobada la tarjeta c1c8463edfef8 (task).
  Aprobada la tarjeta cb70240603cb7 (team).
  Integrada la rama desde la tarjeta c02a482779b29.
  Éxito: task_closed, integrated, 2 rondas limpias, 2 archivos fake y merge a6cbef3f.
  ```
- Límites: el `.rs` más largo tocado tiene 599 líneas (`task_cmds.rs`); ninguna función nueva supera 60 líneas; sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`/`eprintln!`, `unsafe` ni `.unwrap()` fuera de pruebas; el único `remove_dir_all` está al final de una prueba, sobre una carpeta temporal con uuid. Git se ejecuta fuera de los candados del árbitro.

## Pendiente o riesgos
- La exclusión depende del pathspec anidado `:(exclude):(top).ez`, comprobado con el git de esta máquina; `info/exclude` por sí solo ya mantiene `.ez/` fuera del commit.
- El ejemplo usa un `hc` real, cierra solo sus propias terminales y no detiene el servidor `hc` que arranca el motor.
- No se probó el sandbox de Codex que impide ejecutar `ez.exe` (M07, a criterio de la persona).

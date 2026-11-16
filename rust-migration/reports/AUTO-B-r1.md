# AUTO-B — Modo autónomo, fase B: el bucle multiagente sobre el enjambre (10.18)

## Resultado
- Brief/tarea: AUTO-B (10.18 completo, con 10.1-10.13 y 10.16 de fondo; manda la 10)
- Estado: completado

## Cambios

### `ez-swarm` (cambios mínimos y aditivos)
- `rust/crates/ez-swarm/src/supervisor.rs`: conjunto `autonomous` de enjambres del modo autónomo, `mark_autonomous`/`is_autonomous`, y `autonomous.txt` en la carpeta base para que un reinicio de la app recuerde que ese enjambre no puede lanzar a sus miembros con los argumentos que saltan permisos (10.18).
- `rust/crates/ez-swarm/src/supervisor/launch.rs`: `launch_request` recibe si el enjambre es autónomo y quita `--yolo` y `--dangerously-skip-permissions` de los `swarm_args` de ese enjambre; el resto (`--allowedTools Bash(ezs *)`) se conserva.
- `rust/crates/ez-swarm/src/recovery.rs`: `mark_autonomous` y silencio de la recuperación para esos enjambres: no se lanzan preguntas ni avisos de incidente (la única noticia la da el bucle autónomo).
- `rust/crates/ez-swarm/src/service.rs` + `service/lifecycle.rs`: `start` se parte en `create`; `start_autonomous` ejecuta un gancho con el id **antes** de activar el enjambre (así el primer lanzamiento ya es sin `--yolo`); `fix_task` crea la tarea `fix` con su nota, reabre un tablero `done` y respeta el tope de 60 tareas. `service.rs` baja de 600 líneas al mover la creación a `service/lifecycle.rs`.
- `rust/crates/ez-swarm/src/store/tasks.rs`: `set_task_note` (la nota de la tarea `fix`).
- `rust/crates/ez-swarm/src/service/commands.rs`: `describe_task` muestra la nota de una tarea pendiente (el tablero no tiene columna de cuerpo; la ruta de `correccion-N.md` viaja ahí y el miembro tiene que verla al tomarla).
- `rust/crates/ez-swarm/src/stack.rs`: `start_autonomous_team` (marca el enjambre antes de activarlo y despierta al supervisor).
- `rust/crates/ez-swarm/src/supervisor/tests/autonomous.rs` (nuevo) y `service/tests.rs`, `stack.rs`: pruebas de los permisos sin `--yolo`, del reinicio que lo recuerda, del silencio de la recuperación, de las tareas `fix` y de la nota visible con `ezs next`.

### `ez-agent`
- `rust/crates/ez-agent/src/autonomous/mod.rs`: `StartSpec` y `TeamSpec` con `verifier_model`; `TeamSpec` con `verifier_agent` y `verifier_model` (B-F05 y 10.18).
- `rust/crates/ez-agent/src/autonomous/tools/start.rs` y `schema.rs`: las dos herramientas de arranque aceptan `verifier_agent` (también el equipo) y `verifier_model`; el equipo ya no confirma para luego fallar (el host lo arranca de verdad).
- `rust/crates/ez-agent/src/autonomous/texts.rs`: `VERIFIER_MODEL_DESCRIPTION`, `TEAM_VERIFIER_DESCRIPTION` y los motivos de pausa del equipo (`REASON_TEAM_INCIDENT`, `REASON_TEAM_STOPPED`, `REASON_TEAM_ROLE`); `TEAM_UNAVAILABLE` pasa a decir que falta el modo enjambre en la instalación.
- `rust/crates/ez-agent/src/autonomous/tests/tools.rs`: `verifier_model` en las dos herramientas y datos del equipo.

### `ez-app`
- `rust/crates/ez-app/src/app/autonomous/team.rs` (nuevo): `TeamSwarm` (lo que el bucle necesita del enjambre), `StackSwarm` sobre `SwarmStack`, `start_team_task`/`build_team` (papeles normalizados con los alias de `tools_swarm`, `goal = task`, `workdir`, estado con `mode: team` y `team: {swarm, roles}`, miembros en `scope` con papel `member` y `edit_dirs = [workdir]`), `tick_team` (incidentes, miembros, tablero vacío → verificación, permisos de los miembros) y `team_correction` (una tarea `fix` por hallazgo bloqueante o mayor, a `dev`, si no al primer papel que no sea `plan` ni `qa`, si no pausa).
- `rust/crates/ez-app/src/app/autonomous/mod.rs`: `AutonomousLoop` recibe el enjambre del equipo y `Session` lleva `verifier_model` y los incidentes vistos en el tic anterior.
- `rust/crates/ez-app/src/app/autonomous/driver.rs`: `tick_running` delega en el equipo; `SendCorrection` va al tablero en modo equipo; `poll_other` es reutilizable por los miembros.
- `rust/crates/ez-app/src/app/autonomous/verify.rs`: el verificador se lanza con `verifier_model`; `write_correction` se comparte entre el ejecutor único y el equipo.
- `rust/crates/ez-app/src/app/autonomous/host.rs`, `host/start.rs`, `host/restore.rs`: `start_team` real; `run_agent` del verificador con modelo; la tarea de equipo restaurada usa como verificador el primer papel.
- `rust/crates/ez-app/src/app/autonomous/wiring.rs`: el bucle recibe el `SwarmStack` de la app como `StackSwarm` (si existe).
- `rust/crates/ez-app/src/app/autonomous/tests.rs`, `tests/team.rs` (nuevo): doble en memoria del enjambre y 8 pruebas del modo B.

## Decisiones
- `// DECISION` (`team.rs`): la tabla de alias de papeles se copia de `tools_swarm` porque ese archivo no es de esta tarea; da los mismos nombres cortos (`planificador` → `plan`, `desarrollador` → `dev`).
- `// DECISION` (`team.rs`): el estado del equipo se publica desde la app (`state.team`, `state.state = Running`) porque `AutonomousState` no tiene una transición pura de arranque del equipo y el brief limita los cambios de `ez-agent`.
- `// DECISION` (`team.rs`): sin `verifier_agent`, el verificador del equipo es el agente del primer papel (B-F05: mismo CLI por omisión; en un equipo no hay un agente único).
- `// DECISION` (`team.rs`): el cuerpo de la tarea `fix` viaja en `result` (el tablero no tiene columna de cuerpo) y `describe_task` lo muestra al miembro; así la ruta de `correccion-N.md` llega a quien la toma.
- `// DECISION` (`team.rs`): un incidente del enjambre pausa solo si sigue activo dos tics; un incidente de contexto que la recuperación resuelve en la misma pasada no debe pausar el bucle.
- `// DECISION` (`team.rs`): al crear tareas `fix` sobre un tablero `done`, el servicio lo devuelve a `active`; si no, el supervisor no despertaría a nadie.
- `// DECISION` (`Restore`): `estado.json` no guarda el verificador; una tarea de equipo restaurada usa el agente de su primer papel.
- `// DECISION` (`Recovery`): en modo autónomo se callan también los avisos de incidente del enjambre, no solo las preguntas: la única noticia es la pausa del bucle.
- `// DECISION` (`service/commands.rs`): `describe_task` añade `· {nota}` a una tarea pendiente con `result`; ningún flujo del Python deja `result` en una tarea `pending` sin rondas, así que la paridad no cambia.
- `// DECISION` (`supervisor.rs`): los enjambres autónomos se apuntan en `autonomous.txt` dentro de la carpeta base; en memoria sola, un reinicio de la app los devolvería a `--yolo` (el riesgo de seguridad que 10.18 evita).
- `// DECISION`: `service.rs` se parte en `service/lifecycle.rs` y las pruebas del supervisor en `supervisor/tests/autonomous.rs` para respetar el tope de 600 líneas tras `cargo fmt`.

### Desviaciones del brief
- `rust/crates/ez-swarm/src/service/commands.rs` no estaba en la lista de cambios aditivos de `ez-swarm`; se toca una línea de `describe_task` para que el cuerpo de la tarea `fix` (la ruta de `correccion-N.md`) llegue al miembro que la toma. Sin eso, «crear tareas `fix`» quedaría a medias.
- `rust/crates/ez-agent/src/autonomous/texts.rs` recibe tres motivos de pausa y dos descripciones nuevas: son textos generados por el código que 10.18 necesita y no caben en otro crate.

## Verificación
- `cargo fmt --all --check`: limpio.
- `cargo clippy --workspace --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-app -p ez-agent -p ez-swarm`: **316 + 248 + 119 pruebas de librería en verde** (más las de integración), 0 fallos. El módulo autónomo de `ez-app` pasa de 13 a **21 pruebas** (8 del modo B); `ez-swarm` de 117 a **119**; `ez-agent` mantiene 248 con dos pruebas ampliadas.
- `cargo test -p ez-app --features replay --test replay_parity`: **3 escenarios en verde** (25 escenarios; 18-20 de enjambre sin cambios). Requiere `cargo build -p ez-fakes --bins` antes (el arnés usa `fake-llm.exe`); sin eso falla por el binario ausente, no por el código.
- Pruebas del brief: arranque del equipo con papeles normalizados y miembros cubiertos; miembros sin `--yolo` (y `--allowedTools Bash(ezs *)` conservado); `--yolo` intacto en un enjambre normal; reinicio que recuerda el enjambre autónomo; verificación al vaciarse el tablero (dos rondas limpias → `done` y resumen); una tarea `fix` por hallazgo con la ruta de `correccion-1.md` visible con `ezs next`; corrección a `dev`, si no al primer papel que no sea `plan`/`qa`; pausa sin papel; incidente → pausa sin preguntas del enjambre; enjambre detenido → pausa; equipo sin modo enjambre → error y hueco libre.
- Ningún lock se retiene durante una llamada a Herdr, al enjambre, al disco ni a la recuperación: el bucle copia el `Arc` del enjambre, suelta el cerrojo de la app y luego llama; `mark_autonomous` suelta su cerrojo antes de escribir el archivo y de avisar a la recuperación.
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`, `remove_dir_all`, `unsafe`, `"cmd"`, `"/c"`, `-Command` ni `.unwrap()` fuera de pruebas. Ningún `.rs` pasa de 600 líneas (mayores: `team.rs` 548, `supervisor/tests.rs` 560, `service.rs` 494, `state.rs` 583).

## Fixtures consumidos
- `rust/crates/ez-herdr/tests/permission/claude_bash.txt`, `codex_exec.txt`, `codex_edit.txt`: 3 pantallas reales, sin cambios en esta tarea (las usan las pruebas del bucle). No hay fixtures del dominio autónomo: **0 casos**.

## Pruebas portadas (`parity/test-map.csv`)
- Ninguna fila: 10.18 es funcionalidad nueva aprobada por el usuario, sin Python de referencia en `src/`; `test-map.csv` no tiene entradas de autonomía. Las pruebas propias cubren los escenarios que pide el brief.

## Pendiente o riesgos
- **Resume tras un incidente**: si el incidente sigue activo cuando el usuario dice «sigue», el bucle vuelve a pausar en el primer tic (es lo correcto: el equipo sigue roto) hasta que se resuelva con la herramienta `swarm`/`resolve`. Queda documentado porque puede parecer que «sigue» no hace nada.
- **Cobertura inicial de miembros**: un miembro entra en `scope` en cuanto su terminal tiene dirección (≤ 3 s tras el arranque). Un diálogo de permiso en esa ventana espera a que el bucle lo cubra; no se aprueba nada fuera de `scope`.
- **Incidentes y verificación en el mismo tic**: si un incidente aparece justo cuando el tablero se vacía, el primer tic puede lanzar la verificación; el segundo pausa. Es el precio de no pausar por incidentes transitorios.
- **Tareas `failed`**: no cuentan como «pendientes ni en curso»; el tablero se considera vacío y se verifica. El verificador dirá que falta el criterio y la corrección vuelve como tarea `fix`. La tarea `failed` original se queda como está (no hay transición para reintentarla).
- **Confianza al abrir un miembro**: si `run_agent` de un miembro falla por el diálogo de confianza, el supervisor lo marca `gone` y lo avisa; el bucle no aprueba ese diálogo de lanzamiento (el camino de 10.9 solo está cableado al ejecutor único), así que el equipo terminará pausando por plazo. Es la misma limitación que tenía el modo enjambre y queda fuera de 10.18.
- **Papel sin `dev`**: la corrección va al primer papel que no sea `plan` ni `qa`, en el orden declarado; si ese papel no tiene miembros abiertos, el supervisor abrirá uno cuando haya tarea lista.
- **`autonomous.txt`**: archivo adicional en la carpeta del enjambre (`<base>/autonomous.txt`); el Python no lo conoce y no lo necesita. Si un enjambre marcado se detiene, su id se queda en el archivo (inofensivo: nunca se reutiliza).
- **El verificador del equipo** se lanza con el `space` y el `workdir` de la tarea, en una terminal nueva; su carpeta `verificacion-K` es la única que puede editar, así que un permiso de edición fuera de ella pausa (10.1).

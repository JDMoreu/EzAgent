# Informe ECO-2d — `ez-arbiter`: agentes principales, worktrees y delegación

## Resultado
- Brief/tarea: ECO-2d
- Estado: completado

## Cambios
- `rust/crates/ez-arbiter/src/types.rs`: `ElephantKey { space, elephant }` (`e` + 8 hex, `e00000000` para el agente por omisión), `MainAgent`, `Caller::Elephant(ElephantKey)`, `FishKey` con `elephant` y `parent`, y `Command::AgentDelegate`.
- `rust/crates/ez-arbiter/src/arbiter/agents.rs` (nuevo): `agentes.json` con carga/guardado atómico; creación y borrado de worktrees (`git worktree add -b ez/<id>` / `git worktree remove`, sin `--force`), rama base, worktree limpio, integración `git merge --no-ff` con `git merge --abort`; `create_main_agent`, `remove_main_agent`, `main_agents`. Pruebas unitarias de integración correcta y de conflicto sin cambios.
- `rust/crates/ez-arbiter/src/arbiter/setup.rs` (nuevo): claves de cola `space|elephant`, normalización de las APIs antiguas por espacio, restauración de agentes/tareas al arrancar y ejecutor secuencial **por agente principal** (`ez-arbiter-<scope>`).
- `rust/crates/ez-arbiter/src/arbiter/mod.rs`: estado con `main_agents` y colas de peces, enrutado de `AgentDelegate`, tarjetas `Integrate` (merge/keep), colas por agente y por pez, `wait_for_main`, `wait_for_fish`, `snapshot_main`, y huellas/attribution sobre el worktree.
- `rust/crates/ez-arbiter/src/arbiter/api.rs`: `create_main_agent`, `remove_main_agent`, `main_agents`, `wait_for_main`, `wait_for_fish`, `snapshot_main`, `approve_task_for` y `approve_team_for`; `enqueue_any` con tipo borrado para resultados que no son `Outcome`.
- `rust/crates/ez-arbiter/src/arbiter/task_cmds.rs`: la tarea se crea en el worktree del agente y guarda su `ElephantKey`; una tarea activa por agente; tarjeta `Integrate` al cerrar con `done`; `integrate_impl`; un pez con hijos pendientes no puede informar.
- `rust/crates/ez-arbiter/src/arbiter/fish.rs` y `fish/order.rs`, `fish/work.rs`: `FishKey` con dueño y padre, etiquetas hc `ez_elephant`/`ez_parent`, `InstructionContext.parent`, herencia de etapa, aviso al padre cuando un hijo informa y límite de puestos simultáneos.
- `rust/crates/ez-arbiter/src/arbiter/delegate.rs` (nuevo): validación de `can_delegate`, `delega_en`, `cantidad` y `profundidad_maxima`; el hijo hereda la etapa y pertenece a la ronda del padre.
- `rust/crates/ez-arbiter/src/arbiter/lifecycle.rs`, `emit.rs`: eventos con `elephant`, tarjetas por clave de agente y actualización del cuerpo de la tarjeta de integración al fallar.
- `rust/crates/ez-arbiter/src/events.rs`: `CardKind::Integrate`, `EventKind::AgentDelegated` e `Integrated`, y campo `elephant` en `ArbiterEvent`/`ElephantEvent`.
- `rust/crates/ez-arbiter/src/criteria.rs`, `texts.rs`: `[validacion].profundidad_maxima` (1–5, por omisión 3) en la tarjeta y nuevos textos de delegación e integración, en español neutro.
- `rust/crates/ez-arbiter/tests/arbiter/{main_agents,delegation}.rs` (nuevos): dos agentes aislados, borrado con/sin tarea, recarga de `agentes.json`, integración correcta y `keep`, delegación rechazada y permitida, topes, padre antes que hijos y `wait` del padre; `support.rs` gana `reopen`, `elephant_key` y `git_output`.

## Decisiones
- `// DECISION` en `types.rs`: id `e00000000` fijo para el agente por omisión, para que tareas antiguas y pruebas resuelvan la clave sin leer `agentes.json`.
- `// DECISION` en `setup.rs`: las APIs antiguas siguen tomando el id del espacio y se resuelven al agente por omisión; las nuevas toman `ElephantKey`. Así no se rompe a los llamadores actuales de este crate.
- `// DECISION` en `setup.rs`: un espacio sin `agentes.json` recibe un agente por omisión en la carpeta original, sin `git worktree add`, para conservar las tareas creadas antes de los worktrees.
- `// DECISION` en `delegate.rs`: el id del hijo lleva el id del padre (`<padre>-<rol corto>`) para no chocar con el pez del mismo papel lanzado por el agente principal en la misma ronda.
- Los informes de los hijos se entregan al padre por una cola propia por pez, sin bloquear la cola del agente principal; `wait` nunca retiene el candado.

## Verificación
- `cargo fmt -p ez-arbiter --check`: exit 0.
- `cargo clippy -p ez-arbiter --all-targets -- -D warnings`: `Finished dev profile`, exit 0.
- `cargo test -p ez-arbiter`: 16 pruebas unitarias + 72 de integración + 1 doctest; 0 fallos, 0 ignoradas (`test result: ok` en las tres suites).
- Ningún `.rs` del crate supera 600 líneas (máximo 597) ni función nueva supera 60; sin `todo!`, `unimplemented!`, `#[ignore]`, `println!`/`eprintln!`, `unsafe` ni `.unwrap()` en código nuevo. `Cargo.lock` no cambió.

## Desviaciones del brief o del diseño
- Ninguna de alcance: solo se tocaron archivos de `rust/crates/ez-arbiter/**` y el informe. Se añadió `profundidad_maxima` a `[validacion]` porque §15.1 lo exige y el brief lo nombra en el cambio 4.
- El protocolo `agent.delegate` ya existe en `ez-ctl` desde ECO-R1; el arbiter expone `Command::AgentDelegate`. Unir ambos (y las acciones de la tarjeta `Integrate` con la vista) queda en la integración de la ola 2, fuera de este alcance.

## Pendiente o riesgos
- `Caller::Elephant` pasó a variante tupla y `FishKey` ganó campos: `ez-ctl` y `ez-hub` (fuera de alcance) necesitan actualizarse para compilar el workspace completo. Este crate compila y prueba por separado, como pide el brief.
- La tarjeta `Integrate` se abre y se atiende por `card_action` (`merge`/`keep`); el botón en la vista y el botón «Integrar en {rama base}» son trabajo de ECO-6c.
- No se verifican las mutaciones en código (§13); el árbitro sigue exigiendo los números al `qa_validator`.
- La integración con conflictos se prueba a nivel de la función `merge` con git real; la ruta completa de tarjeta fallida queda cubierta por el mensaje y el cuerpo de la tarjeta, pero no por una prueba de conflicto punta a punta.

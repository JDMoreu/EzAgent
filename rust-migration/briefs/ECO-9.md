# ECO-9 — Correcciones que encontró la prueba de punta a punta

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `docs/designs/espacio-elefante.md` §15, el
brief `ECO-8` y su informe `rust-migration/reports/ECO-8-r1.md` (sección «Pendiente o
riesgos»).** La prueba `cargo run -p ez-hub --example e2e_fake` pasa hoy solo gracias a
tres atajos del banco de pruebas; hay que corregir el producto y quitar los atajos.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-hub/src/runtime/**`, `rust/crates/ez-arbiter/src/**` (y sus pruebas) y
  `rust/crates/ez-hub/examples/e2e_fake*`.

## Correcciones
1. **`ez wait` de un agente principal** (`ez-hub/src/runtime/handler.rs`, `ArbiterApi::wait`):
   hoy llama a `wait_for_elephant(space)` y no recibe los eventos de un agente principal con
   id aleatorio. Debe usar `wait_for_main(&ElephantKey)` para agentes principales y
   `wait_for_fish(&FishKey)` para subagentes. Quita del árbitro `wait_for_elephant(space)`
   si ya nadie lo usa.
2. **Ronda solo de verificación** (`ez-arbiter/src/arbiter/fish/order.rs`): en rondas
   posteriores a la 1, los papeles `first_round_only` no se esperan (ni verificadores ni
   ejecutores). Prueba: con `plan_reviewer` en su etapa sugerida (2), la ronda 2 solo de
   verificación (qa + qa_validator) arranca y puede ser limpia.
3. **La integración debe llevar el trabajo.** Los subagentes no hacen commits, así que
   hoy `git merge ez/<id>` no incluye sus cambios. Al cerrar la tarea con éxito (`done`),
   **antes** de abrir la tarjeta de integración, el árbitro confirma los cambios del
   worktree en la rama `ez/<id>`: `git -C <worktree> add -A -- . ":(exclude).ez"` y
   `git -C <worktree> commit -m "EzAgent: «{título}» (ronda {n}, dos rondas limpias)"`
   (sin cambios → no hay commit y la tarjeta lo dice). Además, al crear un agente
   principal, añade `.ez/` a `info/exclude` del repositorio (el común a los worktrees) si
   no está, para que los artefactos nunca entren en un commit. Prueba con git real: los
   archivos de un ejecutor llegan a la rama base tras `merge`, y nada de `.ez/` entra.
4. **Banco de pruebas sin atajos:** el ejemplo usa un agente principal con id aleatorio
   (`main_agent_create` normal), `plan_reviewer` en su etapa sugerida y **no** hace commits
   por su cuenta. Debe seguir saliendo con 0.

## Criterios de aceptación
fmt, clippy y test de `ez-arbiter` y `ez-hub` en verde, y **ejecuta**
`cargo run -p ez-hub --example e2e_fake` con código 0 (pega la salida). Informe en
`rust-migration/reports/ECO-9-r1.md`.

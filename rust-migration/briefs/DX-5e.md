# DX-5e — Aislar elecciones por ciclo de pregunta

Continúa en `work/DX-5`. Lee `AGENTS.md`, `_COMMON-ECO.md`, `DX-5.md`, `DX-5d.md` y el informe `DX-5d-r1.md`. El objetivo y las prohibiciones de DX-5 siguen vigentes.

## Archivos que se tocan

Los archivos permitidos por DX-5 y DX-5b, y además `rust/crates/ez-agent/src/runtime.rs`, `rust/crates/ez-agent/src/runtime/state.rs` y sus pruebas. No amplíes más el alcance sin informar.

## Trabajo

Implementa la identidad de turnos descrita en el informe si verificas que `begin_turn` corresponde realmente a cada entrada de la persona y que está disponible para las herramientas. Mantén el estado pendiente solo para la pregunta activa y su respuesta; no autorices una elección antigua solo porque llegó el siguiente turno o porque el LLM repite una tarea. Una respuesta que no elija los datos pendientes debe cerrar o invalidar la autorización anterior. Conserva los reintentos legítimos del mismo turno y las respuestas cortas explícitas.

Prueba con el puerto falso y, donde sea necesario, el runtime real: misma tarea tras abandono, tareas con prefijos iguales, un turno siguiente que cambia de pedido sin contestar, respuesta legítima, y reintento en el mismo turno. No lances en un pedido nuevo sin autorización propia. Si el ciclo real no permite distinguir algún caso de forma fiable, falla cerrado e informa la limitación; no inventes una señal del LLM.

Sin ventanas, agentes reales, credenciales ni cambios de foco. Ejecuta fmt, clippy y pruebas de los crates afectados; deja commit e informe `rust-migration/reports/DX-5e-r1.md`.

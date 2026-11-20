# DX-5c — No reutilizar elecciones de una pregunta abandonada

Trabaja sobre `work/DX-5`. Lee `AGENTS.md`, `_COMMON-ECO.md`, `DX-5.md` y `DX-5b.md`. Conserva sus límites y prohibiciones: sin ventanas, agentes reales, credenciales ni cambios de foco.

## Corrección

`PendingPlace` conserva espacio y agentes aceptados sin vincularlos al pedido. Como ya señala `DX-5-r1.md`, si la persona abandona la pregunta, `guard_run` puede aceptar esos valores en otra delegación aunque la persona no los haya dicho en ese nuevo pedido. Esto incumple la regla de DX-5 de pedir los datos de este pedido.

Vincula las elecciones pendientes al pedido y descártalas cuando ese pedido se abandone o se sustituya. Mantén las respuestas cortas a la pregunta activa y la conservación de los datos ya elegidos dentro del mismo pedido. No dependas de que el LLM identifique correctamente el abandono. Añade pruebas con el puerto falso que reproduzcan la secuencia: pregunta parcialmente contestada, cambio a otro pedido, intento de delegación con valores anteriores, y ninguna ejecución hasta que la persona los autorice para el nuevo pedido. Cubre espacio y agente y verifica que la continuación legítima sigue funcionando.

Ejecuta fmt, clippy y las pruebas de los crates afectados, sin ventanas. Deja commit e informe `rust-migration/reports/DX-5c-r1.md` con resultados y riesgos. Si la corrección exige ampliar el alcance, detente y explícalo.

# DX-6b — Preparar humo visual y comprobar tareas retenidas

Trabaja sobre `work/DX-6`. Lee `AGENTS.md`, `_COMMON-ECO.md`, `DX-6.md` y `DX-4b.md`. Mantén los límites de DX-6. No abras ventanas ni toques la app o los procesos de la persona; no ejecutes agentes reales ni leas credenciales.

1. El humo entregado usa dobles sin ventana. DX-6 pide además dejar preparado un modo de humo de la app real con ventanas que muestre una tarea terminada, el indicador persistente y la vista rápida. Prepara ese modo sin ejecutarlo; debe permitir al agente principal ejecutarlo solo en la pantalla secundaria después de integrar DX-7, con datos temporales y sin agentes reales. No cambies la app de la persona.
2. Comprueba la coherencia con DX-4: allí las tareas retenidas no entran en `watched` antes del «sí»; la foto de DX-6 parte de `watched` y del registro `voz`. Asegura con una prueba que una tarea guardada que no esté en el registro `voz` aparezca en espera al reabrir, sin lanzar ni reanudar nada. No dependas de que se acepte la reanudación para mostrarla. Si hace falta sincronizar la rama con DX-4 o ampliar el alcance, detente e informa qué necesitas.

Ejecuta fmt, clippy y pruebas de los crates afectados sin ventanas. Deja commit e informe `rust-migration/reports/DX-6b-r1.md`.

# DX-4b — Cerrar fallos de seguridad en la retención al arrancar

Trabaja en `work/DX-4`, sobre la entrega DX-4. Lee `AGENTS.md`, `_COMMON-ECO.md` y `DX-4.md`. Mantén sus límites de archivos y la prohibición de abrir ventanas o tocar la app de la persona.

## Correcciones

1. En `watch/resume.rs`, `hold_saved_swarms` registra un error de `store().live()` o `service.pause()` y continúa. Después `start_pending_resume` arranca el supervisor: un enjambre activo podría actuar sin autorización. Haz que el arranque falle cerrado: si no se puede obtener el inventario o asegurar la pausa de todos los enjambres activos, no arranques el supervisor ni reanudes nada. Informa el fallo sin perder lo pendiente. Prueba ambos fallos con dobles y demuestra que no hay lanzamiento ni envío.
2. `store().live()` incluye enjambres `active` y `paused`, pero el «sí» reanuda todos. Conserva la pausa previa de los que ya estaban pausados por la persona; no los actives por aceptar la recuperación de los que estaban activos. Agrega una prueba con ambos estados.

Mantén las demás conductas pedidas por DX-4. Ejecuta fmt, clippy y pruebas de `ez-app`, sin ventanas ni agentes reales. Deja commit y un informe `rust-migration/reports/DX-4b-r1.md` con resultados y riesgos. Si alguna corrección requiere ampliar el alcance, detente y explícalo.

# DX-2c — Corrección de la revisión de DX-2b

Revisión del agente principal sobre `work/DX-2` (f8a0988..45d96a9): la implementación de
los acuses está bien (A1, A2, A3, regla de voz, `ack_fast`). Un defecto bloquea:

## R2c-1. Las pruebas escriben en los registros reales de la persona
Tras correr las pruebas, `%APPDATA%\EzAgent\logs\latency.jsonl` de la persona tiene 9
líneas `{"kind": "command", "name": "ack_fast", …}` (la app que usa no puede generarlas).
Las pruebas de `ez-app` deben escribir latencia y auditoría en una carpeta temporal:
- Busca cómo `record_command`/`record_turn` y la auditoría eligen la ruta
  (`ez-core/src/dirs.rs` `logs_dir()`, `latency_path()`, `audit_path()`) y haz que las
  pruebas usen una carpeta propia (p. ej. una variable `EZAGENT_LOGS_DIR` que `logs_dir()`
  respete y que el soporte de pruebas de `ez-app` fije a un temporal por prueba, o la
  inyección que ya exista). En producción nada cambia.
- Prueba que lo fija: después de ejercitar el camino rápido, el `latency.jsonl` real no
  cambió (compara tamaño y fecha antes/después usando la ruta que devolvería sin la
  variable) y el temporal sí tiene la línea.
- Revisa si otras pruebas de `ez-app` (turnos, comandos) también escribían ahí y
  cúbrelas con el mismo cambio. **No** borres ni edites el `latency.jsonl` real.

## Alcance
`rust/crates/ez-core/src/dirs.rs` (solo la variable), el soporte de pruebas de `ez-app`
(`app/states/tests/support.rs` u otro equivalente) y las pruebas que haga falta.

## Entrega
fmt, clippy y test de `ez-core`, `ez-app` y `ez-agent` en verde. Informe en
`rust-migration/reports/DX-2c-r1.md`.

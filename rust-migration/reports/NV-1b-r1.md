## Resultado
- Brief/tarea: NV-1b (corrección de la revisión de NV-1, ola HD)
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/subagents/watch/novelties.rs`: el límite de uso solo se evalúa con `agent_state != "working"` **y** la pantalla sin cambios desde la vuelta anterior; la búsqueda se hace en las últimas 15 líneas (`LIMIT_TAIL_LINES` + `last_lines`). Patrones de fábrica = las seis frases completas de §3 (`Free limit reached`, `5 hour usage limit reached`, `You've hit your usage limit`, `Usage limit reached`, `rate limit reached`, `quota exceeded`); se quitaron `quota`, `rate limit`, `usage limit` y `resets at` como disparadores. El mismo recorte se aplica a la hora de reinicio y a la decisión de reanudar un `bloqueado`.
- `rust/crates/ez-herdr/src/subagents/watch/novelties/tests.rs`: pruebas unitarias de patrones sueltos que **no** cuentan, frases completas que sí, ventana de 15 líneas y `last_lines`.
- `rust/crates/ez-herdr/src/subagents/watch/tests/novelties.rs`: pruebas con `hc` y reloj falsos: el encargo de IR-2 y una pantalla con «quota»/«rate limit»/«usage limit» mientras `working` no bloquean ni escriben `uso.json`; la misma frase de límite en la parte alta de una pantalla larga no bloquea; «Free limit reached … Resets at 7:00 p.m.» al final con `idle` y pantalla estable bloquea una vuelta después, con la hora 19:00 en `uso.json`. Se ajustó la prueba de «un aviso por episodio» para exigir la vuelta estable.

## Decisiones
- La primera lectura de una pantalla cuenta como estable (no hay vuelta anterior con la que comparar): así sigue bloqueando un hijo que ya nació mostrando el límite, como en la prueba original de NV-1. Se anotó en el código.
- El recorte a las últimas 15 líneas vive en `Novelties::limit_hit`, no en el vigilante, para que la reanudación de un `bloqueado` use exactamente el mismo criterio que el bloqueo.
- Los patrones de `novedades.toml` conservan la semántica de NV-1: si el archivo trae reglas, sustituyen a los de fábrica (la corrección solo cambia los valores de fábrica y la ventana de búsqueda).
- Sin `// DECISION` nuevos: no hubo que elegir fuera de lo que fija el diseño.

## Verificación
Desde `rust/`, con `ez-herdr`:
- `cargo fmt -p ez-herdr -- --check`: ok (sin salida).
- `cargo clippy -p ez-herdr --all-targets -- -D warnings`:
  - `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.59s` (sin avisos).
- `cargo test -p ez-herdr`:
  - `running 300 tests` → `test result: ok. 300 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.22s`
  - `test result: ok. 1 passed; 0 failed` (agent_driver_parity)
  - `test result: ok. 1 passed; 0 failed` (herdr_parity)
  - `test result: ok. 2 passed; 0 failed` (metrics_parity)
  - `test result: ok. 27 passed; 0 failed` (permission)
  - `test result: ok. 1 passed; 0 failed` (tasks_activity_parity)
  - doc-tests: `test result: ok. 0 passed; 0 failed`
- Métricas: `novelties.rs` 543 → 560 líneas; `watch/tests/novelties.rs` 373 → 510; `novelties/tests.rs` 159 → 204. Ningún `.rs` pasa de 600 líneas.

## Pendiente o riesgos
- Las pruebas usan `hc` falso y carpetas temporales; no se ejecutó ningún agente ni se tocó la ventana de Herdr (NV-1b no lo pide).
- Un agente cuyo estado de `hc` no sea exactamente `"working"` (por ejemplo, estado vacío o desconocido) sigue sujeto a la detección; es lo que pide §3 («solo si no está `working`»).
- Si la frase de límite queda a más de 15 líneas del final (salida muy habladora después del aviso), no se detecta en esa vuelta; es el compromiso de §3 para no marcar a quien escribe sobre límites.

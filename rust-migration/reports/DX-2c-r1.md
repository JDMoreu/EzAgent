# DX-2c r1 — Corrección de la revisión: las pruebas no escriben en los registros reales

Rama `work/DX-2`. Corrección del defecto R2c-1 de la revisión de DX-2b: las pruebas de
`ez-app` escribían `{"kind":"command","name":"ack_fast",…}` en el `latency.jsonl` real de la
persona. No se borró ni editó ese archivo; no se tocó la app de voz abierta, ni Herdr, ni se
lanzaron agentes reales.

## Resultado
- Brief/tarea: DX-2c
- Estado: completado

## Cambios
- `rust/crates/ez-core/src/dirs.rs`: `logs_dir()` respeta `EZAGENT_LOGS_DIR`; sin la variable
  devuelve `%APPDATA%\EzAgent\logs` como antes, así que en producción nada cambia. `latency_path()`
  y `audit_path()` quedan redirigidos por la misma variable.
- `rust/crates/ez-app/src/app/core/init.rs`: **archivo fuera de la lista del brief** (una línea).
  `with_services` calculaba la ruta de latencia a mano desde `config_path().parent()`, sin pasar por
  `ez_core::dirs::latency_path()`; sin este cambio la variable no redirigía lo que la app escribe.
  En producción la ruta es idéntica a la anterior.
- `rust/crates/ez-app/src/app/states/tests/support.rs`: helper `redirect_logs()` que fija
  `EZAGENT_LOGS_DIR` a un temporal del proceso con `Once`, y lo llama `Fixture::with_config` antes
  de construir la app.
- `rust/crates/ez-app/src/app/events/support.rs`: **otro soporte de pruebas**, mismo helper en
  `Fixture::new` (es el fixture que usan las pruebas de turnos y comandos de `events/tests.rs`).
- `rust/crates/ez-app/src/app/events/tests.rs`: `ack_fast_records()` pasa a leer el registro
  temporal; nueva prueba `the_ack_fast_path_writes_to_the_test_logs_not_the_real_one`, que compara
  tamaño y fecha de la ruta real (calculada sin la variable) antes/después y exige la línea
  `ack_fast` en el temporal.

## Decisiones
- `// DECISION: las pruebas fijan esta variable a un temporal para no escribir en los registros
  reales de la persona; en producción nunca se define.` (`dirs.rs`).
- `# DECISION: un único temporal por proceso con Once, no uno por prueba: la variable es global y
  un guardia por prueba exigiría un cerrojo retenido durante las esperas de TTS, que el encargo
  prohíbe. Cada proceso de test tiene un PID distinto, así que no hay colisiones entre binarios.`
- `# DECISION: se redirigen los dos fixtures que construyen apps en events/tests.rs; los demás
  módulos no emiten AppEvent::Transcribed y por eso nunca escribían latencia.`
- `init.rs` se tocó por necesidad (una línea, sin cambio de comportamiento en producción); se
  documenta igual que los archivos extra de DX-2b.

## Verificación
Ejecutado desde `rust/`, solo con `-p ez-core -p ez-app -p ez-agent`:

- `cargo fmt -p ez-core -p ez-app -p ez-agent -- --check`: correcto (sin diffs).
- `cargo clippy -p ez-core -p ez-app -p ez-agent --all-targets -- -D warnings`: correcto
  (`Finished`, código 0). Únicos avisos: los 14 conocidos de `vendor/freetype-sys`.
- `cargo test -p ez-core -p ez-app -p ez-agent -- --test-threads=4`: **todos los binarios en
  verde, 0 fallos**. Incluye `ez-core` 62, `ez-app --lib` 359 (con la prueba nueva) y `ez-agent`
  286, más todos los binarios de paridad. `--test-threads=4` se usó para reducir la contención;
  con el paralelismo por defecto, bajo la carga de otros subagentes, fallaron de forma intermitente
  pruebas de temporización distintas de `app::states::tests`
  (`announcement_waits_while_the_user_speaks_or_thinks`,
  `announcement_reopens_the_call_and_speaks`, `speak_advances_the_turn_and_announces_done`): cada
  una pasa aislada y las 17 de `app::states::tests` pasan juntas con `--test-threads=1` (21.9 s).
  No se tocó ninguna de esas pruebas.
- Registro real `%APPDATA%\EzAgent\logs\latency.jsonl` **sin cambios** antes/después de todas las
  ejecuciones: tamaño 132363 bytes, fecha `2026-11-19T16:46:08.7944246Z`, 469 líneas y 9 líneas
  `ack_fast` (las previas a esta corrección). No se borró ni editó.
- Temporales `ezagent-dx2c-logs-<pid>` con 4 líneas y 3 `ack_fast` cada uno: la app escribe ahí.

## Pendiente o riesgos
- `EZAGENT_LOGS_DIR` es global por proceso de test; si en el futuro otra prueba necesita una ruta
  distinta, habría que sustituir el `Once` por inyección de ruta en `AppParts`.
- Los directorios temporales se acumulan en `%TEMP%` (uno por PID); los limpia el sistema.
- `init.rs` queda fuera del alcance literal del brief; el cambio es de una línea y no altera la
  ruta en producción, pero conviene registrarlo como deuda de alcance para el siguiente revisor.

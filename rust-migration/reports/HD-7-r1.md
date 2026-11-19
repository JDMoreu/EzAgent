# HD-7 — Registro del estado de cada subagente y textos de aviso

## Resultado
- Brief/tarea: HD-7
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/lib.rs`: único cambio permitido, la línea `pub mod subagents;`.
- `rust/crates/ez-herdr/src/subagents.rs`: tipos del contrato (`TaskState`, `Change`, `Validation`, `Record`, `RegistryError`) y lógica pura `pending_notices`, `notice_text` (tres formatos de §13), `one_line`, validación de nombre/padre y reglas de transición.
- `rust/crates/ez-herdr/src/subagents/registry.rs`: `Registry` con `from_env`/`at`/`root`/`create`/`get`/`list`/`find_by_terminal`/`transition`/`mark_notified`, escritura atómica (temporal + renombrar) y candado `<nombre>.lock` con reintento de 50 ms hasta 3 s, limpieza al caer y reemplazo de candados de más de 30 s.
- `rust/crates/ez-herdr/src/subagents/tests.rs`: ayudantes de carpeta temporal única y pruebas de estados, serialización con los nombres del contrato, avisos y `one_line`.
- `rust/crates/ez-herdr/src/subagents/tests/registry.rs`: pruebas de creación duplicada, transiciones permitidas/prohibidas, persistencia de `history`, dos padres (`t6h` y `maquina/t7`), dirección y nombre inválidos, escritura atómica leída por otro hilo, dos transiciones concurrentes, candado abandonado, candado fresco (`Busy`) y `mark_notified`.
- `rust-migration/reports/HD-7-r1.md`: este informe.

## Decisiones
- `subagents.rs` (`notice_text`, terminado): sin informe se dice «sin informe» para no dejar «Informe: » vacío.
- `subagents.rs` (`notice_text`, validado): un registro validado sin objeto `validation` es dato incompleto; se avisa igual con resumen vacío para no callar el cambio.
- `subagents.rs` (`notice_text`, en proceso): §13 no define aviso para `en_proceso`; se usa un texto corto propio para que el vigilante pueda avisar del alta sin inventar otro formato.
- `registry.rs` (`from_env`): fuera de Windows `LOCALAPPDATA` no existe, se usa `HOME` y como último recurso la carpeta temporal para no dejar la raíz vacía.
- `registry.rs` (`acquire_lock`): se crea la carpeta del padre antes de abrir el candado. Era necesario porque el primer `create` intentaba crear `<padre>/<nombre>.lock` antes de que existiera `<padre>` (fallo `os error 3` en Windows). Es la corrección de raíz, en la única función que adquiere el candado, así que cubre `create`, `transition` y `mark_notified`.
- El trabajo llegó a medias de una sesión anterior: `subagents.rs`, `subagents/registry.rs` y la línea de `lib.rs` ya estaban en el árbol sin commitear, pero `mod tests;` apuntaba a un archivo inexistente. Se completaron las pruebas y se corrigió el fallo de la carpeta; no se tocó nada fuera del alcance del brief.

## Verificación
- `cargo fmt -p ez-herdr`: sin salida, código 0.
- `cargo fmt -p ez-herdr -- --check`: sin salida, código 0.
- `cargo clippy -p ez-herdr --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 2.02s`.
- `cargo test -p ez-herdr` (desde `rust/`):
  - `test result: ok. 201 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.44s` (unitarios; 19 nuevos de HD-7, 182 preexistentes).
  - `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.53s` (`agent_driver_parity`).
  - `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s` (`herdr_parity`).
  - `test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s` (`metrics_parity`).
  - `test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s` (`permission`).
  - `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s` (`tasks_activity_parity`).
  - `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` (doc-tests).
- Límites: `subagents.rs` 290, `registry.rs` 298, `tests.rs` 234, `tests/registry.rs` 446 líneas (todas < 600); ninguna función supera 60 líneas.
- `Cargo.toml` y `Cargo.lock` sin cambios: no se añadieron dependencias.

## Pendiente o riesgos
- HD-8 (vigilante, `ez report`/`ez validate`/`ez tasks`/`ez watch`) debe usar este módulo; el aviso de `en_proceso` es una extensión local porque §13 no lo define.
- Las pruebas de concurrencia usan hilos reales y una ventana de 3 s para `Busy`; son deterministas en las ejecuciones locales, pero la prueba `fresh_lock_reports_busy` tarda ~3 s por el reintento del candado.
- No se lanzaron agentes reales, no se habló con `hc`/Herdr ni con la red, y no se leyó ni escribió la configuración real del usuario.

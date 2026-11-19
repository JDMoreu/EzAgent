# Informe ECO-4c — `ez-team`: papeles abiertos (biblioteca y papeles de la tarea)

## Resultado
- Brief/tarea: ECO-4c
- Estado: completado

## Cambios
- `rust/crates/ez-team/src/role_text.rs`: `library_dir()` (`%APPDATA%\EzAgent\hub\papeles` vía `ez_core::dirs::app_data_dir()`), `load_roles(task_dir, library_dir)` que fusiona incluidos, biblioteca y tarea con precedencia tarea > biblioteca > incluido, ignora archivos `_*.md`, trata la carpeta ausente como sin papeles y devuelve error que nombra el archivo inválido; `roles_text` y `roles_json` con el formato de `ez roles`. Pruebas de las tres fuentes y precedencia, error nombrado, `_algo.md` ignorado, carpetas ausentes, ruta de la biblioteca y texto/JSON exactos.
- `rust/crates/ez-team/src/team.rs`: `validate` comprueba también papel disponible, `cantidad`/`etapa` ≥ 1 y `delega_en` (solo papeles que pueden delegar, solo papeles del equipo) con cualquier lista de `RoleDef`; `card` añade al final `Papeles nuevos de esta tarea:` con los papeles de origen `Task` que usa el equipo.
- `rust/crates/ez-team/src/team/tests.rs`: pruebas de cada regla nueva de validación, del aviso de familia para el validador (mutación pendiente de ECO-4b) y para un papel verificador abierto, y de la tarjeta con papeles nuevos de la tarea.
- `rust/crates/ez-team/src/lib.rs`: reexporta `library_dir`, `load_roles`, `roles_text` y `roles_json`.

## Decisiones
- `// DECISION` (role_text.rs): los archivos de cada carpeta se ordenan por nombre antes de leerlos para que el resultado y el primer error sean deterministas (`read_dir` no garantiza orden).
- `// DECISION` (role_text.rs): `ez roles` muestra primero los incluidos, luego los de la biblioteca y al final los de la tarea (`ez --skill` y el propio formato del brief listan los orígenes en ese orden); es el orden de lectura, no el de prioridad.
- `// DECISION` (role_text.rs): las claves JSON van en español (`id`, `nombre`, `tipo`, `delega`, `escribe_pruebas`, `mutaciones`, `origen`, `descripcion`), como en `Inventory::json`; el protocolo no las fija.
- `// DECISION` (team.rs): la tarjeta lista solo los papeles de origen `Task` que usa el equipo; los papeles de la carpeta de la tarea que nadie usa no necesitan aprobación.
- Las comprobaciones de `cantidad`, `etapa` y `delega_en` se añaden también a `validate` (además de `parse`) para que la validación sea correcta con cualquier `Seat`, no solo con los producidos por `parse`.

## Verificación
- `cargo fmt -p ez-team -- --check`: exit 0, sin salida.
- `cargo clippy -p ez-team --all-targets -- -D warnings`: ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.46s``, exit 0, sin avisos.
- `cargo test -p ez-team`: `test result: ok. 73 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s`; doc-tests `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.25s` (antes del cambio: 59 + 1).
- Mutación pendiente de ECO-4b verificada: al excluir al validador del aviso de familia (`if !def.is_verifier() || seat.role == qa_validator { continue; }`) falla `team::tests::validator_sharing_a_family_with_an_executor_is_warned`; se restauró el archivo y la prueba vuelve a pasar.
- Límites: ningún `.rs` supera 600 líneas (máximo 580 en `team/tests.rs`); sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`/`eprintln!`, `unsafe` ni `.unwrap()` fuera de pruebas en código nuevo; ningún lock retenido durante E/S, subproceso o espera.
- Alcance: solo `rust/crates/ez-team/src/**`; contratos (`roles.rs`, `contract.rs`, `roles/*.md`) intactos; `Cargo.lock` sin cambios.

## Pendiente o riesgos
- `ez roles` en `ez-ctl` y en el runtime de `ez-hub` sigue mostrando solo los cinco papeles incluidos: conectar `load_roles`/`roles_text`/`roles_json` y los papeles de biblioteca/tarea es trabajo de ECO-6c, fuera de este alcance.
- El orden de origen en `roles_text` y las claves de `roles_json` son decisiones documentadas: el brief fija el formato de texto pero no el criterio de orden ni las claves JSON.

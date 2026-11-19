## Resultado
- Brief/tarea: HD-3 — `ez-herdr`: validar tipo, modelo y esfuerzo contra el catálogo
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/driver/choice.rs` (nuevo, ~290 líneas): `Choice` (tipo, modelo y esfuerzo validados) y `AgentDriver::validate_choice`, que comprueba tipo con los alias del catálogo, programa instalado (`which` del primer elemento del comando), modelo (`models` con prefijo/`proveedor/nombre`, `discover=mcode`, `discover=codex`, sin lista → avisos) y esfuerzo (`efforts`, `model_efforts`, niveles de Codex, perfiles sin forma de fijarlo) y devuelve avisos sin escribir nada. Helpers locales: `label_of`, `model_matches`, `mcode_matches`, `union_levels`, `unverified`, `has_effort_support`.
- `rust/crates/ez-herdr/src/driver/choice/tests.rs` (nuevo, 312 líneas, 10 pruebas): los seis tipos del catálogo con casos válidos, prefijo de OpenCode, modelos y esfuerzos inválidos, Qwen sin esfuerzo, alias (`open code`/`cloud code`), tipo desconocido, programa no instalado, catálogos sin descubrir (avisos) y comprobación de que ningún gancho de escritura se llamó.
- `rust/crates/ez-herdr/src/driver.rs`: `mod choice;`, `pub use choice::Choice;` y `pub(crate)` en `agents`, `discover`, `has_ignore_case` e `is_prelaunch` para reutilizarlos desde el submódulo. Sin otros cambios.
- `rust/crates/ez-herdr/src/herdr/process.rs`: **no tocado**. `which` ya acepta rutas absolutas a ejecutables existentes (lo cubre la prueba existente `herdr::tests::subprocess::which_encuentra_rutas_explicitas`), que es la condición que el brief ponía para tocarlo.
- `rust-migration/reports/HD-3-r1.md`: este informe.

## Decisiones
- **`// DECISION` en `choice.rs`**: la coincidencia de mcode y la resolución de Codex se reimplementan como helpers de solo lectura dentro de `choice.rs`. `match_mcode_model`/`resolve_codex` viven en `driver/launch.rs`, que queda fuera del alcance del brief, y las rutas de lanzamiento (`fill_args`) llaman a `set_mcode_effort`/`prepare_opencode_state`, prohibidas por la regla 4. Se conservan los mensajes exactos de esas funciones.
- **`pub(crate)` en cuatro ayudantes de `driver.rs`**: es la única forma permitida por el brief de reutilizar `agents`, `discover`, `has_ignore_case` e `is_prelaunch` desde `driver/choice.rs` sin duplicarlos.
- **Modelo/esfuerzo en `Choice`**: se guarda el valor pedido ya recortado (`strip`), tal como dice el contrato, no la referencia canónica del catálogo (p. ej. `GPT-5` se conserva; `opencode-go/deepseek-v4.1-flash` también).
- **Orden de validación**: tipo → programa → `check_value` de modelo y esfuerzo → modelo (`models`/mcode/Codex/sin lista) → esfuerzo (`model_efforts`/Codex/sin niveles). Un rechazo devuelve `Err` antes de acumular avisos y no crea nada.
- Sin dependencias nuevas; `Cargo.toml` y `Cargo.lock` sin cambios. Sin agentes reales, sin red, sin tocar la configuración del usuario ni la ventana de Herdr.

## Verificación
- `cargo fmt -p ez-herdr -- --check`: limpio (`FMT_OK`); `cargo fmt -p ez-herdr` aplicado.
- `cargo clippy -p ez-herdr --all-targets -- -D warnings`: limpio (`Finished 'dev' profile [unoptimized + debuginfo] target(s) in 4.78s`).
- `cargo test -p ez-herdr -q`:
  - `test result: ok. 191 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.53s` (10 nuevas: `driver::choice::tests`).
  - Paridad: `agent_driver_parity` 1 passed, `herdr_parity` 1 passed, `metrics_parity` 2 passed, `permission` 27 passed, `tasks_activity_parity` 1 passed, doc-tests 0. Todas en verde; las pruebas existentes de `ez-herdr`, incluidas las de paridad, siguen pasando.
- Tamaños post-`fmt`: `driver/choice.rs` ~290, `driver/choice/tests.rs` 312; ambos por debajo de 600; ninguna función supera 60 líneas (clippy no señala `too_many_lines`).
- Prohibidos en lo añadido: sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`/`eprintln!`, `remove_dir_all`, `unsafe`, `"cmd"`, `"/c"`, `-Command`, `.unwrap()` fuera de pruebas ni locks retenidos durante E/S o subprocesos.

## Pendiente o riesgos
- La lógica de mcode/Codex en `choice.rs` es un espejo de validación de la de `launch.rs`: si cambian los mensajes o la coincidencia en el oráculo, hay que actualizar ambos. Unificarlos exigiría tocar `launch.rs`, fuera de este alcance.
- `validate_choice` comprueba el programa en el catálogo con el `which` inyectado; con el `which` real, una ruta absoluta a un ejecutable que exista se acepta (comportamiento ya probado en `process.rs`). No se ha integrado todavía en el comando `ez delegate` (otro brief).
- No se ejecutó `pytest`: no se tocó el Python.

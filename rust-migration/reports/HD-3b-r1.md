## Resultado
- Brief/tarea: HD-3b — correcciones de la revisión de HD-3 (R3-1, R3-2, R3-3)
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/driver/launch.rs`: `agent_command`, `match_mcode_model`, `resolve_codex` y `Wanted` (+ sus campos) pasan a `pub(crate)`. Solo visibilidad; sin cambios de comportamiento.
- `rust/crates/ez-herdr/src/driver.rs`: `Wanted` pasa a `pub(crate)` con campos `pub(crate)` para que `choice.rs` construya el `wanted` de Codex con la misma función que usa el lanzamiento. Sin otros cambios.
- `rust/crates/ez-herdr/src/driver/choice.rs`: R3-1 reutiliza `match_mcode_model` y `resolve_codex` de `launch.rs` (borra `mcode_matches`, `union_levels` y la copia de la lógica de Codex; `check_codex` pasa `model` y `effort` en un `Wanted` y usa `resolve_codex`), actualiza la `// DECISION`. R3-2 añade `check_effort_without_model`: con esfuerzo y sin modelo en un perfil con `model_efforts`, valida contra los niveles del modelo por omisión (`hooks.opencode_default_model`, solo lectura, comparando la parte tras la última `/`) o, si no se conoce, contra la unión de niveles; `model_effort_error` centraliza el mensaje. R3-3 `check_program` usa `agent_command(cfg, &key, &profile)`, la misma función de `build_launch`, en vez de rearmar el comando.
- `rust/crates/ez-herdr/src/driver/choice/tests.rs`: el ayudante `driver_with` gana el parámetro `default_model` y se añade `driver_configured` (cfg inyectable) y `driver_which` (`which` como closure). Nuevas pruebas: `esfuerzo_sin_modelo_usa_los_niveles_del_modelo_por_omision` (`ultra` rechazado por la unión, `high` aceptado, modelo por omisión `kimi-k3` con niveles propios) y `programa_se_busca_igual_que_al_lanzar` (`agents` del `voice.json` con ruta absoluta). Se corrigen los dos llamadores que quedaron con la firma antigua. 12 pruebas en total en este módulo.
- `rust-migration/reports/HD-3b-r1.md`: este informe.
- `rust/crates/ez-herdr/src/herdr/process.rs`: **no tocado**. `which` ya acepta rutas absolutas a ejecutables existentes (prueba existente `which_encuentra_rutas_explicitas`), cubierto además por la nueva prueba con la ruta `C:/tmp/fake-ws-agent.exe`.

## Decisiones
- **`// DECISION` de `choice.rs`** (actualizada): `match_mcode_model` y `resolve_codex` son puras (solo comparan con el catálogo ya leído), se reutilizan como `pub(crate)` para que validación y lanzamiento no diverjan; las rutas que escriben (`set_mcode_effort`, `prepare_opencode_state`) siguen sin llamarse desde la validación.
- **Esfuerzo sin modelo (R3-2)**: se compara solo la parte tras la última `/` del modelo por omisión, igual que hace `launch.rs` con `wanted.model`; si el modelo no está en `model_efforts` o el hook devuelve vacío, se usa la unión de todos los niveles y el mensaje «OpenCode no tiene el esfuerzo «X». Valores válidos: …». No se escribe nada.
- **Visibilidad mínima**: solo lo que pide el brief (`agent_command`, `match_mcode_model`, `resolve_codex`, `Wanted`) se expone a `pub(crate)`; el resto del módulo sigue privado.
- `Cargo.toml` y `Cargo.lock` sin cambios. Sin agentes reales, sin red, sin tocar la configuración del usuario ni la ventana de Herdr. Sin `// DECISION` nuevas además de la actualizada.

## Verificación
- Commits en `work/HD-3`: `af20845` (visibilidades), `aec2bbd` (reutilización y esfuerzo sin modelo), `3c84b7c` (pruebas terminadas).
- `cargo fmt -p ez-herdr`: aplicado, sin cambios pendientes.
- `cargo clippy -p ez-herdr --all-targets -- -D warnings`: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 3.36s` (limpio).
- `cargo test -p ez-herdr` (segunda ejecución completa, en verde):
  - `test result: ok. 193 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.17s` (lib; incluye las 12 de `driver::choice::tests`).
  - Paridad e integración: `agent_driver_parity` 1 passed, `herdr_parity` 1 passed, `metrics_parity` 2 passed, `permission` 27 passed, `tasks_activity_parity` 1 passed, doc-tests 0. Total 225 pruebas, 0 fallos.
- Tamaños post-`fmt`: `choice.rs` 292, `choice/tests.rs` ~384, `launch.rs` 366, `driver.rs` 261 líneas; ninguna función supera 60 líneas (clippy no señala `too_many_lines`).
- Prohibidos en lo añadido: sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`/`eprintln!`, `remove_dir_all`, `unsafe`, `"cmd"`, `"/c"`, `-Command`, `.unwrap()` fuera de pruebas ni locks retenidos durante E/S o subprocesos.

## Pendiente o riesgos
- En la primera ejecución de `cargo test -p ez-herdr` falló `watcher::tests::arrancar_y_parar_no_se_cuelga` («los dos hilos se unen al parar»), prueba de hilos ajena a este brief y a los archivos tocados; al repetirla aislada y la suite completa pasó (193/193). Es una carrera de temporización en `watcher/tests.rs:234` (`is_alive()` justo tras `stop()`): riesgo intermitente preexistente, no introducido por HD-3b.
- `validate_choice` sigue sin estar conectada al comando `ez delegate` (otro brief de la ola); el alcance de HD-3b era solo la validación.
- No se ejecutó `pytest`: no se tocó el Python.

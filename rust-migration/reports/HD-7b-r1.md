# HD-7b — Corrección de la revisión de HD-7

## Resultado
- Brief/tarea: HD-7b (R7-1, R7-2)
- Estado: completado

## Cambios
- `rust/crates/ez-herdr/src/subagents/tests/registry.rs`: dos pruebas nuevas para cubrir los estados finales que HD-7 dejó sin probar:
  - `transition_table_all_sixteen_pairs` (R7-1): recorre los 16 pares `(desde, hacia)` de `TaskState`, exige que solo `en_proceso→terminado`, `en_proceso→fallido`, `terminado→validado` y `terminado→fallido` sean aceptados, y que todo lo demás (incluidos los estados finales `validado`/`fallido` y las identidades) devuelva `Invalid` con el texto exacto «<nombre> está «<estado>»; no puede pasar a «<nuevo>».» sin que el JSON cambie (se compara el archivo antes y después).
  - `create_rejects_terminal_initial_states` (R7-2): comprueba que `create` rechaza registros que nacen `terminado` o `validado` con el mensaje de error exacto y sin crear el archivo.
  - Ayudantes `ALL_STATES`, `spec_allows` (tabla de la especificación, independiente del código) y `seed` (deja un registro en cualquiera de los cuatro estados por rutas válidas).
- `rust-migration/reports/HD-7b-r1.md`: este informe.

## Decisiones
- No se tocó código de producción: `transition_allowed` y `create` de HD-7 ya aplican las reglas correctas; la revisión Detectó una laguna de cobertura, no un error real. El brief lo permite («solo pruebas, salvo que encuentres un error real»).
- La tabla permitida se escribe literalmente en `spec_allows` en vez de reutilizar `transition_allowed`, para que la prueba sea una especificación independiente y detecte una mutación en la función real.
- La mutación que motivó el brief se verificó de verdad: al añadir temporalmente `(Validated, Done)` a `transition_allowed`, `transition_table_all_sixteen_pairs` falló (`transición prohibida: … state: Done`) en el par `validado → terminado`. Se revirtió de inmediato; `git diff --stat` confirma que solo cambió el archivo de pruebas.

## Verificación
- `cargo fmt --check -p ez-herdr`: sin salida, código 0.
- `cargo clippy -p ez-herdr --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 2.47s`.
- `cargo test -p ez-herdr` (desde `rust/`):
  - `test result: ok. 203 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.28s` (unitarios; antes 201, +2 de HD-7b).
  - `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.20s` (`agent_driver_parity`).
  - `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s` (`herdr_parity`).
  - `test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s` (`metrics_parity`).
  - `test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s` (`permission`).
  - `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s` (`tasks_activity_parity`).
  - `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` (doc-tests).
- Límites: `subagents/tests/registry.rs` quedó en 553 líneas (< 600); ninguna función nueva supera 60 líneas.
- `Cargo.toml` y `Cargo.lock` sin cambios: no se añadieron dependencias.

## Pendiente o riesgos
- Ninguno dentro del alcance. No se lanzaron agentes reales, no se habló con `hc`/Herdr ni con la red, y no se tocó la configuración real del usuario ni la ventana de Herdr.

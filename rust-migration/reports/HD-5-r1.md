# HD-5 — Informe de la ola HD (r1)

## Resultado
- Brief/tarea: HD-5 (enjambre y equipo autónomo: los miembros cuelgan del primero)
- Estado: completado

## Cambios
- `rust/crates/ez-swarm/src/supervisor/host.rs`: `LaunchRequest` gana `parent`, `hc_name`, `labels`, `show`, `notify` y `outlive_parent`; `HerdrHost::run_agent` los copia al `RunAgent` de `ez-herdr`. Añade un `HerdrRuntime` falso que registra cada argv para comprobar la copia (`hc run --parent/--name/--label/--no-notify/--outlive-parent`).
- `rust/crates/ez-swarm/src/supervisor/launch.rs`: `launch_request` calcula la raíz (primer miembro en orden de tablero con dirección y distinto del que se lanza), aplica la regla §9 del contrato (`parent`, `outlive_parent`, `notify=false`, `show=Tab`, `hc_name=hc_slug(nombre)` y etiquetas `ez.swarm`/`ez.role`/`ez.agent`/`ez.parent`/`ez.model`/`ez.effort`), y emite un `info!` cuando no hay raíz. Helpers `root_address` y `member_labels`.
- `rust/crates/ez-swarm/src/supervisor/tests/launch.rs`: pruebas con `FakeHost` de primer miembro sin padre, miembros posteriores colgando de la raíz con las etiquetas del contrato, raíz todavía sin dirección y reapertura de un miembro no raíz con padre; además ajuste de `MemberSpec` para clippy.
- Nota: la reapertura/reemplazo no construye `LaunchRequest` en `recovery*`: `handoff` limpia la dirección y reinicia contadores, y el supervisor vuelve a lanzar por `launch_request`, así que hereda la misma regla sin tocar `recovery*` (verificado con la prueba de reapertura). No hubo literales `LaunchRequest { … }` en otros crates que ajustar (búsqueda en `rust/`; el `LaunchRequest` de `ez-arbiter` es otro tipo).

## Decisiones
- `// DECISION` en `launch.rs`: si falla la lectura del tablero al buscar la raíz, se abre el miembro como hoy (sin padre) y se avisa por `warn!`, para que un error de SQLite no impida el lanzamiento.
- El `info!` de «sin raíz» usa el momento del lanzamiento; los lanzamientos paralelos en la misma pasada pueden no ver todavía la dirección de la raíz y abrirse sin padre (comportamiento «como hoy» previsto por §9).

## Verificación
- `cargo fmt --check -p ez-swarm`: OK (sin salida, código 0).
- `cargo clippy -p ez-swarm --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.46s` (sin advertencias).
- `cargo test -p ez-swarm`:
  - `test result: ok. 134 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 32.65s`
  - `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` (`ezs`)
  - `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.10s` (`swarm_parity`)
  - `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` (doc-tests)
- `cargo test -p ez-app` (criterio de aceptación): `test result: ok. 349 passed; 0 failed; …` y el resto de targets en verde; las únicas advertencias son de la dependencia externa `freetype-sys`.
- Tamaños: `host.rs` 358 líneas, `launch.rs` 288, `tests/launch.rs` 239 (todos < 600); funciones nuevas < 60 líneas.

## Pendiente o riesgos
- Ninguno que bloquee. No se lanzaron agentes reales ni se tocó la ventana de Herdr: todas las pruebas usan `FakeHost` y un `HerdrRuntime` falso.
- La comprobación de `HerdrHost::run_agent` se hizo con el runtime falso definido en `host.rs` (el doble de `ez-herdr` está dentro de su `#[cfg(test)]` y no es reutilizable desde este crate); se valida sobre el argv real de `hc run`.

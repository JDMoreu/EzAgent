# HD-5b — Corrección de la revisión de HD-5 (r1)

## Resultado
- Brief/tarea: HD-5b (R5-1: la raíz es siempre el primer miembro)
- Estado: completado

## Cambios
- `rust/crates/ez-swarm/src/supervisor/launch.rs`: `root_address` ya no elige «el primer miembro con dirección distinto del que se lanza». Ahora la raíz es el **primer miembro en orden de trabajo** (`members.first()`, orden de `rowid` del tablero), tenga o no dirección: si quien se lanza es la raíz no cuelga de nadie; si no, cuelga de la dirección de la raíz solo si la tiene, y si no la tiene se abre sin padre. Actualizado el comentario que cita §9. El `info!` de «sin raíz con dirección» se conserva.
- `rust/crates/ez-swarm/src/supervisor/tests/launch.rs`: nuevo helper `three_member_swarm` y dos pruebas con `FakeHost`: (1) reabrir la raíz la deja sin padre (mata la mutación «la raíz puede ser el propio miembro»: si se excluyera a la raíz, colgaría de la dirección de `bo`); (2) con la raíz sin dirección, el tercer miembro no cuelga del segundo (`bo`, que sí tiene dirección).
- Sin cambios en `recovery*`: reabrir/reemplazar pasa por `handoff` (`update_member` limpia la dirección), y el supervisor vuelve a lanzar por `launch_request`, que usa la regla corregida. No hay otros `LaunchRequest { … }` que ajustar (búsqueda en `rust/`).

## Decisiones
- Ninguna nueva. Se mantiene el `// DECISION` previo de `launch.rs`: si falla la lectura del tablero al buscar la raíz, el miembro se abre sin padre y se avisa por `warn!`.
- Desviación respecto al contrato: `docs/designs/subagentes-herdr.md` §9 todavía describe la regla antigua («primer miembro con dirección que no sea el que se lanza»). HD-5b la corrige explícitamente. No toqué el contrato (fuera de alcance y es de solo lectura); conviene que el elefante actualice §9. El único cambio en código es el comentario que citaba §9, tal como pide el brief.
- El `info!` sigue diciendo «no hay raíz con dirección» también cuando quien se lanza es la propia raíz; el brief pide conservar el `info!` actual, así que no se cambió el texto.

## Verificación
- `cargo fmt -p ez-swarm` y `cargo fmt -p ez-swarm --check`: OK (código 0, sin salida).
- `cargo clippy -p ez-swarm --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 4.65s` (sin advertencias).
- `cargo test -p ez-swarm`: `test result: ok. 136 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 32.96s` (134 anteriores + 2 nuevas); `ezs` 0, `swarm_parity` 1 passed, doc-tests 0.
- `cargo test -p ez-app --no-run`: `Finished \`test\` profile … in 31.16s`; `ez-app` compila con los campos de `LaunchRequest` sin cambios. (No se corrieron los tests completos de `ez-app`: superan el timeout de 120 s y el brief solo exige que compile; el informe HD-5-r1 ya los dejó en verde.)
- Tamaños tras `cargo fmt`: `launch.rs` 293 líneas, `tests/launch.rs` 342 (< 600); funciones nuevas y modificadas < 60 líneas.
- Mutación comprobada por prueba: si `root_address` vuelve a excluir al miembro que se lanza, `reopening_the_root_leaves_it_without_a_parent` falla (la raíz colgaría de `t2`).

## Pendiente o riesgos
- Riesgo leve preexistente (no introducido ni tocado por este cambio): el test `supervisor::tests::a_member_without_terminal_is_launched_when_it_has_work` (en `supervisor/tests.rs`, fuera del alcance del brief) tiene una carrera — `wait_for(launched_count() == 1)` puede devolver antes de que el hilo guarde la dirección. Falló una vez en la primera pasada (`left: ""`, `right: "t-claude"`), pasó aislado y pasó en las dos repeticiones completas siguientes. No se modificó por estar fuera de alcance.
- No se lanzaron agentes reales ni se tocó la ventana de Herdr: todas las pruebas usan `FakeHost`.

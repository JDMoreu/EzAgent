# ECO-3b — `ez-ctl`: correcciones de la revisión 1

## Resultado
- Brief/tarea: ECO-3b
- Estado: completado

## Cambios
- `rust/crates/ez-ctl/src/protocol.rs`: `Caller::Fish { space, task, fish, attempt: u32 }` (§14.1) y `Tokens::revoke_all_for(predicate) -> usize`, que revoca los tokens cuyo `Caller` cumple el predicado; pruebas nuevas para ambos.
- `rust/crates/ez-ctl/src/server.rs`: reparto con un hilo `ez-ctl-accept`, 8 hilos `ez-ctl-http-<n>` para comandos y 32 `ez-ctl-wait-<n>` aparte; contador atómico de esperas, 503 con «Demasiadas esperas.» a la 33.ª, y `stop` que deja de aceptar, espera solo a los hilos de comandos y abandona los de `wait`.
- `rust/crates/ez-ctl/src/lib.rs`: reexporta `MAX_WAITS`.
- `rust/crates/ez-ctl/src/cli.rs`: `--json` global (imprime el `Reply` entero en cualquier comando), `inventory` envía `args: {}` y el inventario viaja en `data`; `report` exige `--blocking`, `--major` y `--minor` (falta cualquiera → salida 2); ayuda actualizada.
- `rust/crates/ez-ctl/tests/support/mod.rs`: `Caller::Fish` adaptado con `task` y `attempt`, y constante `TASK`.
- `rust/crates/ez-ctl/tests/roundtrip.rs`: `inventory` con `args: {}`, `--json` en `status` e `inventory` (incluso antes del comando), `report` con los tres conteos obligatorios y `Caller::Fish` adaptado.
- `rust/crates/ez-ctl/tests/cli_errors.rs`: casos de `report` sin conteos y sin `--major` → salida 2 con el mensaje «Falta --major.».
- `rust/crates/ez-ctl/tests/waits.rs`: nuevo; 32 `wait` retenidos con `report` y `status` atendidos en menos de 2 s, 33.ª espera → 503 con el cuerpo exacto, y `stop` con un `wait` retenido en menos de 2 s.
- `rust/crates/ez-ctl/src/bin/ez.rs`, `src/client.rs`: sin cambios en esta ronda.
- `rust/crates/ez-ctl/src/skill.md`: **sin tocar** (contrato del elefante).

## Decisiones
- `// DECISION`: `--json` solo controla la salida; en `inventory` el inventario viaja en `data`, no en `args` (§14.8, m02).
- `// DECISION`: los hilos de `wait` se abandonan en `stop` y vuelven solos cuando el `Handler` responde. El brief manda no esperarlos; §14.8 dice «stop despierta las esperas», así que se sigue el brief por ser más específico y se registra aquí la diferencia.
- `// DECISION`: `revoke_all_for` devuelve el número de tokens revocados; el brief no fija el tipo de retorno y el contador resulta útil para la ola 2.
- `// DECISION` heredada de ECO-3 y conservada: la ruta se valida antes que el token, para que una ruta desconocida dé 404 sin credenciales; todo error cierra la conexión.
- Al empezar había una traducción de comentarios sin confirmar, ajena a ECO-3b; se descartó para mantener el diff limitado al alcance del brief.
- `MAX_WAITS` se reexporta para que las pruebas y la ola 2 puedan comprobar el tope sin duplicar el valor.

## Verificación
Ejecutada desde `rust/`, solo para el crate propio:

- `cargo fmt -p ez-ctl --check`: exit 0, sin diferencias (`FMT_OK`).
- `cargo clippy -p ez-ctl --all-targets -- -D warnings`: exit 0, sin advertencias.
  ```
  Checking ez-ctl v0.1.0
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.38s
  ```
- `cargo test -p ez-ctl`: exit 0, **24 passed / 0 failed / 0 ignored** (4 unitarias + 7 de CLI + 6 de ida y vuelta + 5 de servidor + 2 de esperas).
  ```
  test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.07s
  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.06s
  test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.07s
  test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  ```
- Comprobaciones adicionales: ningún `.rs` supera 600 líneas (máximo `src/cli.rs`, 374); ninguna función supera 60 líneas (`CtlServer::start`, 42); sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`/`eprintln!`, `unsafe` ni `.unwrap()` en `src/`; el único `remove_dir_all` está en `tests/support/mod.rs` sobre la carpeta temporal única bajo `std::env::temp_dir()`.
- Métricas: no aplica.

## Pendiente o riesgos
- Cada servidor crea 41 hilos (1 repartidor + 8 comandos + 32 esperas). Es el coste de que 32 `wait` retenidos no bloqueen un `report` o un `status`.
- `stop` no espera a los hilos de `wait`: si un `Handler` ajeno no volviera nunca, ese hilo quedaría abandonado hasta el fin del proceso. El contrato del `Handler` exige no entrar en pánico y el árbitro libera la espera al publicar un evento o vencer el tope.
- Si falla el lanzamiento de algún hilo de espera se registra un aviso y el tope efectivo baja de 32; no se ha dado en las pruebas.
- `rust-migration/reports/ECO-3-r1.md` estaba sin seguimiento al empezar esta ronda (informe de ECO-3); no se tocó ni se incluyó en estos commits por estar fuera del alcance.
- Solo se compiló y probó `-p ez-ctl`, como pide el reparto de peces en paralelo.

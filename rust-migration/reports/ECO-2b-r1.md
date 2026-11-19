## Resultado
- Brief/tarea: ECO-2b — `ez-arbiter`: correcciones de la revisión 1 (parte 1 de 2).
- Estado: completado.

## Cambios
- `rust/crates/ez-arbiter/src/types.rs`: `FishKey` (serde, `Hash`), `Caller::Fish(FishKey)`, `Delivery`/`Launched`, `FishLauncher::launch -> Launched` y `revoke`, `LaunchRequest.token`.
- `rust/crates/ez-arbiter/src/state.rs`: `WorkItem`/`WorkKind`/`WorkState`, `FishRecord` por intento con `FishKey`, `ReportRecord.sha`, huellas `encargo_sha`/`team_sha` y consultas por clave.
- `rust/crates/ez-arbiter/src/events.rs`: se elimina la redacción de tokens; ningún evento lleva `token`.
- `rust/crates/ez-arbiter/src/paths.rs`: única función `inside(root, path) -> Result<PathBuf, String>` (canonicaliza raíz y destino o su padre); pruebas con `..`, prefijo compartido y archivo inexistente.
- `rust/crates/ez-arbiter/src/criteria.rs`: rangos de §14.6 (mutaciones 1–20, rondas 2–20, horas finitas >0 y ≤24, segundos 1–3600, descripción no vacía) y línea de validación en la tarjeta.
- `rust/crates/ez-arbiter/src/verify.rs`: `taskkill /T /F /PID` al vencer el tope más `kill`; lectores abandonados a los 2 s con `recv_timeout` (sin `join` sin límite); especie helpers `wait_for_child`/`kill_tree`.
- `rust/crates/ez-arbiter/src/texts.rs`: textos literales del brief (clave inactiva, entrega fallida/no confirmada, ruta del informe, `pass` con hallazgos, trabajo pendiente, QA iniciado, plan solo en ronda 1, artefactos, intentos, mutaciones y validación).
- `rust/crates/ez-arbiter/src/arbiter/mod.rs`: id `t`+12 hex, hc `ez-{8 hex}-{pez}[-a{n}]` con etiqueta `ez_attempt`, registro de tokens en memoria (fuera de `estado.json`), resolución de tarea por clave vigente, revocaciones y señales `fish_blocked`/`fish_exited`/`fish_failed(&FishKey, motivo)`.
- `rust/crates/ez-arbiter/src/arbiter/emit.rs`: bitácora escribe el evento directamente (sin token ni redacción).
- `rust/crates/ez-arbiter/src/arbiter/fish.rs`: `fish_new` dividido en `prepare_launch`/`deliver`; entrega `Delivered`/`Unconfirmed`/`Failed` y trabajo fallido con evento `fish_failed`.
- `rust/crates/ez-arbiter/src/arbiter/fish/order.rs` (nuevo): fases de ronda 1, developers antes del QA, ronda solo de verificación, un verificador vivo por ronda y tres intentos.
- `rust/crates/ez-arbiter/src/arbiter/fish/work.rs` (nuevo): correcciones con línea exacta e informes con ruta exacta, copia privada y sha256.
- `rust/crates/ez-arbiter/src/arbiter/round.rs`: `round.close` rechaza trabajo pendiente; ronda limpia exige todos los informes limpios y sin cambios de planner/revisor/validador; comprobación de artefactos en apertura, verificación y cierre.
- `rust/crates/ez-arbiter/src/arbiter/task_cmds.rs`: congelado privado con sha256 de encargo, criterios y equipo; pausa `artefactos_modificados` y restauración al reanudar; revocación al terminar/rechazar/detener.
- `rust/crates/ez-arbiter/src/arbiter/team.rs`: congela el equipo aprobado y revoca tokens al rechazarlo.
- `rust/crates/ez-arbiter/src/arbiter/load.rs`: sin cambios de lógica (recarga de tareas no finales).
- `rust/crates/ez-arbiter/src/lib.rs`: doc-test y reexportaciones de `FishKey`, `Delivery` y `Launched`.
- `rust/crates/ez-arbiter/tests/arbiter/support.rs`: lanzador falso con `Delivery`, fallos y revocaciones; resolución de clave/ruta de informe desde `estado.json`; `setup` con informe del planificador.
- `rust/crates/ez-arbiter/tests/arbiter/fixes.rs` (nuevo): 15 pruebas de identidad, tokens, entregas, rutas, fases, artefactos, intentos y topes.
- `rust/crates/ez-arbiter/tests/arbiter/{task,fish,round,events,arbiter.rs}`: adaptadas a claves, fases, rutas exactas y nuevo orden de eventos.
- `rust-migration/reports/ECO-2b-r1.md` (este archivo): informe de la entrega.

## Decisiones
- `// DECISION` en `src/arbiter/fish/order.rs`: hasta ECO-2c la ronda solo de verificación compara la foto de git; el brief prevé sustituirla por la huella de contenido.
- `// DECISION` en `src/arbiter/round.rs` (heredada): las paradas posteriores al cierre de ronda se informan con `Ok` en el mensaje, no como rechazo.
- `// DECISION` en `src/arbiter/task_cmds.rs` (heredada): el título vacío recibe «Tarea» para que la tarjeta sea legible.
- `// DECISION` en `src/state.rs` (heredada): la atribución compara rutas exactas; el cambio de contenido de un archivo sin seguimiento no se atribuye.
- Sin `// DECISION`: los tokens viven solo en un mapa en memoria de `Inner` (clave `FishKey::storage_key`) para que no aparezcan en `estado.json`, bitácora ni eventos; el equipo congelado se serializa desde `Task` (el árbitro no lee `equipo.toml`, que recibe como `Vec<Seat>`); `attempt` cuenta los peces lanzados de ese papel en la ronda, y la corrección reemplaza el `WorkItem` pendiente de esa clave para mantener un solo trabajo vivo.

## Verificación
- `cargo fmt -p ez-arbiter -- --check`: `FMT_EXIT=0` (sin diferencias).
- `cargo clippy -p ez-arbiter --all-targets -- -D warnings`: `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.55s`, `CLIPPY_EXIT=0`.
- `cargo test -p ez-arbiter`:
  - `test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s` (unitarias).
  - `test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 10.20s` (integración).
  - `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s` (doc-test).
- Tamaño: ningún `.rs` supera 600 líneas (máximo 537 en pruebas, 484 en fuente) ni ninguna función 60 líneas (comprobado con script de conteo de llaves).
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`/`eprintln!`, `unsafe`, `.unwrap()` en `src/` ni `remove_dir_all` fuera de carpetas temporales propias.

## Pendiente o riesgos
- Las pruebas de `inside` cubren `..`, prefijo compartido (`proyecto` frente a `proyecto-otro`) y archivo inexistente; no se probó un junction/enlace simbólico por requerir permisos especiales en Windows.
- ECO-2c queda pendiente: huellas de contenido, ejecutor secuencial por espacio, pausa al reiniciar, acciones de pausa (`more_time`, `stop` con tarjeta) y `ArbiterEvent` tipado con `seq`.
- `FishLauncher` cambió de firma (`launch -> Launched`, nuevo `revoke`) y `Caller::Fish` pasó a tupla con `FishKey`; cualquier implementación/integrador fuera de este crate (ola 2) debe adaptarse. `ez-ctl` define su propio `Caller` y no depende de `ez-arbiter`, así que no se rompió.
- El token se registra en el lanzador antes de lanzar el proceso (§14.1); este crate solo se lo entrega en `LaunchRequest` y guarda la copia en memoria para revocarlo. Al reiniciar, ese mapa se pierde: ECO-2c debe revocar los tokens persistentes del lanzador.
- No se tocó nada fuera de `rust/crates/ez-arbiter/**` salvo este informe; `Cargo.toml`, `Cargo.lock` y los contratos de `ez-team` quedaron intactos.

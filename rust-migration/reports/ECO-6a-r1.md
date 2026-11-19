# Informe ECO-6a-r1 — Motor del espacio de trabajo en `ez-hub`

## Resultado
- Brief/tarea: ECO-6a (integración, parte 1: motor del espacio de trabajo en `ez-hub`)
- Estado: completado

## Cambios
- `rust/crates/ez-ctl/src/server.rs`: `CtlServer::start_on(port, handler, tokens)`; `start` delega con puerto 0. Sin cambios de comportamiento en el reparto ni en los hilos.
- `rust/crates/ez-ctl/tests/server_errors.rs`: dos pruebas nuevas (puerto libre y puerto ocupado).
- `rust/crates/ez-hub/Cargo.toml`: dependencias de ruta `ez-arbiter`, `ez-ctl`, `ez-herdr`, `ez-team` y `ez-term`.
- `rust/crates/ez-hub/src/runtime/mod.rs` (nuevo): `HubRuntime::start`, arranque degradado sin hc, registro de espacios y tokens de elefantes, `start_on(47830)` con respaldo en puerto libre, `ensure_elephant`, `space_fish`, `elephant_send` y `subscribe_changed`.
- `rust/crates/ez-hub/src/runtime/handler.rs` (nuevo): `Handler` de `ez-ctl` sobre el árbitro, con `ArbiterApi` (seam para dobles), conversión de `Caller`, inventario en caché de 60 s, `team.propose` y `wait`.
- `rust/crates/ez-hub/src/runtime/launcher.rs` (nuevo): trait `Terminals` con adaptador `HcClient` y doble; `FishLauncher` y `InstructionWriter` sobre `ez-team`/`ez-herdr`; lanzamiento del elefante.
- `rust/crates/ez-hub/src/runtime/events.rs` (nuevo): hilo `ez-hub-hc-events`, traducción de estados a `fish_blocked`/`fish_failed`/`fish_exited` y aviso `hub://changed` por canal.
- `rust/crates/ez-hub/src/runtime/spaces.rs` (nuevo): lectura de `espacios.json` en formato viejo (`argv`) y nuevo (`modelo`/`esfuerzo`), y `elefantes.json` con escritura atómica.
- `rust/crates/ez-hub/src/runtime/types.rs` (nuevo): `RuntimeStatus` y `FishInfo` serializables.
- `rust/crates/ez-hub/src/runtime/tests.rs` y `runtime/tests/handler.rs` (nuevos): dobles de terminales y árbitro, git real, cobertura de cada comando y cada `Caller`, `team.propose` válido y con errores, `wait` con y sin evento, token registrado antes del lanzamiento, entorno, revocación, elefante reutilizado/relanzado/lanzado, peces y eventos de hc.
- `rust/crates/ez-hub/src/commands.rs`: campo `runtime` en `AppState`, creación solo fuera de `--demo`, métodos y comandos Tauri `runtime_status`, `space_elephant`, `space_fish` y `elephant_send`.
- `rust/crates/ez-hub/src/lib.rs`: módulo `runtime`, registro de los cuatro comandos y hilo `ez-hub-changed` que emite `hub://changed`.
- `rust/Cargo.lock`: efecto de compilar el crate con las dependencias de ruta nuevas.

## Decisiones
- `handler.rs:80`: el inventario se calcula fuera del candado; dos peticiones simultáneas pueden repetir el sondeo, pero ninguna retiene un lock durante E/S.
- `launcher.rs:273`: las etiquetas hc del árbitro son la fuente de identidad del pez; si falta alguna se usan valores vacíos y el árbitro rechaza la acción.
- `spaces.rs:57`: el formato viejo de `espacios.json` no tiene modelo; se deja vacío para que la persona lo configure en lugar de adivinarlo desde `argv`.
- `mod.rs:291`: `ez.exe` lleva extensión en Windows y no en el resto.
- `mod.rs:312`: el `CtlServer` se suelta a propósito; sus hilos viven mientras la ventana está abierta y el proceso los termina al salir.
- Aviso de cambios por canal en vez de `AppHandle` dentro del motor: meter `AppHandle` en la biblioteca hacía que el binario de pruebas enlazara los DLL de la GUI de Tauri y muriera al arrancar con `STATUS_ENTRYPOINT_NOT_FOUND`. El hilo de Tauri en `lib.rs::run` emite `hub://changed`.
- `ensure_elephant` recuerda por espacio cuándo ya se relanzó (bandera `elephants_need_relaunch`), para no matar y relanzar el elefante en cada llamada.
- No se modifica `src/space.rs` ni `ui/**`: fuera del alcance del brief. El formato nuevo se lee dentro de `runtime/spaces.rs`.
- Las etiquetas `ez_agent`, `ez_model` y `ez_role_fish` se añaden al `RunSpec` para que `space_fish` pueda informar papel, agente y modelo sin leer archivos del proyecto.

## Verificación
- `cargo fmt -p ez-hub -p ez-ctl --check`: `FMT_OK` (sin diferencias).
- `cargo clippy -p ez-hub -p ez-ctl --all-targets -- -D warnings`: `Finished 'dev' profile [unoptimized + debuginfo] target(s) in 0.78s`.
- `cargo test -p ez-hub -p ez-ctl`: `ez-ctl` 4 + 7 + 6 + 7 + 2 pruebas en verde; `ez-hub` **75 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.20s**.
- `cargo run -p ez-hub --bin ezagent-hub -- --demo --selfcheck <temporal>.json`: código de salida **0**; JSON:
  `{"errores":[],"cabecera":"Elefante · PowerShell","terminales":3,"peces":2,"eventos":5,"tarjetas":1,"formulario_abierto":false}`
- Límites: ningún `.rs` nuevo supera 600 líneas (máximo 384) y ninguna función supera 60; sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`/`eprintln!`, `unsafe` ni `.unwrap()`.

## Pendiente o riesgos
- `rust/crates/ez-ctl/tests/waits.rs::thirty_two_waits_do_not_block_report_and_the_next_wait_is_503` es sensible a carga: con 32 esperas y tope de 8 s falló dos veces al ejecutar `ez-hub` y `ez-ctl` juntos justo después de compilar («solo entraron 30/31 esperas»), y pasó al repetirlo (0,13 s). No se modificó: no es una prueba de `server.rs` y el brief no pide cambiarla.
- El arranque normal depende de `hc` y de que `ez.exe` esté junto al ejecutable; sin ellos el motor queda degradado y la vista puede mostrar el motivo con `runtime_status`. No se probó contra un `hc` real en esta ola (las pruebas usan dobles), conforme al alcance.
- `espacios.json` en formato viejo deja el modelo vacío: `ensure_elephant` responde «El espacio necesita configurar el modelo del elefante en agentes.toml.» hasta que se escriba el formato nuevo (la vista para configurarlo es ECO-6b).
- Esta entrega cubre el motor; la vista de tarjetas y traza (ECO-6b) y el ejecutor secuencial/eventos tipados de ECO-2c quedan fuera de alcance y no se consumen aquí.

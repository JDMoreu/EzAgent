# ECO-5 — `ez-hub`: la ventana (Tauri 2 + xterm.js) con modo demostración

## Resultado
- Brief/tarea: ECO-5
- Estado: completado

## Cambios
- `rust/Cargo.lock`: nuevas dependencias del crate (`tauri`, `tauri-build`, `portable-pty`, `uuid`, `parking_lot`, `crossbeam-channel`, `tracing-subscriber`) como efecto de compilar `ez-hub`.
- `rust/crates/ez-hub/Cargo.toml`: binario `ezagent-hub`, `build.rs` y las dependencias permitidas por el brief.
- `rust/crates/ez-hub/build.rs`: `tauri_build::build()`.
- `rust/crates/ez-hub/tauri.conf.json`: producto «EzAgent», `dev.ezagent.hub`, vista `ui`, ventana `main` 1400×900 (mínimo 900×600), CSP exigida, `bundle.active: false` e icono.
- `rust/crates/ez-hub/capabilities/default.json`: ventana `main`, solo `core:default`.
- `rust/crates/ez-hub/icons/icon.ico`: copia de `packaging/icon.ico` para el icono de la app.
- `rust/crates/ez-hub/.gitignore`: ignora `gen/`, generado por `tauri-build` (no se commitea).
- `rust/crates/ez-hub/src/lib.rs`: módulos públicos, `init_tracing()` y el `Builder` de Tauri con los nueve comandos.
- `rust/crates/ez-hub/src/main.rs`: entry point de `ezagent-hub`, lee `--demo` y reporta fallos por `tracing`.
- `rust/crates/ez-hub/src/space.rs`: modelos de `espacios.json`, validación de carpeta/agente y guardado atómico (temporal + renombrar).
- `rust/crates/ez-hub/src/input.rs`: codificación de la barra (una línea + `\r`; varias, pegado entre corchetes + `\r`).
- `rust/crates/ez-hub/src/hub.rs`: modelos de peces/traza/tarjetas, tarjeta de equipo de §6 y estado de demostración.
- `rust/crates/ez-hub/src/pty.rs`: terminales ConPTY con hilos de lectura (8 KiB), agrupación (16 ms), escritura y espera; resuelve `argv[0]`, avisa del código de salida y mata el proceso al cerrar.
- `rust/crates/ez-hub/src/commands.rs`: `AppState` con cerrojos cortos y los comandos Tauri (`spaces_list`, `space_create`, `view_open`, `view_write`, `view_resize`, `view_close`, `elephant_input`, `hub_state`, `card_action`).
- `rust/crates/ez-hub/src/texts.rs`: mensajes del servidor en español neutro.
- `rust/crates/ez-hub/src/testutil.rs`: carpeta temporal con nombre único, borrada al soltar.
- `rust/crates/ez-hub/ui/index.html`: esqueleto de las tres columnas y el formulario de espacio.
- `rust/crates/ez-hub/ui/css/app.css`: tema oscuro con los tokens de color exigidos, rejilla 220 px / centro / 40 % y estados de los chips.
- `rust/crates/ez-hub/ui/js/texts.js`: todos los textos de la vista, en español neutro.
- `rust/crates/ez-hub/ui/js/spaces.js`: carga y creación de espacios (nada de red fuera del IPC local).
- `rust/crates/ez-hub/ui/js/terminal.js`: xterm.js con `Channel` de Tauri; bytes crudos, eventos de salida y mensaje «No encontré … en PATH.».
- `rust/crates/ez-hub/ui/js/panel.js`: tarjetas de peces (expansión al hacer clic), traza y aprobaciones con comentario obligatorio al rechazar.
- `rust/crates/ez-hub/ui/js/main.js`: arranque, selección de espacio, envío por Enter/Mayús+Enter, pestañas y `hub://changed`.
- `rust/crates/ez-hub/ui/vendor/xterm/` y `ui/vendor/xterm-addon-fit/`: `@xterm/xterm 6.0.0` y `@xterm/addon-fit 0.11.0` bajados de registry.npmjs.org, con su `LICENSE`; módulos ES sin npm ni empaquetador.
- `rust-migration/reports/ECO-5-r1.md`: este informe.

## Decisiones
- `src/commands.rs` (`view_open`): el brief no daba forma de asociar `elephant_input {space, text}` con la vista del elefante; `view_open` acepta `space` opcional y registra esa vista como elefante del espacio. Sin ese parámetro la barra de entrada no podría escribir en la terminal en la ola 1 (no hay `hc`).
- `src/commands.rs` (`create_space`): se escribe `espacios.json` fuera del cerrojo; si falla, la lista en memoria queda intacta.
- `src/hub.rs` (`team_card`): el encabezado «Equipo propuesto para «…»:» va en `title` y las filas `- Papel: …` en `body`; la vista ya pinta el título y repetirlo se vería duplicado.
- `src/pty.rs`: cuatro hilos por vista (lector, agrupador, escritor, espera) para no retener cerrojos durante E/S; `clone_killer()` permite matar el proceso desde otro hilo sin compartir el `Child` en `wait()`.
- `src/pty.rs`: se resuelve `argv[0]` en PATH antes de abrir el PTY, tanto para el mensaje «No encontré … en PATH.» como para no depender del PATH del hijo.
- `src/pty.rs` (pruebas): ConPTY envía `ESC[6n` al arrancar y retiene la salida hasta que el cliente responde; xterm.js lo hace solo y el doble de prueba responde `ESC[1;1R` a mano.
- En demostración, «+ Nuevo espacio» crea espacios solo en memoria (no escribe `espacios.json`), coherente con «no escribe espacios.json».
- La ruta de carpeta se guarda en forma canónica absoluta para que el espacio siga apuntando al mismo sitio aunque cambie el directorio de trabajo.

## Verificación
- `cargo fmt --all --check`: sin salida, código 0.
- `cargo clippy -p ez-hub --all-targets -- -D warnings`: ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.74s``.
- `cargo test -p ez-hub`: `test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.48s` (más 0 pruebas en `src/main.rs` y 0 doc-tests).
- `cargo build -p ez-hub --bin ezagent-hub`: ``Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.19s``.
- `ezagent-hub --demo`: proceso vivo durante 8 s con `MainWindowHandle` distinto de cero; `stdout` solo `INFO abriendo el hub demo=true`, `stderr` vacío; se terminó con `taskkill /T /F` y no queda proceso `ezagent-hub`. La revisión visual la hace el elefante.
- Vista sin recursos remotos: ni una sola URL `http(s)` en `index.html`, CSS o JS (solo los textos `ipc:` de la configuración de Tauri).
- Límites: ningún `.rs` supera 416 líneas; 28 pruebas propias; sin `todo!`, `unimplemented!`, `#[ignore]`, `println!`, `unsafe` ni `.unwrap()` fuera de pruebas.

## Pendiente o riesgos
- No se pudo comprobar visualmente el render de xterm.js en esta sesión: la revisión de la captura del modo demostración queda para el elefante (criterio 6 de la ola 1).
- `card_action` y `hub_state` son de momento puramente demostrativos: en la ola 2 el árbitro debe poblar peces, traza y tarjetas y ejecutar las acciones de verdad.
- El modo demostración usa `powershell.exe -NoLogo`; en equipos sin PowerShell la vista mostrará «No encontré «powershell.exe» en PATH.»
- `Cargo.lock` y el binario se construyeron con el resto del workspace sin cambios de esta tarea; no se lanzaron comandos de cargo para otros crates.

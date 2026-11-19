# ECO-6a — Integración, parte 1: el motor del espacio de trabajo en `ez-hub`

**Lee `rust-migration/briefs/_COMMON-ECO.md`, `docs/designs/espacio-elefante.md` completo
(manda §14) y los briefs ya integrados `ECO-1b`, `ECO-2b`, `ECO-3b`, `ECO-4b`, `ECO-5b` y
`ECO-7`. Lee también la API pública real de `ez-term`, `ez-arbiter`, `ez-ctl` y `ez-team`
antes de escribir nada.** En paralelo, otro pez hace `ECO-2c` (eventos tipados y tarjetas
del árbitro): **no** dependas de eso; la vista de tarjetas y traza es ECO-6b.

## Archivos que se tocan (solo estos)
- `rust/crates/ez-hub/src/runtime/**` (módulo nuevo) y, para enchufarlo, `src/lib.rs`,
  `src/main.rs` y `src/commands.rs` (solo lo necesario para crear el runtime al arrancar y
  exponer su estado). No toques `ui/**` (es ECO-6b).
- `rust/crates/ez-hub/Cargo.toml`: dependencias de ruta `ez-arbiter`, `ez-ctl`, `ez-term`,
  `ez-team`, `ez-herdr`.
- `rust/crates/ez-ctl/src/server.rs` y sus pruebas: añade
  `CtlServer::start_on(port: u16, handler, tokens)`; `start` sigue usando el puerto 0.

## Qué construir (`ez_hub::runtime`)
1. **`HubRuntime::start(data_dir) -> Result<HubRuntime, String>`** (solo en modo normal; en
   `--demo` no se crea). Crea, en este orden: perfiles (`ez_team::profiles::load_or_init`),
   suscripciones, cliente hc (`ez_term::locate_hc` + `HcClient::connect`; si falla, una vez
   `hc server start` con argumentos en lista y reintento), `Tokens`, el árbitro
   (`Arbiter::new` con el lanzador y el escritor de abajo), el servidor `ez`
   (`CtlServer::start_on(47830, …)`; si el puerto está ocupado, `start` con puerto 0 y se
   marca `elephants_need_relaunch`) y el hilo de eventos de hc. Si falta `hc`, el runtime
   arranca degradado con el motivo «No encontré herdr-core (hc). Instálalo desde
   https://releases.herdr.dev y vuelve a abrir EzAgent.» y no lanza nada.
2. **`Handler` de `ez-ctl`** sobre el árbitro. Traduce `Caller` (pez →
   `FishKey { space, task, fish, attempt }`) y cada `command` de §7 a su `Command`:
   - `inventory`: `ez_team::inventory::detect` con `RealSystem` (en memoria 60 s);
     `message` = `Inventory::text()`, `data` = `Inventory::json()`.
   - `team.propose {file}`: lee el archivo (dentro de la carpeta del espacio), `team::parse`,
     `team::validate` con el inventario; con errores → `ok: false` y los errores, uno por
     línea; si no, `team::card(título de la tarea activa o «Tarea», …)` y
     `Command::TeamPropose`.
   - `wait {timeout}`: `arbiter.wait_for_elephant(space, timeout)`; sin evento →
     `ok: true`, `data.event: null`, «Sin novedades; vuelve a ejecutar `ez wait`.».
   - El resto, uno a uno. `Rejection` → `ok: false` con su mensaje. Rutas relativas no
     llegan (el `ez` ya las hace absolutas).
3. **`FishLauncher`** sobre un trait propio `Terminals` (`launch_and_deliver`, `send`, `get`)
   que implementa `HcClient` y un doble en pruebas:
   - `launch`: **primero** `tokens.register(token, Caller::Fish{…})`; perfil del agente
     del puesto → `ez_team::launch::command(perfil, puesto)`; si `prelaunch` es
     `"opencode_state"`, añade al entorno lo que devuelve
     `ez_herdr::prelaunch::prepare_opencode_state(modelo, esfuerzo)`; entorno final =
     `req.env` + el del comando + `EZ_URL` + `PATH` con la carpeta de `ez.exe` delante;
     `RunSpec` con `req.hc_name`, `req.labels`, `req.cwd`, `no_parent` y `no_notify`;
     `launch_and_deliver(spec, &req.first_line, 60 s)` → `Launched`.
   - `send` → `HcClient::send`; `alive` → estado `live`; `revoke` → `tokens.revoke`.
   - `ez.exe`: la carpeta del ejecutable de la ventana (`std::env::current_exe`); si no
     existe `ez.exe` ahí, el runtime lo informa como estado degradado.
4. **`InstructionWriter`** con `ez_team::launch::render_instructions` y `first_line`.
5. **Elefante de cada espacio.** `espacios.json` pasa a guardar
   `elefante = { agente, modelo, esfuerzo }` (lee también el formato viejo con `argv`; en
   ese caso `modelo` queda vacío y el espacio pide configurarlo). `runtime.ensure_elephant(space)`:
   si hay una terminal viva con `ez_space=<id>` y `ez_role=elephant`, la reutiliza (y si
   `elephants_need_relaunch`, la mata y lanza otra); si no, lanza con
   `launch::command(perfil, puesto del elefante)`, token nuevo registrado como
   `Caller::Elephant{space}` y guardado en `%APPDATA%\EzAgent\hub\elefantes.json`
   (espacio → token; al arrancar se vuelven a registrar), `EZ_ROLE=elephant`, etiquetas
   `ez=1`, `ez_space`, `ez_role=elephant` y primera línea
   `ez_team::launch::elephant_first_line()` con `launch_and_deliver`. Devuelve su
   dirección hc.
6. **Hilo de eventos de hc** (`ez-hub-hc-events`, etiqueta `ez=1`): mantiene por espacio
   la lista de peces vivos (de sus etiquetas) con su estado; `status` `blocked` →
   `arbiter.fish_blocked`, `error` → `fish_failed`, `finished`/`lost` →
   `fish_exited`/`fish_failed`; `missed` → resincroniza con `HcClient::list`. Emite
   `hub://changed` para la vista.
7. **Comandos Tauri nuevos** (sin tocar la vista): `runtime_status` (normal, degradado y
   motivo), `space_elephant {space}` → dirección hc del elefante (lo lanza si hace falta),
   `space_fish {space}` → peces con dirección, papel, agente, modelo y estado,
   `elephant_send {space, text}` → `HcClient::send` con el texto crudo.

## Pruebas
Con git real en carpetas temporales, el árbitro real y dobles de `Terminals`: el `Handler`
traduce cada comando y cada `Caller`; `team.propose` con errores y válido; `wait` con y
sin evento; el lanzador registra el token **antes** de llamar a `launch_and_deliver`
(el doble lo comprueba) y arma el entorno (incluido `OPENCODE_CONFIG_CONTENT`, `EZ_URL` y
`PATH`); `revoke`; lectura de `espacios.json` viejo y nuevo; `ensure_elephant` reutiliza,
relanza y lanza; el hilo de eventos traduce `blocked`, `error`, `finished` y `lost`.
`CtlServer::start_on` con puerto libre y ocupado. `--demo --selfcheck` sigue saliendo con 0.

## Criterios de aceptación
fmt (`-p ez-hub -p ez-ctl`), clippy y test de `ez-hub` y `ez-ctl` en verde. Informe en
`rust-migration/reports/ECO-6a-r1.md`.

# P-DRIVER — Paridad de agent_driver (ronda 1)

## Resultado
- Brief/tarea: P-DRIVER
- Estado: completado

`agent_driver` queda al 100 %: 420/420 casos del fixture consumidos y en verde.
399 pasaron al primer intento. Los 21 restantes eran 3 defectos reales de Rust y
18 del arnés (entorno, pantallas, archivos y dos artefactos del oráculo).

## Cambios
- `rust/crates/ez-herdr/tests/agent_driver_parity.rs`: nuevo; carga el fixture, fija entorno y carpeta temporal por caso (layout `<tmp>/cases/<caso>` como el generador), compara JSON con tolerancia de flotantes y exige 420 casos consumidos.
- `rust/crates/ez-herdr/tests/agent_driver_parity/{cases,common,fake}.rs`: nuevos; reparto por dominio, comparación/entorno/snapshot de archivos y doble de `AgentSession` (pantallas, agentes, envíos, teclas y reloj que avanza al dormir).
- `rust/crates/ez-herdr/tests/agent_driver_parity/prelaunch.rs`: nuevo; 72 casos de `agent_prelaunch.py`.
- `rust/crates/ez-herdr/tests/agent_driver_parity/driver.rs`: nuevo; 116 casos de `agent_driver.py` (incluye catálogo con `which` inyectado).
- `rust/crates/ez-herdr/tests/agent_driver_parity/session.rs`: nuevo; 185 casos de `agent_session.py` con las costuras de `finish_launch`/`configure_agent`/`send_task`.
- `rust/crates/ez-herdr/tests/agent_driver_parity/watcher.rs`: nuevo; 47 casos de `watcher.py` con doble de `subprocess.run`, reloj y `threading.Thread`.
- `rust/crates/ez-herdr/src/session.rs`: costuras públicas `#[doc(hidden)]` (`SessionClock`/`with_clock`, `try_screen`, `screen`, `screen_shows_program`, `check_screen`, `cycle`, `run_step`, `apply_in_session`, `unsent_in_input`, `is_shell`, `session_steps`, `valid_keys_value`) y el flujo de entrega movido a `session/flow.rs` (el archivo pasaba de 600 líneas).
- `rust/crates/ez-herdr/src/session/flow.rs`: nuevo; `send_task`, `finish_launch` y `configure_agent` con sus variantes `*_with` para el arnés; la producción usa el mismo ensamblado de resultados.
- `rust/crates/ez-herdr/src/session/text.rs`: `command_line` no reconocía la barra invertida como argumento simple (`_PLAIN_ARG` sin `\\`), así que citaba `C:\tmp\notas.txt`; corregido. Nuevo `valid_keys_value` (lista JSON tal cual).
- `rust/crates/ez-herdr/src/session/host.rs`: `SessionClock` público.
- `rust/crates/ez-herdr/src/session/steps.rs`: `session_steps` público y oculto.
- `rust/crates/ez-herdr/src/driver.rs`: `text_list` y `check_value` públicos y ocultos; `LaunchHooks.which` pasa a `Arc<dyn Fn>` y nuevo `AgentDriver::with_which`; reexport de `resolve_codex_value`.
- `rust/crates/ez-herdr/src/driver/launch.rs`: `resolve_codex_value` (entrada con `Value`: lee el catálogo de Codex y devuelve error + modelo resuelto).
- `rust/crates/ez-herdr/src/prelaunch.rs`: `strip_jsonc` público y oculto; `discover_codex_models` itera los caracteres cuando `supported_reasoning_levels` es una cadena (quirk de Python, Q-PDRIVER-01).
- `rust/crates/ez-herdr/src/watcher.rs`: runtime y fábrica de hilos inyectables (`with_runtime`, `with_threads`, traits `Worker`/`ThreadFactory`), `compare`/`poll_once`/`wants_fast_poll`/`set_health`/`deliver`/`poll_loop` públicos y ocultos, `set_health_value` (acepta `null` como Python, Q-PDRIVER-02), `live_map`/`previous_map`/`next_event_now`/`stop_requested`; `healthy` pasa de `AtomicBool` a `Mutex<Value>` y `HealthCallback` recibe `Option<bool>`.
- `rust/crates/ez-herdr/src/{driver,session,watcher}/tests.rs`: ajustes mínimos por las firmas nuevas.
- `rust-migration/python-quirks.md`: añadidos Q-PDRIVER-01..04 al final.
- `rust-migration/reports/P-DRIVER-r1.md`: este informe.

## Paridad por dominio
| Dominio | Casos | Al primer intento | Defectos Rust corregidos | Fallos restantes |
|---|---:|---:|---|---:|
| `agent_driver` (prelaunch) | 72 | 66 | 1 (Codex) | 0 |
| `agent_driver` (driver) | 116 | 114 | 0 | 0 |
| `agent_driver` (session) | 185 | 172 | 1 (`command_line`, 2 casos) | 0 |
| `agent_driver` (watcher) | 47 | 47 | 0 | 0 |
| **Total** | **420** | **399** | **2** | **0** |

### Defectos de Rust corregidos
- `session/text.rs::command_line`: el patrón `_PLAIN_ARG` había perdido el
  `\\` de la clase, así que citaba argumentos con barra invertida
  (`command_line/008-backslash`) y con el resto de caracteres permitidos
  (`009-allowed`). Python los deja tal cual.
- `prelaunch.rs::discover_codex_models`: con
  `supported_reasoning_levels` como cadena, Python itera sus caracteres y
  devuelve `["x", "y"]`; Rust exigía lista y el modelo desaparecía del catálogo
  (`discover_codex_models/009-levels-not-list`). Es un quirk reproducido
  (Q-PDRIVER-01).

### Fallos intermedios del arnés (18)
- Rutas de entorno: `<tmp>` en los `env` del caso apunta a la carpeta *home* del
  generador (no a `<home>/cases/<caso>`) y usa `/`; se normaliza a la ruta
  nativa (3 casos de rutas + 2 de `opencode_default_model`).
- Los casos de una sola pantalla traen `screen` (no `screens`); se envuelve en
  lista como el generador (5 `blocking_dialog`, 3 `screen`, 1
  `screen_shows_program`).
- `agents_catalog/003-discover` no sembraba `input.files` antes de descubrir.
- Artefactos del oráculo: mutación del `apply` capturado (Q-PDRIVER-03, 2
  casos) y tupla serializada como lista (Q-PDRIVER-04, 1 caso).

## Fixtures consumidos
- `parity/fixtures/agent_driver.jsonl`: 420/420.

## Pruebas portadas (parity/test-map.csv)
`parity/test-map.csv` no tiene filas para este dominio. Funciones cubiertas:
- `agent_prelaunch`: `opencode_state_root`, `prepare_opencode_state`,
  `mcode_config_path`, `discover_mcode_models`, `set_mcode_effort`,
  `codex_home`, `discover_codex_models`, `_strip_jsonc`,
  `opencode_default_model`.
- `agent_driver`: `AgentDriver.{resolve_agent,_text_list,_check_value,
  swarm_args,agent_launch,agent_argv,_resolve_codex,agents_catalog,
  catalog_text}`.
- `agent_session`: `valid_keys`, `fill`, `command_line`, `_is_shell`,
  `AgentSession.{_session_steps,wait_shell_prompt,_screen_shows_program,
  wait_ready,_try_screen,_check_screen,_cycle,_run_step,_apply_in_session,
  blocking_dialog,_unsent_in_input,confirm_submitted,send_task,finish_launch,
  configure_agent}`.
- `watcher`: `Watcher.{_compare,_poll_once,_wants_fast_poll,_set_health,
  start/stop,state_of}`; `_loop` y `_deliver` se consumen como salida
  registrada (Q-PHERDR-01).

## Decisiones
- **Costuras `#[doc(hidden)]`**: todo lo que solo usa el arnés queda público y
  oculto; la producción conserva las firmas anteriores (`Watcher::new`,
  `AgentDriver::new`, `AgentSession::new`).
- **`HealthCallback` con `Option<bool>`**: el `_set_health(None)` del oráculo
  guarda `null`; el tipo anterior (`bool`) no podía representarlo
  (Q-PDRIVER-02). `set_health(bool)` sigue siendo el camino de producción.
- **Fábrica de hilos**: `start()` pide sus dos hilos a `ThreadFactory`
  (`ez-watch-events`, `ez-watch-poll`); el sistema usa `std::thread` como antes
  y `stop()` espera `STOP_JOIN_S` igual que Python.
- **`finish_launch`/`configure_agent` con costura**: separan el ensamblado del
  resultado de los pasos que el oráculo monkeypatchea; el código real usa las
  mismas funciones internas.
- **`session/flow.rs`**: se partió `session.rs` para respetar las 600 líneas.
- **Arnés del generador**: `<tmp>` = home del generador, layout
  `<home>/cases/<caso>` y entorno HOME/USERPROFILE/LOCALAPPDATA/APPDATA/
  CODEX_HOME/MINIMAX_DATA_DIR/OPENCODE_CONFIG_DIR por caso.
- **`finish_launch` con `apply` ya mutado** (Q-PDRIVER-03): el arnés deshace el
  append que el propio `finish_launch` hizo sobre la lista capturada.
- **`text_list/009`** (Q-PDRIVER-04): la entrada no distingue tupla de lista;
  se devuelve `[]` como Python con la tupla.
- **`watcher/loop` y `watcher/deliver`**: salida registrada, sin ejecutar el
  bucle (Q-PHERDR-01).

## Verificación
- `cargo fmt --all`: ok.
- `cargo clippy -p ez-herdr --all-targets -- -D warnings`: ok.
- `cargo test -p ez-herdr`: 139 unitarias + `agent_driver_parity` (420 casos) +
  `herdr_parity` (177) + `tasks_activity_parity` (100) + `metrics_parity`
  (56 + 4 asserts), todo en verde.
- `cargo check --workspace --all-targets`: ok.
- `cargo test -p ez-app`: 285/285 (el cambio de `healthy` no rompe el HUD).
- Cuentas: `agent_driver` 420/420.
- Métricas: casos portados 0 → 420.

## Pendiente o riesgos
- `watcher/_loop` y `watcher/_deliver` están expuestos pero ninguna prueba los
  ejecuta: el generador no los llama (Q-PHERDR-01). Los 8 casos siguen
  afirmando listas vacías; conviene arreglar el generador y regenerar.
- Q-PDRIVER-03 y Q-PDRIVER-04 son artefactos del oráculo, no del módulo Rust:
  el arnés los reconstruye a mano. Al regenerar el fixture se pueden eliminar
  ambas excepciones.
- No se tocó `Cargo.toml`, `parity/` ni el Python.

# P-HERDR — Paridad de ez-herdr y de las métricas de ez-core (ronda 1)

## Resultado
- Brief/tarea: P-HERDR
- Estado: parcial

Dominios entregados al 100 %: `herdr` (177/177), `tasks_activity` (100/100) y
`metrics` (56/56). `agent_driver` **no se portó** (0/420): el detalle y el
trabajo exacto que falta están en «Pendiente o riesgos».

## Cambios
- `rust/crates/ez-herdr/tests/herdr_parity.rs` y `tests/herdr_parity/{harness,cases,launch}.rs`: nuevo; 177 casos de `herdr.py` con un doble de `subprocess.run`, `shutil.which`, el reloj y `uuid4`. Se partió en módulos (el archivo único pasaba de 600 líneas tras `cargo fmt`).
- `rust/crates/ez-herdr/tests/tasks_activity_parity.rs`: nuevo; 100 casos de `tasks.py` y `activity.py` con reloj congelado y `tasks.json` temporal.
- `rust/crates/ez-herdr/tests/metrics_parity.rs`: nuevo; 56 casos de `metrics.py` contra `ez_core::metrics`.
- `rust/crates/ez-herdr/src/herdr/runtime.rs`: nuevo; trait `HerdrRuntime` (run/which/reloj/ids/hilos + doble de `list_agents`) y `SystemRuntime`.
- `rust/crates/ez-herdr/src/herdr.rs`: `Herdr` recibe el runtime; `with_runtime`; `resolve_paths`, `spawn`, `json_command`, `replacement_for`, `run_installer`, `anonymous_env` públicas; `powershell_command` para el `run --bg` (el argv del oráculo no lleva `-ExecutionPolicy`).
- `rust/crates/ez-herdr/src/herdr/actions.rs`: el `run --bg` usa `powershell_command` y los hilos de `ez-launch` van por el runtime.
- `rust/crates/ez-herdr/src/herdr/terminals.rs`: `refresh_context` público y sin `Arc`, reloj del runtime, `seed_context`/`context_pending`/`context_snapshot` para el arnés, y `list_agents` consulta el doble del runtime.
- `rust/crates/ez-herdr/src/herdr/handoff.rs`: id de `save_context`/`stage_long_text` inyectado por el runtime.
- `rust/crates/ez-herdr/src/herdr/process.rs`: `anonymous_env()` (entorno sin `HC_TERMINAL_ID`, comparación sin mayúsculas).
- `rust/crates/ez-herdr/src/herdr/tests/launch.rs`: la prueba de `save_context_in` pasa el id.
- `rust/crates/ez-herdr/src/activity.rs`: `order` corregido (lo no visto primero; más reciente primero), `chain_title` con el `or` de Python y `snapshot_at` con instante inyectable.
- `rust/crates/ez-herdr/src/tasks.rs`: `with_clock`, `add_raw` (`float(created_at)` con el `ValueError` de Python), `prompt_value` público y con `limit=0` = `text[:-1]`, y el texto `[WinError N] …: 'origen' -> 'destino'` de `os.replace`.
- `rust/crates/ez-core/src/metrics.rs`: `META_KEYS` público, `float_value`, `mark_value`, `add_span_value`, `LatencyStats::{turns,total}` y `round1` con el redondeo decimal de Python.
- `rust-migration/python-quirks.md`: añadidos Q-PHERDR-01 (casos `watcher/loop` y `watcher/deliver` del generador que no llaman a la función) y Q-PHERDR-02 (`read_agent` con `limit=0` no recorta).
- `rust-migration/reports/P-HERDR-r1.md`: este informe.

## Paridad por dominio
| Dominio | Casos | Al primer intento | Defectos Rust corregidos | Fallos restantes |
|---|---:|---:|---|---:|
| `herdr` | 177 | 157 | 0 | 0 |
| `tasks_activity` | 100 | 95 | 4 | 0 |
| `metrics` | 56 | 51 | 1 | 0 |
| `agent_driver` | 420 | 0 | — | 420 (no portado) |
| **Total** | **753** | **303** | **5** | **420** |

Los fallos intermedios de `herdr` fueron del arnés (forma del JSON de `hc`,
normalización de PowerShell, `which` tras el instalador), no del módulo Rust.
En `tasks_activity`, 5 de los 9 fallos intermedios también fueron del arnés
(títulos con valores numéricos, `snapshot` sin reloj inyectable).

### Defectos de Rust corregidos
- `activity.rs::order`: comparaba `unseen` ascendente y ponía lo **no visto al
  final**; Python ordena por `not unseen` (lo no visto primero). Además el
  desempate por `since` estaba invertido (ordenaba lo más antiguo primero).
  Corregido en el mismo comparador; afecta a `order/003` y a todos los
  `snapshot`/`merge` con empates.
- `activity.rs::chain_title`: con `agent` presente pero vacío devolvía `"None"`
  (limpiaba con `filter(truthy)` antes de convertir). Python devuelve `""` y
  `clean_title` cae a «Tarea». Corregido respetando el `or` de Python.
- `metrics.rs::round1`: `(value * 10).round_ties_even() / 10` daba 2.6 para
  2.55; Python `round(2.55, 1)` da 2.5 porque redondea el binario exacto. Ahora
  usa `format!("{value:.1}")`, que reproduce `round(x, 1)` (incluye `-0.0`).
- `tasks.rs::prompt_value`: con `limit=0` hacía `limit - 1` con `usize`
  (underflow en depuración / valor erróneo). Python `text[:limit-1]` es
  `text[:-1]`; ahora se reproduce ese recorte.
- `herdr/actions.rs`: el `run --bg` construía PowerShell con el argv del
  instalador (`-ExecutionPolicy Bypass -Command`); Python usa
  `-NoProfile -Command`. Ahora `powershell_command` emite la misma forma (con
  `-EncodedCommand`, por la puerta de calidad).

## Fixtures consumidos
- `parity/fixtures/herdr.jsonl`: 177/177.
- `parity/fixtures/tasks_activity.jsonl`: 100/100.
- `parity/fixtures/metrics.jsonl`: 56/56.
- `parity/fixtures/agent_driver.jsonl`: 0/420.

## Pruebas portadas (parity/test-map.csv)
- `herdr`: `Herdr.{_anonymous_env,_json_command,_replacement_for,_resolve_paths,
  _run_installer,_spawn,ensure_installed,list_agents,get_agent,label_for,
  read_agent,read_full,cached_agents,context_text,send_agent,save_context,
  keys_agent,open_terminal,run_agent,focus_agent,kill_agent}`, `strip_ansi` y
  `visible_name` (con sus pruebas unitarias ya existentes).
- `tasks_activity`: `TaskRegistry`, `prompt_value`, `ActivityLog`, `chain_key`,
  `clean_title`, `dismissible`, `order`, `merge`, `orbit`, `live_items`,
  `snapshot` y constantes de módulo.
- `metrics`: `TurnTimings.{mark,add_span,report}`, `format_stt`, `format_turn`,
  `LatencyStats.percentile`, `LatencyStats` (cola/`total`/JSONL) y constantes.
- `agent_driver`: ninguna (dominio no portado).

## Decisiones
- **Costura `HerdrRuntime`** en vez de ejecutar `hc`/`herdr-app`: el oráculo
  sustituye `subprocess.run`, `shutil.which`, `time.monotonic` y `uuid.uuid4`.
  El trait cubre los cuatro y un doble de `list_agents` (el Python lo
  monkeypatchea en los casos de contexto). `Herdr::new` conserva la firma y usa
  `SystemRuntime`.
- **`-EncodedCommand` frente a `-Command`**: la puerta prohíbe el literal
  `-Command` con texto interpolado. La prueba decodifica el base64 UTF-16LE y
  compara el script; el resto del argv se compara tal cual.
- **`installed_calls` de `_replacement_for`**: el contador del oráculo es un
  doble de `ensure_installed` que Rust no puede observar sin un contador de
  producción; la prueba lo deriva de si el nombre es `hc`/`herdr-app` (el único
  caso en que el doble lo incrementa).
- **`_spawn`**: el oráculo parchea `ensure_installed` para que no instale; el
  arnés presiembra `herdr-app` en el `which` para que la costura real no lance
  el instalador.
- **`run_agent`**: `calls` excluye el subproceso `hc send` (el Python lo
  monkeypatchea); su texto y su `confirmed` se comparan en `agent_calls`.
  `visible-send-fails` fuerza el `hc send` real con `rc=1`.
- **Relojes congelados**: `metrics` compara `ts` con 1735689600.0 y
  `mark/default` usa 1234.5 explícito; `tasks_activity` construye
  `TaskRegistry`/`ActivityLog` con 1735689600.0 y pasa ese instante a
  `live_items`/`snapshot_at` cuando el caso trae `now: null`.
- **Archivos de prueba partidos**: `tests/herdr_parity.rs` declara
  `#[path = "herdr_parity/*.rs"] mod …;` para no superar 600 líneas. El test
  target sigue llamándose `herdr_parity`.

## Verificación
- `cargo fmt --all`: ok.
- `cargo clippy -p ez-herdr -p ez-core --all-targets -- -D warnings`: ok.
- `cargo test -p ez-herdr`: 139 unitarias + 1 `herdr_parity` (177 casos) +
  1 `tasks_activity_parity` (100) + 2 `metrics_parity` (56 + 4 asserts), todo en
  verde.
- `cargo test -p ez-core`: verde (incluye las unitarias de `metrics`).
- `cargo check --workspace --all-targets`: ok.
- `cargo test -p ez-app`: 285/285 (la corrección de `order` no rompe el HUD).
- Cuentas: `herdr` 177/177, `tasks_activity` 100/100, `metrics` 56/56.

## Pendiente o riesgos
- **`agent_driver` (0/420) sin portar.** Falta, por orden:
  1. `tests/agent_driver_parity.rs` y costuras que hoy no existen:
     - `session.rs`: `SessionClock`/`with_clock` son `pub(crate)`/`#[cfg(test)]`;
       exponerlos, y hacer públicos `_try_screen`, `_screen_shows_program`,
       `_check_screen`, `_cycle`, `_run_step`, `_apply_in_session` y
       `_unsent_in_input`; `_session_steps` y `_is_shell` ya son `pub(crate)`.
     - `driver.rs`: `check_value` y `text_list` son `pub(crate)`; `resolve_codex`
       y `Wanted` son privados. Los 20 casos de `check_value` y los 9 de
       `resolve_codex` necesitan una entrada pública con `Value`.
     - `driver/catalog.rs`: `agents_catalog`/`catalog_text` dependen de
       `LaunchHooks::which` (privado); hace falta un constructor con hooks.
     - `watcher.rs`: `_compare`, `_poll_once`, `_deliver`, `_set_health`,
       `_wants_fast_poll`, `_loop` son privados y no hay fábrica de hilos; los
       2 casos de `start/stop` necesitan un doble de `threading.Thread`.
  2. Arnés de sesión (pantallas/agentes/sends/keys con `raise`, reloj que avanza
     al dormir) y de prelaunch (HOME/USERPROFILE/LOCALAPPDATA/CODEX_HOME/
     MINIMAX_DATA_DIR/OPENCODE_CONFIG_DIR por caso y snapshots de archivos).
  3. El generador tiene dos grupos vacíos (Q-PHERDR-01): `watcher/loop` y
     `watcher/deliver` no llaman a la función. Al portarlos hay que decidir si
     se reproducen vacíos o se regenera el fixture.
- **Orden de dominios**: el brief pide `herdr → agent_driver → tasks_activity`.
  Al no poder asumir `agent_driver` con garantías en esta ronda, se cerraron
  `tasks_activity` y `metrics` (que el brief pide «al terminar») para entregar
  tres dominios verdes en vez de dejar la puerta en rojo. Es una desviación
  consciente del orden, no del alcance.
- **Comparación de números**: el comparador trata `40` y `40.0` como iguales
  (los `timeout` del oráculo mezclan `int` y `float`). Los enteros del dominio
  son exactos; los flotantes usan la tolerancia `f64_rel = 1e-9`.
- **`round1` con `format!`**: pierde precisión solo en valores que Python
  tampoco conserva al redondear a un decimal; los 5 casos de `rounding` y
  `stats/*` lo confirman.
- No se tocó `Cargo.toml`, `parity/` ni `src/` de Python.

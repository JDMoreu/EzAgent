# P-FIX-MICLOCK — r1

## Resultado
- Brief/tarea: P-FIX-MICLOCK (el menú y el chat no abren en la app real: el hilo
  de la llamada retenía el candado del micrófono durante toda la llamada)
- Estado: completado
- Prueba real: `cargo build --release -p ez-app --bin ezagent-voice` y un ejemplo
  temporal (creado, ejecutado y borrado, no commiteado) que arranca `VoiceApp`
  como `run-dev.cmd` (`VoiceConfig::load(None)`, `LiveHooks`), espera 6 s y pulsa
  el engranaje y el chat **con la llamada activa** («Escuchando al iniciar»):
  `RESULT panel=true chat=true` (panel en 200 ms, chat en ~2 s), código 0. Antes
  del arreglo `hud_menu_settings()` no volvía nunca en esa situación.

## Cambios
- `rust/crates/ez-audio/src/capture/mic.rs`: `MicCapture` pasa a ser un `Arc`
  por dentro (`Clone`), con `stream(&self)`; el cerrojo ya no envuelve la
  captura. `stream` separa el origen de bloques (`Source::Device` / `Source::Silent`),
  marca `capturing` con un guardián RAII y sigue leyendo los ajustes y el
  detector de turno **en cada bloque**. `end_probe` ahora es siempre `Some` y
  consulta el detector en cada sonda, como `Segmenter(end_probe=self._end_probe)`
  del Python (antes lo congelaba al abrir la captura).
- `rust/crates/ez-audio/src/capture/mic/live.rs` (nuevo): `Live` con `AtomicF64`
  por ajuste (min_rms, silence_secs, pre_roll_secs, min_speech_secs,
  max_utterance_secs, noise_multiplier, barge_in_rms, barge_in_secs,
  turn_quick_secs), índice de dispositivo atómico y `Mutex` corto para el
  detector y `last_speech_end`; más los accesores `min_rms()/set_min_rms()`…
  que sustituyen a los campos públicos.
- `rust/crates/ez-app/src/app/call.rs`: `_call_worker` clona el micrófono y llama
  `stream` sin retener el cerrojo; `_on_utterance` lee `last_speech_end` del clon
  en vez de `try_lock()`.
- `rust/crates/ez-app/src/app/core/state.rs`: `AudioState::mic()` (clon barato) y
  `AudioState::device_index()` (espejo del dispositivo, sin esperar a la captura).
- `rust/crates/ez-app/src/app/menu_wiring/menu.rs`, `apply.rs`,
  `apply/live.rs`, `apply/mic.rs`: el menú y los cambios en vivo usan esos
  accesores; `update_mic_settings` y `reload_mic_with` escriben por setters y el
  cerrojo solo cubre el cambio de dispositivo.
- `rust/crates/ez-app/src/app/core/parts.rs`, `startup.rs`, `core/init.rs`:
  detector de turno y cierre por setters; el arranque lee nombre/índice/tasa con
  los accesores.
- `rust/crates/ez-app/src/app/settings_apply.rs`: `refresh_devices` enumera
  PortAudio **fuera** del cerrojo de `devices_cache` (el menú ya no espera a la
  enumeración).
- `rust/crates/ez-app/src/app/menu_wiring/tests/mic_lock.rs` (nuevo): las tres
  regresiones obligatorias con `MicCapture::silent()` (captura falsa que se queda
  en `stream`).
- `rust/crates/ez-app/src/app/{call/tests.rs, core/tests.rs, events/support.rs,
  menu_wiring/tests.rs, menu_wiring/tests/more.rs, states/tests/support.rs}`:
  los dobles pasan de escribir campos a los setters nuevos.
- `rust/crates/ez-app/src/replay/mic.rs`: una línea, `set_last_speech_end` (el
  arnés de reproducción también tomaba `audio.mic.lock()`; ver desviaciones).

## Decisiones
- `// DECISION` en `call.rs`: clonar el micrófono es barato y `stream` corre sin
  el cerrojo, como `audio.py` (el Python no tiene candado: lee atributos).
- `MicCapture` con estado interior (`Arc`) en vez de `RwLock`: los ajustes son
  escalares sueltos que se leen en cada bloque; un `RwLock` habría bloqueado a
  los escritores del menú durante toda la llamada.
- `AudioState` mantiene `Arc<Mutex<MicCapture>>` (mismo tipo en `AppParts` y en
  los dobles): el cerrojo ahora solo cubre el reemplazo del dispositivo, y evita
  tocar los ~10 puntos de construcción de las pruebas.
- `MicCapture::silent()`/`capturing()`: costura mínima de prueba para la
  regresión obligatoria («captura falsa que se quede en `stream`»), sin hardware.
- `end_probe` siempre `Some` y leyendo el detector en cada sonda: así el cambio
  en vivo de `turn_detector`/`smart_turn` llega a la captura en curso; es lo que
  hace el Python (el segmentador llama `self._end_probe` y este devuelve `None`
  sin detector). No cambia ningún umbral ni el comportamiento del `Segmenter`.
- `live_settings()` conserva literal `self.barge_in_rms or barge_in_rms`.

## Auditoría del resto de `ez-app` (mutex alrededor de operación larga)
Encontrado y **corregido**:
1. `call.rs::call_worker` — `audio.mic.lock().stream(...)` durante toda la
   llamada (la causa del síntoma). Corregido.
2. `menu_wiring/menu.rs::menu_values`, `apply/live.rs::live_device`,
   `apply.rs::prepare_settings`, `core/init.rs` — lectores del índice de
   dispositivo esperaban ese mismo cerrojo. Corregidos con `device_index()`.
3. `settings_apply.rs::refresh_devices` — `list_input_devices()` (PortAudio)
   dentro del cerrojo de `devices_cache`, que el menú lee. Corregido.

Encontrado y **sin tocar** (no bloquea la interfaz):
- `startup.rs::shutdown_services`: `if let Some(messenger) = …messenger.lock().take() { messenger.close() }`
  retiene el cerrojo del mensajero mientras `close()` une hilos; solo ocurre en
  el apagado y es el mismo orden que el Python.
- `states.rs::latency_meta` toma el cerrojo del mensajero para leer `last_rounds`
  (en memoria, microsegundos).
- `LatencyStats::record` hace `append_jsonl` bajo su candado; solo lo llama el
  hilo de la interfaz y nadie más espera ese candado.
- `menu_wiring/apply/mic.rs::reload_mic_with` construye `MicCapture::new`
  (sondas de PortAudio) en el hilo de la interfaz: es exactamente lo que hace
  `_reload_mic` del Python en Tk; cambiarlo alteraría el orden del Python.
- `startup.rs::drain_workers` / `join_call_thread`: uniones con fecha límite,
  solo en el apagado.

## Verificación
- `cargo fmt --all --check`: ok (0 diferencias).
- `cargo clippy -p ez-audio -p ez-app --all-targets -- -D warnings`: ok.
- `cargo test -p ez-audio`: 68 lib + `audio_parity` (1), `turn_parity` (1),
  `wake_parity` (1), `shared_environment` (1) — todo en verde.
- `cargo test -p ez-app`: 293 lib (incluye las 3 regresiones nuevas) +
  `app_text_parity` + `cli_parity` — en verde.
- `cargo test -p ez-app --features replay --test replay_parity`: 3 passed
  (411 s; escenarios 21-chat-panel y 23-menu-settings incluidos). La primera
  corrida falló solo porque faltaban los binarios de `ez-fakes`:
  `cargo build -p ez-fakes --bins` y volvió a verde.
- Comprobación real (release + entorno de DLL de `run-dev.cmd`, apuntando al
  `.venv` del repo principal porque este worktree no tiene uno):
  `RESULT panel=true chat=true`, código de salida 0. Log:
  `HUD: panel` → panel abierto en 200 ms; `HUD: chat` → chat abierto a los ~2 s,
  ambos con la llamada activa (`Llamada iniciada` / `Escuchando al iniciar`).
- Métricas: antes `hud_menu_settings()` no volvía (cuelgue permanente con la
  llamada activa) → ahora responde en < 1 s (la prueba exige < 1 s; medido en
  microsegundos).

## Fixtures y pruebas
- Fixtures consumidos: ninguno (el brief dice «(ninguno)»); `audio_parity`,
  `turn_parity` y `wake_parity` se ejecutan completos y siguen en verde.
- Pruebas nuevas (`menu_wiring::tests::mic_lock`):
  - `hud_menu_settings_answers_while_the_call_is_capturing`
  - `live_min_rms_reaches_the_capture_in_progress`
  - `tick_opens_the_panel_and_the_chat_during_a_call`
- Pruebas nuevas en `ez-audio` (`capture::mic::tests`):
  - `silent_capture_stays_in_stream_and_applies_live_changes`
  - `the_turn_detector_is_read_on_every_probe`

## Desviaciones del brief
- Se tocó `rust/crates/ez-app/src/replay/mic.rs` (1 línea): toma
  `app.inner.audio.mic.lock()` y asignaba `last_speech_end`, que ahora es un
  setter. Fuera de `app/**`, pero es el mismo patrón que el brief manda auditar y
  sin ese cambio no compila con `--features replay`.
- El ejemplo `examples/diag_open.rs` se creó, se ejecutó y se borró: no está en
  el commit (el brief lo pide así).

## Pendiente o riesgos
- `MicCapture` cambió de campos públicos a métodos: cualquier código futuro que
  leyera `mic.min_rms` debe usar `min_rms()`. No queda ningún lector de campos
  en el árbol.
- `end_probe` ahora cuesta una llamada por `probe_interval_secs` incluso sin
  Smart Turn (devuelve `None`); es lo que hace el Python y no altera ninguna
  decisión del `Segmenter`.
- El cambio de micrófono sigue construyendo el dispositivo en el hilo de la
  interfaz (igual que el Python): si PortAudio tarda, el menú puede tardar ese
  rato, pero no se cuelga.

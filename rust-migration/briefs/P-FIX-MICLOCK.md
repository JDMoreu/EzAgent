# P-FIX-MICLOCK — El menú y el chat no abren en la app real: el hilo de la llamada retiene el candado del micrófono durante toda la llamada
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- src/ezagent/voice/app.py: `_call_worker` (1492-1506, `self.mic.stream(...)`), `_hud_menu_settings`, los cambios en vivo del menú que tocan `self.mic.*` (`min_rms`, `silence_secs`, `noise_multiplier`, `pre_roll_secs`, `min_speech_secs`, `max_utterance_secs`, `barge_in_rms`, `barge_in_secs`, `device_index`, `turn_detector`, `turn_quick_secs`) y el cambio de micrófono
- src/ezagent/voice/audio.py: `MicCapture.stream` lee esos atributos **en cada bloque**, así que un cambio en vivo se aplica a la captura en curso sin bloquear a nadie
- Rust: `rust/crates/ez-app/src/app/call.rs` (~142: `self.inner.audio.mic.lock().stream(...)` mantiene el `Mutex` del micrófono **toda la llamada**), `app/menu_wiring/menu.rs::menu_values` (~84: `self.inner.audio.mic.lock().device_index`), `app/menu_wiring/apply/live.rs::live_float` (~150), `app/menu_wiring/apply/mic.rs` (cambio de micrófono y `update_mic_settings`), `app/core/parts.rs` (`create_turn_detector`, `apply_turn_settings`), `app/startup.rs` (cierre); `rust/crates/ez-audio/src/capture*` (`MicCapture`, `stream`, `VadSettings`, `CapturePipeline`)

## Archivos que se tocan (solo estos)
- rust/crates/ez-audio/src/capture.rs y rust/crates/ez-audio/src/capture/**
- rust/crates/ez-app/src/app/call.rs, rust/crates/ez-app/src/app/core/*.rs, rust/crates/ez-app/src/app/menu_wiring/**, rust/crates/ez-app/src/app/startup.rs y cualquier otro archivo de `rust/crates/ez-app/src/app/**` que tome `audio.mic.lock()`
- pruebas nuevas en esos crates
- rust-migration/reports/P-FIX-MICLOCK-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- (ninguno)

## Notas específicas
- **Síntoma real (reportado por el usuario):** con `rust/run-dev.cmd`, al pulsar el engranaje o el botón de chat de la barra, la consola muestra `HUD: panel` / `HUD: chat` pero no se abre nada. **Diagnóstico ya hecho por el orquestador:** un ejecutable de diagnóstico que arranca `VoiceApp` igual que `run-dev.cmd` (la app arranca con la llamada activa, «Escuchando al iniciar») se queda **colgado para siempre** en `VoiceApp::hud_menu_settings()`. Causa: `menu_values` hace `self.inner.audio.mic.lock()` y ese `Mutex` lo retiene el hilo de la llamada dentro de `MicCapture::stream` mientras dura la llamada. Como `toggle_menu` corre en el hilo de la interfaz (`_tick`), el bucle `_tick` queda bloqueado y por eso tampoco se procesa el `HUD: chat` que llega después. En la demo y en las pruebas no hay llamada activa, por eso allí funcionaba.
- **Qué hacer:** que **ningún** lector o escritor de ajustes del micrófono espere a que termine la llamada, con la misma semántica que el Python: (1) leer el índice/nombre del dispositivo sin tomar el candado de la captura (espejo atómico o estado compartido separado); (2) los cambios en vivo de VAD y Smart Turn (`min_rms`, `silence_secs`, `noise_multiplier`, `pre_roll_secs`, `min_speech_secs`, `max_utterance_secs`, `barge_in_rms`, `barge_in_secs`, `turn_detector`, `turn_quick_secs`) deben aplicarse **a la captura en curso** sin bloquear (p. ej. `VadSettings` compartidos con `Arc<Mutex<…>>`/atómicos que `stream` relee en cada bloque, como `audio.py`; o un canal de cambios que `stream` drena por bloque); (3) el cambio de micrófono durante una llamada debe seguir el orden del Python (para la captura, cambia, la reanuda) sin abrazo mortal. Diseña la costura en `ez-audio` lo más pequeña posible y no cambies el comportamiento de captura (las pruebas de paridad `audio_parity`, `turn_parity` y `wake_parity` deben seguir en verde).
- **Audita** el resto de `ez-app` en busca del mismo patrón: cualquier `Mutex` tomado alrededor de una operación larga (captura, red, `join` de hilos, reproducción de voz) que otro hilo —sobre todo el de la interfaz— pueda pedir. Lista lo encontrado en el informe y corrige lo que bloquee la interfaz.
- **Pruebas de regresión obligatorias:** con una captura falsa que se quede en `stream` (sin micrófono real), `hud_menu_settings()` vuelve en < 1 s y devuelve `Some`; un cambio en vivo de `min_rms` llega a la captura en curso; `toggle_menu` y `toggle_chat` se procesan en `tick` mientras la llamada está activa. **Comprobación real:** compila `cargo build --release -p ez-app --bin ezagent-voice` y escribe un ejemplo temporal (no lo commitees) como `rust/crates/ez-app/examples/diag_open.rs` que arranque `VoiceApp` con la configuración real (`VoiceConfig::load(None)`, `LiveHooks`), espere 6 s, llame a `on_hud_action("panel")` y a `on_hud_action("chat")` y muestre `hud().menu_open()` y `hud().chat_is_open()`; ejecútalo con las variables de DLL de `rust/run-dev.cmd` y pon el resultado en el informe (ambos deben quedar `true`).
- `cargo fmt`, `cargo clippy -p ez-audio -p ez-app --all-targets -- -D warnings` y `cargo test -p ez-audio -p ez-app` en verde; `cargo test -p ez-app --features replay --test replay_parity` también debe seguir en verde (tarda ~7 min).

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/P-FIX-MICLOCK-r1.md` y un commit en tu rama.

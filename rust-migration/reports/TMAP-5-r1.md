# TMAP-5 — Mapa de pruebas Python → Rust, grupo 5

## Resultado
- Brief/tarea: TMAP-5
- Estado: completado
- 314 filas en `parity/test-map/TMAP-5.csv` (todas las del grupo, ninguna `pendiente`).
  Conteo por estado: **portada 198 · nueva 81 · paridad 32 · no_aplica 3**.

## Cambios
- `parity/test-map/TMAP-5.csv`: 314 filas `python_test_id,rust_crate,rust_test_path,estado,nota`
  (una por prueba del grupo; las parametrizadas comparten la prueba Rust que cubre la conducta).
- `rust/crates/ez-chat/src/text_input.rs`: 3 pruebas nuevas (retroceso/suprimir en los
  extremos, selección con Mayús, Enter/set_text) y la de selección ampliada con el borrado.
- `rust/crates/ez-llm/src/tool_markup.rs`: 2 pruebas nuevas (marcador partido en trozos de
  1/2/3/5/8 y texto del aviso) y 2 ampliadas (texto normal con `<`, prefijo falso `<｜`).
- `rust/crates/ez-llm/src/messenger/tool_loop/tests/text.rs`: 1 prueba nueva (sin runtime el
  bloque de modo chat no se queda pegado al turno de voz siguiente).
- `rust/crates/ez-audio/src/wake.rs`: ampliadas las frases que despiertan (easy agent / Ez Agent
  / isi agent) y la comprobación de los límites literales 0,35–3,0 s.
- `rust/crates/ez-hud/src/motion.rs`: 2 pruebas nuevas (trayectoria independiente de los fps
  a 30/60/144 y cambio de objetivo sin reiniciar posición ni velocidad).
- `rust/crates/ez-hud/src/model/settings/tests/more.rs`: 1 prueba nueva (las 14 claves
  instantáneas activan Guardar, guardar limpia, restaurar vuelve a dejarlo pendiente; también
  selectores, textos y atajos).
- `rust/crates/ez-hud/src/win/chat/window/tests.rs`: 6 pruebas nuevas (entrada comida mientras
  anima, hover del borde, estado/orbe según pose y chat ocupado, la forma se oculta al primer
  cuadro pintado, 60/30 fps, open/reveal reenviados) y un helper de panel con vista.
- `rust/crates/ez-hud/src/win/chat/window/lifecycle.rs`: 1 prueba nueva (`window_rect` y
  `form_around`: la ventana es el panel más el margen de sombra).
- `rust/crates/ez-hud/src/win/drop/ui/tests.rs`: 2 pruebas nuevas (`set_activity` guarda la
  foto que ve la vista rápida; `toggle_chat` sin panel instalado no falla) y la del chat
  ampliada con `form_size`/`set_chat_open`/`set_form_hidden`.
- `rust/crates/ez-app/src/app/chat/tests.rs`: 3 pruebas nuevas (el texto responde a una
  confirmación pendiente, el chat con voz sintetiza la respuesta, y con tabla solo se dice
  «El detalle está en el chat.»).
- `rust/crates/ez-app/src/app/events/tests.rs`: 2 pruebas nuevas (micrófono del chat dentro y
  fuera de llamada; el atajo del chat devuelve la barra oculta).
- `rust/crates/ez-app/src/app/core/tests.rs`: 1 prueba nueva (HOTKEY_KEYS/INSTANT_SETTINGS).
- `rust/crates/ez-app/src/app/menu_wiring/tests.rs`: 2 pruebas nuevas (cambio en vivo sin
  escribir hasta Guardar; `stt_dictionary` llega a la config).
- `rust/crates/ez-app/src/app/activity/tests.rs`: 1 prueba nueva (tarea guardada sin terminal:
  `forget_gone` la olvida sin avisar).
- `rust/crates/ez-swarm/src/service/tests.rs`: 1 prueba nueva (el primer miembro nunca abre
  una etapa paralela).
- `rust-migration/reports/TMAP-5-r1.md`: este informe.

No se tocó código de producción ni ningún `Cargo.toml`.

## Decisiones
- `rust_test_path` sigue el formato literal del brief: `rust/crates/<crate>/…::<nombre>`.
  Para los casos de paridad se cita `parity/fixtures/<archivo>.jsonl#<función>` y la prueba que
  los consume.
- Una fila por prueba Python; las parametrizadas (esfuerzo de razonamiento, spring por fps,
  panel en pantalla, claves de Guardar, frases de wake, marcador partido) apuntan todas a la
  misma prueba Rust que recorre los mismos casos.
- No hizo falta ningún `#[ignore]`: las pruebas nuevas comprueban conducta ya portada y pasan.
- Pruebas de plataforma: `ChatPanel::toggle`, el arrastre/redimensionado con el ratón y la
  creación de la ventana nativa no se pueden simular sin tocar producción (leen el cursor real
  o crean una ventana Win32); su matemática está en `ez-hud::geometry` y en los fixtures, y su
  parte de estado en pruebas de módulo. Queda anotado en las notas del CSV.

## Verificación
- `cargo fmt --all`: limpio.
- `cargo clippy -p ez-chat -p ez-llm -p ez-audio -p ez-hud -p ez-app -p ez-swarm --all-targets -- -D warnings`: limpio.
- `cargo test` (desde `rust/`):
  - ez-chat: 82 passed
  - ez-llm: 108 passed (+ paridad de fixtures)
  - ez-audio: 107 passed
  - ez-hud: 524 passed
  - ez-app: 345 passed
  - ez-swarm: 121 passed
  - 0 fallos, 0 ignoradas.
- Fixtures consumidos por las filas `paridad` (169 casos):
  - `parity/fixtures/config.jsonl`: 3 (`VoiceConfig.load` 0004-0005, `VoiceConfig.save` 0002 →
    migración `notch_hidden`).
  - `parity/fixtures/hud_geometry.jsonl`: 42 (`chat_geometry.panel_rect` 8, `panel_size` 14,
    `resize_rect` 20).
  - `parity/fixtures/hud_chat.jsonl`: 30 (`ChatPanel._compose` 12, `_elapsed_for` 6,
    `_duration` 2, `chat_paint.row_reveal` 10).
  - `parity/fixtures/llm_tools.jsonl`: 44 (`turn` 32, `max_tokens` 8, `prompt_parts` 4).
  - `parity/fixtures/hud_bar.jsonl`: 50 (`orbit_tile`).
- Pruebas portadas: las 314 filas están en `parity/test-map/TMAP-5.csv`; 28 pruebas Rust
  nuevas y 6 ampliadas (ver «Cambios»).

## Hallazgos y límites
1. `_apply_live_setting("llm_saved_profile")` (test_chat_app
   `test_selecting_a_saved_model_uses_its_provider_and_connection`) no se puede ejercitar:
   `apply_live_profile` llama a `load_provider_catalog(None, None)`, que lee el `llm.json` real
   de la carpeta de la app y no admite inyección. La parte de conducta (perfil guardado →
   proveedor/modelo/base_url/clave/estilo) sí está cubierta por
   `ez-hud::settings::saved_configurations_restore_the_connection_fields`; la fila apunta ahí.
2. Ninguna prueba nueva falló por diferencia con Python: no se dejó ninguna `#[ignore]`.
3. `orbit_tile` no cachea en Rust (Python sí); es una diferencia de implementación sin cambio
   de conducta visible, anotada en la fila.
4. `Messenger::pending_notify_address`: la rama `except → None` de Python no tiene equivalente
   (el trait `Runtime` devuelve `Option<String>` y no puede fallar).
5. `wake::utterance_secs` recibe `u32`: `utterance_secs(…, -1) == 0.0` de Python no es
   representable; `normalize(None)` tampoco aplica a `&str`. Ambas anotadas en el CSV.
6. El arrastre/redimensionado del panel y `ChatPanel::toggle` de extremo a extremo dependen del
   cursor real y de la ventana nativa; quedan cubiertos por geometría/fixtures y por pruebas de
   estado, no por una prueba de ratón simulada.
7. `test_a_window_that_cannot_start_leaves_everything_clean` y
   `test_clicks_still_land_after_moving_the_window` quedan `no_aplica` (doble `FakeWindow` no
   inyectable y coordenadas de cliente ya relativas a la ventana, respectivamente).

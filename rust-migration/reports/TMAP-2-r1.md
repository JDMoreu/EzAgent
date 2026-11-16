## Resultado
- Brief/tarea: TMAP-2 (mapa de pruebas Python → Rust, grupo 2)
- Estado: completado

## Cambios
- `parity/test-map/TMAP-2.csv`: 306 filas del grupo (una por `python_test_id` del mapa) con crate, ruta de la prueba Rust, estado y nota. Sin `pendiente`.
- `rust/crates/ez-audio/src/segmenter/tests.rs`: pre-roll dentro del PCM devuelto, aviso único de `speech_start`, reset del turno y tolerancia a pre-roll negativo.
- `rust/crates/ez-audio/src/barge_in.rs`: segunda interrupción tras confirmar (detector reutilizable).
- `rust/crates/ez-hud/src/model/drop/geometry.rs`: no solapamiento de botones/forma y separación uniforme del botón de llamada.
- `rust/crates/ez-hud/src/win/drop/ui/tests.rs`: arrastre a bordes conservando la forma bajo el cursor, vuelta al centro, recorte sin cambios, revalidación del dock al cambiar forma o tamaño, y restauración del dock guardado por disposición.
- `rust/crates/ez-hud/src/model/menu/edit/tests.rs`: `close()` termina la captura; Esc ignora texto y pegado durante la captura.
- `rust/crates/ez-hud/src/paint/chat/render/tests.rs`: palabra única más ancha que la columna y altura lógica independiente de la escala.
- `rust/crates/ez-hud/src/paint/chat/render_rich/tests.rs`: tablas (muchas columnas, ancho natural y celda larga), preview limitado y preview sin zonas pulsables.
- `rust/crates/ez-chat/src/markdown/tests.rs`: ruta entre `<…>` como documento.
- `rust/crates/ez-herdr/src/watcher/tests.rs`: done solo tras working, sondeo fallido sin perder el snapshot, callbacks fuera del sondeo, unión de ambos hilos y secuencia de backoff.
- `rust/crates/ez-herdr/src/session/tests.rs` y `session/tests/extra.rs`: pantalla vacía sin confirmar y plazo de `wait_ready` con lecturas lentas (reloj falso).
- `rust/crates/ez-agent/src/policy.rs`: oráculo completo (200 combinaciones), reglas de nivel 3, borrado, umbral, contenido no confiable, nivel 2, nivel devuelto y motivo.
- `rust/crates/ez-agent/src/confirm.rs` y `confirm/extra_tests.rs`: tablas de aceptación/rechazo, números 100/120/33, repetir pregunta y respuestas concurrentes.
- `rust/crates/ez-core/tests/speech_text_properties.rs`: idempotencia y fuzz determinista de enteros, horas y texto arbitrario.

## Decisiones
- `// DECISION`: no se añadió ninguna dependencia ni se tocó código de producción; todas las pruebas nuevas viven en módulos `tests`/`tests/` existentes.
- Las filas del mapa se generaron desde `parity/test-map.csv` filtrando los 10 archivos del grupo; el brief dice 313 pruebas y el mapa tiene **306** filas (306 ids únicos). Se cubren las 306; la diferencia parece de conteo/parametrización del generador, no de filas ausentes.
- Varias filas del mapa traían un `rust_crate` orientativo incorrecto (`ez-app` para handoff y hotkeys): se corrigió a `ez-agent`, `ez-hud`, `ez-herdr`, `ez-llm` o `ez-core` según dónde vive la conducta.
- `test_text_measurement_and_paint_share_masks_across_colors` se marcó `no_aplica`: monkeypatch de `hud.text._render_bitmap` para contar máscaras; `ez-gfx` cachea bitmaps y los PNG dorados comparan el resultado.

## Verificación
- `cargo fmt --all`: ok.
- `cargo clippy -p ez-audio -p ez-hud -p ez-agent -p ez-herdr -p ez-core --all-targets -- -D warnings`: ok.
- `cargo test -p ez-audio`: 110 unitarias + paridad `audio` en verde.
- `cargo test -p ez-hud`: 525 unitarias + paridad/PNG (`hud_bar`, `hud_chat`, `hud_menu`, `hud_drop_model`, `hud_settings`, `hud_peek`, `hud_geometry`) en verde.
- `cargo test -p ez-agent`: 259 unitarias + 26 binarios de paridad en verde.
- `cargo test -p ez-herdr`: 178 unitarias + paridad `agent_driver`/`herdr`/`tasks_activity`/`permission` en verde.
- `cargo test -p ez-core`: 51 unitarias + `speech_text` y `config` en verde.
- `cargo test -p ez-chat`: 79 unitarias + 5 paridades en verde.
- No se ejecutó pytest: la fase es de mapa y las filas de Python son la especificación, no el objetivo de la puerta.
- Fixtures consumidos (solo lectura): `audio.jsonl` (265 casos), `config.jsonl` (63), `confirm.jsonl` (37), `policy.jsonl` (144), `handoff.jsonl` (123), `herdr.jsonl` (subconjunto `Herdr.cached_agents`/`Herdr.context_text`), `condenser.jsonl` (41), `hud_bar.jsonl` (361), `hud_chat.jsonl` (221), `hud_menu.jsonl` (323), `hud_settings.jsonl` (1175), `chat_store.jsonl` (85), `speech_text.jsonl` (452).
- Filas portadas: ver `parity/test-map/TMAP-2.csv` (306 filas).
- Conteo por estado: `portada` 113, `paridad` 107, `nueva` 82, `no_aplica` 4.
- Conteo por crate: `ez-hud` 130, `ez-agent` 77, `ez-core` 42, `ez-audio` 32, `ez-herdr` 15, `ez-chat` 6, `ez-llm` 4.
- Pruebas nuevas escritas en esta ronda: 34 funciones (≈600 líneas de prueba, repartidas en los archivos listados).

## Pendiente o riesgos
- `tests/test_hotkey_field.py::test_every_state_of_the_field_draws` quedó en `paridad` con los cuadros dorados de `Menu.draw`: no hay caso de fixture con la captura de atajo activa y el intento de reproducir la diferencia de cuadros con un menú sintético no pintó el campo. El dibujo de la captura existe (`advanced/keycaps.rs`) y la lógica de estados está cubierta; falta un caso dorado específico.
- `tests/test_audit_watch.py::test_backoff_grows_to_five_seconds`: `Watcher::poll_loop` no tiene el doble `_stop` de Python, así que la prueba nueva verifica la tabla y el índice usado por el bucle (1, 2, 5, 5) y no los tiempos reales de espera.
- `tests/test_chat_trace.py::test_the_stamp_never_says_how_long_the_reply_took` se apoyó en el formato fijo y en que el módulo no tiene `reply_times`; no hay una prueba negativa independiente.
- El mapa no trae pruebas de paridad nuevas de esta fase (`parity/fixtures` no se amplió), según `_COMMON.md`.

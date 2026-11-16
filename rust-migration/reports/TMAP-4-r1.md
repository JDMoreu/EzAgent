# TMAP-4 — Mapa de pruebas Python → Rust, grupo 4

## Resultado
- Brief/tarea: TMAP-4
- Estado: completado

## Cambios
- `parity/test-map/TMAP-4.csv`: 314 filas (una por prueba del grupo), sin `pendiente`; referencias de archivo/prueba verificadas por script antes de cerrar.
- `rust/crates/ez-core/tests/tmap_speech_money.rs`: casos exactos de precios, importes, tablas, signos, teléfonos/fechas, unidades y frases normales de `test_speech_money.py`.
- `rust/crates/ez-stt/tests/tmap_stt_cases.rs`: literales del diccionario, reglas inválidas/límites, alucinaciones exactas y pistas de vocabulario.
- `rust/crates/ez-stt/src/client/tests.rs`: pista por defecto de Whisper, errores 400/500/503 en bucle, `whisper_home` con entorno.
- `rust/crates/ez-hud/src/win/peek/tests/tmap_group4.rs`: sin actividad no abre, menú/chat cierran, revelar por voz, escala de la barra, toast caducado y operaciones de ventana desde otro hilo.
- `rust/crates/ez-hud/src/win/peek/tests.rs`: se declara el submódulo anterior.
- `rust/crates/ez-hud/src/paint/peek/tests.rs`: tiempos exactos de la línea de estado, cápsula de una fila, centrado de las dos líneas y la × solo en la fila del puntero.
- `rust/crates/ez-hud/src/win/frame_stats.rs`: informe «Cuadros drop: 4» con un cuadro lento.
- `rust/crates/ez-hud/src/model/menu/sliders/tests.rs`: escala completa del medidor en dB, extremos, pulgar bajo el puntero, uniformidad y umbral por defecto.
- `rust/crates/ez-hud/src/model/drop/model.rs`: ancho del hub con botones y micro plegado.
- `rust/crates/ez-hud/src/model/chat/view/tests.rs`: cambiar de paleta vacía la caché del chat.
- `rust/crates/ez-hud/tests/tmap_theme_accent.rs`: presets legibles en claro/oscuro, botón tintado y órbita siguiendo la paleta.
- `rust/crates/ez-llm/src/context.rs`: umbrales 70 %/45 %, corte sin separar llamada y resultado, transcripción con herramientas y final de un historial enorme, mensajes de resumen y crecimiento de la estimación.
- `rust/crates/ez-llm/src/messenger/tool_loop/tests/budgets.rs`: presupuesto de cuatro llamadas, lote que excede el cupo, llamada truncada, caché canónica de lecturas, escrituras no cacheadas, proveedor que ignora la finalización y recuperación de JSON con la nota de cupo.
- `rust/crates/ez-llm/src/messenger/tool_loop/tests/waits.rs`: datos volátiles por turno, compactación completa/repetida/fallida/ventana grande, turno enorme recortado y avisos de espera (una vez, sin aviso si suelta antes, silencio al cancelar, cancelación con herramienta).
- `rust/crates/ez-llm/src/messenger/tool_loop/tests/support.rs`: doble de pruebas con `budget` (cupo vivo) y `execute_gate` (herramienta lenta); sin cambios de producción.
- `rust/crates/ez-llm/src/messenger/tool_loop/tests.rs`: declara los dos submódulos nuevos.
- `rust/crates/ez-llm/tests/tmap_tool_json.rs`: todos los cortes por punto de código dan JSON válido y el corte del informe se repara.
- `rust/crates/ez-agent/tests/system_prompt_bypass.rs`: las ocho frases de evasión, auditoría una vez por versión, `system.md` normal y reglas de voz de la plantilla.
- `rust/crates/ez-agent/src/tools_herdr/summaries/tests.rs`: resúmenes sin dirección interna y resultados con solo el nombre visible (incluida la etiqueta compuesta).
- `rust/crates/ez-herdr/src/herdr/tests/parsing.rs`: el contexto del prompt no lleva `where`/`name` crudos y `label_for` resuelve.
- `rust/crates/ez-herdr/src/herdr/tests.rs`: nombre no numérico + espacio («planificador de EzAgent»).
- `rust/crates/ez-herdr/src/activity/tests.rs`: la fila viva no lleva agente.
- `rust/crates/ez-app/src/app/core/tests.rs`: frases de relleno exactas y tool_ack que no son muletillas.
- `rust/crates/ez-app/src/app/events/tests.rs`: aviso hablado con voz en el chat y ventana post-TTS al terminar.
- `rust/crates/ez-app/src/app/call/tests.rs`: arranque de voz durante el aviso, `signal_tts_stop` y flujo tras el barge-in.
- `rust/crates/ez-app/src/app/watch/tests.rs`: aviso de fin con pestaña, cuatro motivos y sin repetir la petición.

## Decisiones
- Ninguna `// DECISION` en producción: no se tocó código de producción; los únicos cambios fuera de pruebas son dos campos del doble de pruebas `FakeRuntime`.
- Los casos parametrizados de Python se mapean a una prueba Rust que recorre la misma tabla (mismo `rust_test_path` para varias filas), salvo el caso `İ/ı`, que apunta a su prueba ignorada por el hallazgo.
- Los casos de JSON, configuración y system prompt que ya estaban en fixtures se marcan `paridad` contra el fixture concreto y la prueba que lo consume (`cases_run == cases_in_file`).
- `test_every_self_attribute_used_by_voice_app_is_defined` y `test_the_request_marker_helpers_exist` son `no_aplica`: introspección AST del Python y un ayudante del arnés, respectivamente.
- `test_un_fallo_de_red_tambien_prueba_la_alternativa` es `no_aplica`: el fallo se inyecta monkeypatcheando `httpx.post`; en Rust la alternativa comparte host:puerto y la rama de conexión está cubierta por `without_a_server_the_error_explains_how_to_start_it`.

## Verificación
- pytest (las 11 pruebas del grupo, Python de referencia): **314 passed** en 11,16 s.
- `cargo fmt --all --check`: **ok**.
- `cargo clippy -p ez-core -p ez-stt -p ez-herdr -p ez-agent -p ez-hud -p ez-llm -p ez-app --all-targets -- -D warnings`: **ok**.
- `cargo test` de los siete crates tocados: **verde**, con 2 pruebas `#[ignore]` de hallazgos.
- Referencias del CSV: 314/314 filas, ids idénticos a `parity/test-map.csv`, sin `pendiente`, archivos y nombres de prueba verificados.
- Conteos por estado: `portada` 68 · `paridad` 56 · `nueva` 187 · `no_aplica` 3.
- Conteos por archivo: actividad 14/20 (nueva/portada), cableado 1/1 + 1 no aplica, turno 10/8/2, contexto 16/8/20/1, medidor 5, voz y dinero 55, STT 37/23/5/1, system prompt 12/20, tema 12/3, nombres 19/5/2, ejecución 6/7/1.
- Fixtures consumidos (casos en el archivo): `tool_json.jsonl` (169; reparación, cierre, saneado, estructura y errores), `llm_tools.jsonl` (219; `request_timeout`, `tool_round_limit`, `max_tokens` y estados del mensajero), `system_prompt.jsonl` (31; build, parts, reload y constants), `config.jsonl` (63; casos 0011-0012 del timeout STT) y `stt_dictionary.jsonl` (102; reglas y límites).

## Hallazgos (pruebas `#[ignore]`)

| Función | Entrada | Python | Rust |
|---|---|---|---|
| `Messenger._notify_context` / `Messenger::notify_context` | `on_context_event` lanza (`RuntimeError("la ventana se cerró")`) durante la compactación | La excepción se traga y el turno responde «ok» | El pánico desenreda el hilo `ezagent-turn`; el flujo termina vacío y se responde «El modelo devolvió una respuesta vacía…» |
| `stt_dictionary.correct_transcription` | `"İ ı I i"` con `"i → letra"` | «letra letra letra letra» (`re.IGNORECASE` empareja U+0130/U+0131) | «İ ı letra letra» (el `casefold` de Rust no los iguala) |

Pruebas ignoradas:
- `rust/crates/ez-llm/src/messenger/tool_loop/tests/waits.rs::a_failing_notification_callback_does_not_break_the_turn`
- `rust/crates/ez-stt/tests/tmap_stt_cases.rs::python_re_ignorecase_matches_turkish_dotted_and_dotless_i`

## Pendiente o riesgos
- Los dos hallazgos anteriores requieren decisión de coordinación: igualar la tolerancia de Python (envolver la callback) o documentar la diferencia como deliberada. No se tocó producción por el alcance de TMAP-4.
- `test_un_fallo_de_red_tambien_prueba_la_alternativa` y la rama de `whisper_home` con `VoiceConfig` rota no son inyectables sin cambiar producción; quedan descritas en el CSV.
- Las pruebas de aviso de espera dependen de `BUSY_WAIT_SECS = 1,5 s`; se ejecutaron cinco veces seguidas en verde. Añaden ~1,6 s a la suite de `ez-llm`.
- El mapa usa `nueva` también cuando se amplió una prueba existente con los casos exactos del Python (p. ej. `meta_line`, `busy_done`, `filler`, `hub`); la ruta apunta a la prueba que hoy los comprueba.

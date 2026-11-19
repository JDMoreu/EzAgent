# DX-2b r1 — Acuses cortos de voz (implementación)

Rama `work/DX-2`. Implementación del brief `DX-2b` sobre el diagnóstico `DX-2-r1`.
Se borra la sonda `rust/crates/ez-app/tests/zz_dx2_probe.rs` (commit `136cfb0`).
No se tocó la app de voz abierta, ni Herdr, ni se lanzaron agentes reales.

## Resultado
- Brief/tarea: DX-2b
- Estado: completado

## Cambios
- `rust/crates/ez-app/src/fast.rs`: constantes de acuse (`ACK_TOKENS`, `ACK_EXACT`, `ACK_REPLIES` de A3, ventana 10 s), `is_pure_acknowledgement`, `FastOutcome.ack`, tres métodos nuevos de `FastHost` con valores por defecto y el bloque final de acuse en `fast_command` (después de los atajos, consulta solo si el texto es acuse puro).
- `rust/crates/ez-app/src/fast/tests.rs`: tabla de acuses, casos sin contexto/tras pregunta/fuera de ventana, rotación de las seis respuestas, prioridad de atajos y rechazo de texto con contenido nuevo.
- `rust/crates/ez-app/src/app/core/state.rs`: cuatro campos nuevos en `SpeechState` (`last_outbound_kind`, `last_outbound_done_at`, `last_outbound_question`, `ack_index`), con `RwLock`/`AtomicUsize` como pedía el diagnóstico.
- `rust/crates/ez-app/src/app/core/init.rs`: **archivo extra**, iniciación mecánica de esos cuatro campos; sin ella `SpeechState` no compila. Es la única razón por la que se tocó un archivo fuera de la lista del brief.
- `rust/crates/ez-app/src/app/settings_apply.rs` (`AppFastHost`): `ack_context()` consume el tipo y calcula segundos + `question`; `ack_index()` con `fetch_add`; `ack_window_secs()` lee `ack_fast_window_secs` (10 s por omisión). `fast_command_outcome()` expone el resultado para el registro; `apply_fast_outcome` marca si la frase hablada terminaba en «?».
- `rust/crates/ez-app/src/app/events/branches.rs`: `event_transcribed` registra `ack_fast` frente a `fast_command`; `event_speak_done` y `event_answer_done` guardan tipo/instante; `event_swarm_notice` marca las preguntas; los errores se dicen con kind `error` para que no abran el atajo.
- `rust/crates/ez-app/src/app/events/tests.rs`: acuse tras afirmación (una frase corta, sin turno LLM, `ack_fast` registrado), acuse tras pregunta (va al LLM), segundo acuse consecutivo (va al LLM).
- `rust/crates/ez-app/src/app/watch.rs`: A1, los avisos de «terminó» pasan de pregunta a afirmación; `handle_done_answer` y `handle_done_prompt` fijan `last_outbound_question`. Los motivos `lost`/`blocked`/`error` conservan su texto y siguen siendo preguntas.
- `rust/crates/ez-app/src/app/watch/tests.rs`: actualizadas las dos pruebas que afirmaban los textos viejos de `done_question` (`done_question_names_the_tab_and_keeps_flow_names`, `done_prompt_queues_the_question`); añadida `a_question_announcement_marks_the_outbound_as_a_question`.
- `rust/crates/ez-agent/src/text/voice_flow_hint.txt`: bloque «# Acuses cortos» con la regla y las respuestas, inyectado también en `system.md` ya existentes.
- `rust/crates/ez-agent/tests/system_prompt_parity.rs`: nueva prueba `voice_flow_hint_includes_the_short_ack_rule`; en la paridad se sustituye solo el texto viejo del hint por el nuevo, porque el oráculo Python (`src/ezagent/agent/system_prompt.py`) y su fixture aún no traen el bloque (DX-2b es cambio solo en Rust). El resto de campos se compara sin relajar.
- `rust/crates/ez-app/tests/eval_context_parity.rs`: **archivo extra**, mismo ajuste puntual sobre el prompt congelado (`parity/frozen/eval_context.json`), también anterior a la regla. Sin él, `context_matches_the_frozen_python_provider` compara un prompt con el bloque nuevo contra otro sin él.

## Decisiones
- `# DECISION: se comprueba al final para que los atajos reales ganen siempre.` (bloque de acuse detrás de agradecimiento, saludo, listados; `mutea`/`cierra`/`repite` ganan).
- `# DECISION: no se incluye «sí» en los acuses porque puede responder a la pregunta del aviso.` (tampoco «no»).
- `# DECISION: la regla va en voice_flow_hint.txt (código) para alcanzar system.md ya existentes.`
- `# DECISION (DX-2b A1): afirmación, no pregunta, para que un «ok» posterior sea un acuse y no un «sí» (A2).`
- `# DECISION: solo una respuesta completa deja contexto de acuse; un error o una cancelación no deben abrir el atajo (DX-2b A2).`
- `# DECISION: la respuesta de acuse se dice con kind «ack» para que un segundo «ok» seguido no rearme el atajo.`
- `# DECISION: el contexto se consulta solo si el texto es acuse puro, para que un atajo posterior no consuma el tipo de salida hablada.`
- Se añadió `FastOutcome.ack` (campo con `Default`) para distinguir el registro `ack_fast` sin cambiar la firma de `fast_command` ni romper la paridad Python↔Rust de `app_text_parity.rs`.

## Verificación
Ejecutado desde `rust/`, solo con `-p ez-app -p ez-agent`:

- `cargo fmt -p ez-app -p ez-agent`: correcto; los archivos quedan formateados.
- `cargo clippy -p ez-app -p ez-agent --all-targets -- -D warnings`: correcto (`Finished`, código de salida 0). Los avisos de `vendor/freetype-sys` son de una dependencia y no son errores.
- `cargo test -p ez-app -p ez-agent`: correcto, **678 passed / 0 failed** en 31 objetivos (incluye `ez-agent` 286 y `ez-app --lib` 358), `EXIT=0`.
- Pruebas nuevas: 26 en `fast::tests`, 29 en `app::events::tests` (3 de acuse), 19 en `app::watch::tests`, 2 en `system_prompt_parity` y 1 en `eval_context_parity`.
- Flaky conocido: en la línea base, antes de tocar nada, `app::core::hud_bridge::tests::the_writer_keeps_the_last_value_per_key_in_order` falló bajo carga (348 passed / 1 failed) y pasó aislada (`1 passed`). En las dos ejecuciones finales del suite completo pasó dentro de `ez-app --lib` (358/358 y 0 fallos), así que no se tocó esa prueba.
- Métricas (registro real `latency.jsonl`, líneas generadas por las pruebas del camino rápido): `ack_fast` con `handled_ms` de **0.2–0.4 ms**, 0 rondas LLM y una sola frase dicha. Referencia de la línea base del diagnóstico: turno de 1 ronda `ttfa_ms` mediana 4624, `llm_ms` 3458, `turn_total_ms` 11112; `fast_command` 0–16 ms. La comparación de TTFA en voz real requiere repetir la medición con el mismo provider/modelo; aquí no se lanzaron agentes reales ni se usó el micrófono.

## Archivos fuera de la lista del brief
- `rust/crates/ez-app/src/app/core/init.rs`: cuatro líneas para inicializar los campos nuevos de `SpeechState`; sin ellas no compila. No es un cambio funcional.
- `rust/crates/ez-app/tests/eval_context_parity.rs`: una adaptación para que el prompt congelado del eval incluya el bloque nuevo. La lista del brief no la menciona, pero la prueba comparaba el prompt completo y fallaba.

## Pendiente o riesgos
- El oráculo Python, `parity/fixtures/system_prompt.jsonl` y `parity/frozen/eval_context.json` siguen con el hint antiguo: la paridad se adapta solo en ese bloque. Si se regeneran desde Python, hay que llevar la misma regla al oráculo o revertir la adaptación.
- El registro `ack_fast` se escribe en `%APPDATA%\EzAgent\logs\latency.jsonl` (ruta de producción); las pruebas que exercise el camino rápido añaden una línea. Es el punto donde la propuesta mide el antes/después.
- `ack_fast_window_secs` es configurable pero no se añadió a la UI del menú; se ajusta en `voice.json`.

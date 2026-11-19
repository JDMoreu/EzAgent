# DX-2b — Implementar los acuses cortos de voz (con los ajustes de la revisión)

**Lee `AGENTS.md`, `rust-migration/reports/DX-2-r1.md` (tu diagnóstico y propuesta, ya
revisados) y este brief.** Rama `work/DX-2` (tiene el informe y una sonda que **borras**
en este encargo: `rust/crates/ez-app/tests/zz_dx2_probe.rs`).

## Archivos que se tocan (solo estos)
`rust/crates/ez-app/src/fast.rs` (+ `fast/tests.rs`), `rust/crates/ez-app/src/app/core/state.rs`,
`rust/crates/ez-app/src/app/events/branches.rs` (+ `events/tests.rs`),
`rust/crates/ez-app/src/app/settings_apply.rs` (`AppFastHost`),
`rust/crates/ez-app/src/app/watch.rs` (+ `watch/tests.rs`: textos de aviso),
`rust/crates/ez-agent/src/text/voice_flow_hint.txt` y la prueba del prompt en `ez-agent`.
Si otra prueba existente afirma los textos viejos de aviso, actualízala y lístala.

## Lo que se implementa (tu propuesta §2, con estos ajustes del agente principal)
1. **A1 — Avisos que no preguntan.** Hoy `done_question` (`watch.rs:394`) termina con
   «¿Quieres que te resuma qué hizo…?». Tras una pregunta, «ok» o «dale» significan «sí»,
   no un acuse. Los avisos de «terminó» pasan a ser afirmaciones:
   - `Oye, {agente} terminó. Cuando quieras te cuento qué hizo.`
   - flujo: `Oye, terminó {flujo}. Cuando quieras te cuento el resultado.`
   (Los demás motivos —error, bloqueado— conservan su texto; si alguno pregunta, déjalo y
   el acuse no aplica tras él, ver A2.)
2. **A2 — El acuse rápido no aplica tras una pregunta.** Además de tu
   `last_outbound_kind`/`last_outbound_done_at`, guarda `last_outbound_question: bool`
   (la frase hablada terminaba en «?»). Si era pregunta → no hay atajo: va al LLM como hoy.
3. **A3 — Respuestas naturales** (rotan con `ack_index`, sin repetir la anterior):
   `Listo, quedo atento.`, `Me avisas si necesitas algo.`, `Perfecto.`,
   `Dale, aquí sigo.`, `Entendido.`, `Cualquier cosa me dices.`
   (fuera «Perfecto, no hay cambio.» y «Ya, gracias.»).
4. El resto de tu propuesta tal cual: `is_pure_acknowledgement` (≤ 3 palabras, todas
   acuses; sin «sí» ni «no»), el bloque al final de `fast_command`, la ventana
   `ack_fast_window_secs` (10 s), el consumo del tipo tras usarlo, `record_command("ack_fast",
   …)` y la regla «# Acuses cortos» en `voice_flow_hint.txt`. «gracias» suelto sigue con su
   atajo actual («De nada.»).

## Pruebas
Las cinco de tu §2.3, más: tras un aviso que pregunta → «ok» va al LLM; tras el aviso nuevo
(afirmación) → «ok» responde una frase de A3; los textos nuevos de `done_question`.

## Criterios de aceptación
`cargo fmt -p ez-app -p ez-agent`, `cargo clippy -p ez-app -p ez-agent --all-targets -- -D
warnings` y `cargo test -p ez-app -p ez-agent` en verde (si una prueba de `ez-app` falla
solo bajo carga y pasa aislada, demuéstralo). Informe en
`rust-migration/reports/DX-2b-r1.md`.

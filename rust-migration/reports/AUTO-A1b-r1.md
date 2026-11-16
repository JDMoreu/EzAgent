# AUTO-A1b — `ez-agent::autonomous` a la versión 2 (partes puras)

## Resultado
- Brief/tarea: AUTO-A1b (fase A1b: 10.1-10.5 y 10.10-10.17 en `ez-agent`)
- Estado: completado

## Cambios
- `rust/crates/ez-agent/src/autonomous/mod.rs`: `Criterion {text, check}`, `StartSpec`/`TeamSpec` con `workdir` y `max_approvals`, y el trait `AutonomousHost` con `resume(extend_rounds, extend_minutes)` y `stop(reason, accept)` (10.2, 10.3 y 10.5).
- `rust/crates/ez-agent/src/autonomous/state.rs`: `scope` pasa a `Vec<ScopeEntry {address, role, edit_dirs}>` con `executor|verifier|member`; `approval_scope` no devuelve nada en pausa; `executor_started`/`member_started` editan solo en `workdir` y el verificador en su carpeta; `on_executor_idle` (fin-N + 3 min), `Incident::Lost`, `record_approval` (tope 200), 3 verificaciones por ronda, `paused_at` que descuenta el tiempo en pausa, `resume` con prórroga, `stop` con `accept`, `base_commit` y `verification_seq` (10.1, 10.10, 10.13 y 10.14).
- `rust/crates/ez-agent/src/autonomous/artifacts.rs`: `privado/` (estado, bitácora, resumen y copias de veredictos) y `compartido/` (encargo, criterios, corrección y fin-N) separados; `verificacion-K/` con su `veredicto.json`; nonce de 16 hex por verificación; `fin_written_after`; el encargo pide `fin-N.md` y nunca menciona `privado` (10.10).
- `rust/crates/ez-agent/src/autonomous/artifacts/correction.rs`: `correccion-N.md` con la cabecera fija de 10.12 y cada hallazgo saneado (`#`, cercas y `<`/`>` fuera, 500 caracteres por campo).
- `rust/crates/ez-agent/src/autonomous/artifacts/summary.rs`: `privado/resumen.md` a partir de criterios, evidencia del último veredicto limpio (redactada, ≤ 3 líneas), `git diff --stat` y `git log --oneline` que recibe de A3 (10.16).
- `rust/crates/ez-agent/src/autonomous/verdict.rs`: validación estricta de 10.11 (token, una entrada por criterio, evidencia no vacía, `cumple` coherente, severidades válidas, claves extra ignoradas, JSON solo o cercado); el churn compara con todo lo corregido (10.14).
- `rust/crates/ez-agent/src/autonomous/audit.rs`: `ApprovalRecord`, `approval_detail` (tipo, opción ≤ 60 redactada, primer token + SHA-256 de 16 hex, ruta redactada, dirección y decisión), `log_approval` (`autonomous.approve`, `turn_id = autonomous:<id>`) y `redact_value` para la bitácora (10.15).
- `rust/crates/ez-agent/src/autonomous/tools.rs` → `tools/{mod,start,actions,schema}.rs`: cinco herramientas con nivel y `self_confirmed` de 10.5, `workdir` con `default_cwd`, criterios `{text, check}`, `AUTHORIZE_RE` sobre los últimos 3 mensajes de 10 min + `NEGATION_RE`, confirmación fuerte que enumera criterios, carpeta, agente/papeles y topes, espera si hay otra confirmación pendiente, comprobación de timeout antes de `host.start`, `resume_autonomous_task` y `stop` con `accept` (10.2-10.5).
- `rust/crates/ez-agent/src/autonomous/texts.rs`: textos de 10.2, 10.3, 10.10, 10.13 y 10.16 (confirmación con criterios, fin con resumen, aviso de reposo, motivos nuevos).
- `rust/crates/ez-agent/src/runtime/state.rs`, `runtime.rs`: historial de los tres últimos mensajes del usuario con su instante (10.4), mínimo y aditivo.
- `rust/crates/ez-agent/src/runtime/selection.rs`, `runtime/tools.rs`: `SelectOptions` gana `autonomous_authorized` y `autonomous_active`; las de arranque solo se ofrecen con autorización y `status`/`stop`/`resume` con tarea (10.5).
- `rust/crates/ez-agent/src/registry.rs`, `policy.rs`, `runtime/execute.rs`: `ToolSpec.self_confirmed` y `policy::with_self_confirmed`; la política no añade su confirmación a las dos de arranque (10.5).
- `rust/crates/ez-agent/src/autonomous/tests*.rs`: 59 pruebas del módulo (antes 40), incluidas las de cada punto de 10.1-10.17.
- `rust-migration/reports/AUTO-A1b-r1.md`: este informe.

## Decisiones
- `// DECISION`: `AutonomousHost` cambia `resume` y `stop` para llevar la prórroga y `accept`; son las dos únicas formas de cumplir 10.5 y 10.13 sin un canal aparte.
- `// DECISION`: `executor_started`/`member_started` no reciben `edit_dirs`: los derivan de `workdir`, porque 10.1 fija esa carpeta para los dos papeles. El verificador sí recibe la suya (`verificacion-K`).
- `// DECISION`: `task_active()` (selección) incluye la tarea `done` pendiente de revisión, para que «acepto» pueda llamar a `stop_autonomous_task {accept}`; el hueco para arrancar otra sigue siendo `unfinished` (10.1).
- `// DECISION`: 10.3 fija el contenido de la confirmación pero no su literal; las líneas son `{n}. {texto} (se comprueba con: {check})` y la frase «hasta que se cumplan estos criterios».
- `// DECISION`: textos que el diseño no fija: `RESUMED`, `NOT_PAUSED`, `ACCEPTED`, `TIMED_OUT`, `MISSING_CHECK`, `REASON_APPROVALS`, `REASON_LOST`, `REASON_VERIFICATIONS`, `REASON_DOUBTFUL`, `idle()` y `done_with_summary()` (este último con el literal de 10.16).
- `// DECISION`: `estado.json` cambia `last_findings` por `corrected_findings` (todos los problemas ya mandados a corregir, 10.14) y añade `workdir`, `base_commit`, `max_approvals`, `accepted`, `paused_at` y `verification_seq`.
- `// DECISION`: un veredicto `cumple=false` sin hallazgos bloqueantes ni mayores pausa con «el veredicto no cumple pero no dice qué corregir»; no se escribe una corrección vacía (F-04 y 10.17).
- `// DECISION`: se mantienen los dos topes de 10.11 y 10.13: dos veredictos inválidos seguidos pausan y, además, nunca se lanzan más de tres verificaciones por ronda.
- `// DECISION`: `resume` descuenta el tiempo en pausa y suma `extend_rounds`/`extend_minutes`; un «sigue» sin prórroga no amplía los topes (el usuario decide en la pausa). A3 decide cómo mapea la voz a la prórroga.
- `// DECISION`: `accept` solo tiene efecto en `done`; parar una tarea en curso nunca la da por aceptada.
- `// DECISION`: `max_approvals` se lee de `autonomous_max_approvals` (200 por omisión); 9.8 solo nombra rondas y minutos, pero 10.13 incluye el tope de aprobaciones.
- `// DECISION`: si ya hay otra confirmación pendiente, la activación espera hasta 30 s y, si no se libera, declina (10.5: «espera»), con tope para no agotar el timeout del registro.
- `// DECISION`: el JSON cercado solo se desenvuelve si la cerca aparece antes del primer `{`; así una cerca dentro de un hallazgo no rompe el análisis.
- `// DECISION`: `log_approval` usa `turn_id = autonomous:<id>` para correlacionar la auditoría del bucle (M-02 de la revisión B).

## Verificación
- `cargo fmt --all`: limpio.
- `cargo clippy -p ez-agent --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-agent`: **237 pruebas de librería + 18 de integración = 255 en verde**, 0 fallos; **59 del módulo autónomo** (antes 40).
- `cargo check --workspace`: limpio.
- Tamaños tras `cargo fmt` (tope 600): `state.rs` 583, `tests/tools.rs` 586, `tools/mod.rs` 388, `artifacts.rs` 305, `verdict.rs` 312, `texts.rs` 264, `tools/start.rs` 256, `audit.rs` 197, `artifacts/summary.rs` 182, `artifacts/correction.rs` 114, el resto menor.
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`, `remove_dir_all`, `unsafe`, `"cmd"`, `"/c"`, `-Command` ni `.unwrap()` fuera de pruebas.
- Ningún lock se retiene durante una operación larga ni una llamada a otro componente: el historial y los topes se copian antes de soltar el cerrojo, y la confirmación (con su espera) corre fuera de él.

## Fixtures consumidos
- Ninguno: el brief no define fixtures de autonomía y `parity/fixtures/` no tiene un dominio autónomo. 0 casos.

## Pruebas portadas (`parity/test-map.csv`)
- Ninguna fila: es funcionalidad nueva aprobada por el usuario, sin Python de referencia en `src/`; `test-map.csv` no tiene entradas de autonomía. Las 59 pruebas propias cubren: papeles y `edit_dirs`, hueco único con `preparing/running/verifying/paused`, `workdir` por argumento o `default_cwd`, criterios con `check` y confirmación fuerte que los enumera, autorización en los últimos 3 mensajes de 10 min con negación, niveles y `self_confirmed`, selección de herramientas, carpetas privada/compartida/verificación-K, nonce, `fin-N.md`, reposo de 3 min, `lost`, veredicto estricto (cercas, token, criterios, evidencia, coherencia, claves extra), corrección saneada, topes de aprobaciones y verificaciones, tiempo en pausa, churn contra todo el historial, redacción de auditoría y bitácora con clave falsa, y `resumen.md`.

## Pendiente o riesgos
- **A3 debe pasar `task.shared()` a `on_verdict`** para que la ruta de `correccion-N.md` coincida con `TaskDir::correction_path`; el doc de la función lo dice.
- **A3 debe incluir `Verification.token` en el prompt del verificador** y pedirle el JSON con `token`; sin él, 10.11 rechaza todos los veredictos. No se añadió `VERIFIER_PROMPT` aquí porque el bucle es A3.
- **`terminal` ya abierta**: 10.1 pide que solo entre si el usuario la nombró; en código puro no se puede comprobar contra una dirección técnica. La herramienta la pasa y la confirmación no la menciona (10.3 no lo pide). A3/A2 deben validar que la dirección pertenece al espacio o pedir confirmación aparte.
- **`resume` sin prórroga en un tope**: si la tarea se pausó por rondas o tiempo y el usuario dice «sigue» sin `extend_*`, volverá a pausarse en la siguiente comprobación. A3 decide el mapeo de la voz.
- **`accept` en una tarea en curso**: se ignora (solo aplica a `done`). Si el producto quiere aceptar un resultado parcial al parar, es un cambio de estado pequeño.
- **Modo B (10.18)**: `TeamSpec` aún no lleva `verifier_agent`/`verifier_model`; es fase B, no A1b.
- **`install_host` no reconstruye el registro**: si A3 lo llama después de crear el runtime, debe usar `add_tool_group` o forzar `rebuild_tools`; se mantiene el criterio de A1.
- **Métrica de churn**: sigue siendo Sørensen–Dice sobre bigramas normalizados (decisión de A1); los umbrales están aislados en `verdict.rs`.

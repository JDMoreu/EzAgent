# DX-5f — No interpretar una negación como autorización

Continúa `work/DX-5`; lee `AGENTS.md`, `_COMMON-ECO.md`, DX-5 y DX-5e. Mantén el alcance de DX-5e y sus prohibiciones. No abras ventanas, no ejecutes agentes reales ni leas credenciales.

La revisión de `place.rs` y `place/answers.rs` encontró que `agent_said` y `space_said` aceptan menciones sin examinar la negación, y `answer_shaped` trata «no» como conector. Si el modelo propone OpenCode después de «no con OpenCode», la regla puede considerar autorizada la elección. Comprueba con el puerto falso la secuencia de pregunta pendiente y respuesta negativa, y también una petición inicial que niega un agente o espacio. Impide que una elección negada autorice un lanzamiento o envío; si hay ambigüedad, pregunta de nuevo. Conserva respuestas afirmativas legítimas y las respuestas mixtas con alternativa explícita cuando se puedan interpretar de forma segura.

Añade pruebas de agente y espacio negados, alternativa afirmativa y continuación legítima. Por la carga del equipo, no ejecutes compilaciones ni pruebas hasta que el orquestador lo indique. Deja un commit e informe `rust-migration/reports/DX-5f-r1.md` explicando lo verificado y lo pendiente.

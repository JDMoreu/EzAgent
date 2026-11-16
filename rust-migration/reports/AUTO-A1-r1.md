# AUTO-A1 — ez-agent::autonomous: estado, artefactos, veredicto, textos y herramientas con guardas

## Resultado
- Brief/tarea: AUTO-A1 (fase A1 del modo autónomo: 9.2, 9.3 y 9.6)
- Estado: completado

## Cambios
- `rust/crates/ez-agent/src/autonomous/mod.rs`: módulo nuevo, trait `AutonomousHost` (`start`, `start_team`, `status`, `resume`, `stop`) y los datos `StartSpec`, `TeamSpec`, `RoleSpec` que piden las herramientas.
- `rust/crates/ez-agent/src/autonomous/state.rs`: `AutonomousState` (el `estado.json` de 9.3, con el orden de campos del diseño), `Mode`, `TaskState`, `Executor`, `Team`, `TeamRole`, `Summary` y `Action`. Transiciones puras de 9.5: `on_executor_finished`, `on_verdict`, `on_incident`, `on_deadline`, `pause`, `resume`, `stop`, más `on_restart` (9.3), `executor_started`, `verifier_started`, `cover`/`forget` para `scope`.
- `rust/crates/ez-agent/src/autonomous/artifacts.rs`: carpeta `%APPDATA%/EzAgent/autonomo/<id>/` con `id = AAAAMMDD-HHMMSS-<4 hex>`; `encargo.md`, `criterios.md`, `estado.json` atómico (temporal + rename), `bitacora.jsonl` (una línea `{ts, event, detail}`), `ronda-N/veredicto.json`, `correccion-N.md` y `load_state`.
- `rust/crates/ez-agent/src/autonomous/verdict.rs`: esquema del veredicto de 9.5 (`cumple`, `criterios`, `hallazgos` con severidad bloqueante/mayor/menor), validación con texto propio, `clean()`, `problems()` y detección de churn por similitud ≥ 0,85.
- `rust/crates/ez-agent/src/autonomous/texts.rs`: literales de 9.6 (`confirm`, inicio, ronda, pausa, fin, parada) y los textos de las guardas, motivos de pausa y etiquetas de estado de la fila 9.8.
- `rust/crates/ez-agent/src/autonomous/tools.rs`: `start_autonomous_task`, `start_autonomous_team`, `autonomous_status` y `stop_autonomous_task` (nivel 2, esquemas de 9.2), `AUTHORIZE_RE` y las cuatro guardas. `RuntimeGuards` conecta las guardas con `AgentRuntime` (taint, último mensaje y `ConfirmationManager::ask`).
- `rust/crates/ez-agent/src/lib.rs`: `pub mod autonomous;`.
- `rust/crates/ez-agent/src/runtime/tools.rs`: registro aditivo del grupo autónomo donde se construyen los demás grupos.
- `rust/crates/ez-agent/src/autonomous/tests.rs` + `autonomous/tests/{state,verdict,artifacts,tools}.rs`: 40 pruebas unitarias nuevas.
- `rust-migration/reports/AUTO-A1-r1.md`: este informe.

## Decisiones
- `// DECISION`: el trait del bucle añade `start_team` a los cuatro métodos pedidos; `start_autonomous_team` no cabe en `start(spec)` sin un enum de modos.
- `// DECISION`: el grupo se registra siempre en `runtime/tools.rs`, pero devuelve vacío hasta que exista bucle (mismo criterio que `build_swarm_tools(None)`). A3 llama a `autonomous::tools::install_host` antes de crear el runtime o registra el grupo con `build_autonomous_tools` + `RuntimeGuards`.
- `// DECISION`: `verify_failures` y `executor_errors` son contadores de proceso y no van a `estado.json` (`#[serde(skip)]`); el esquema de 9.3 no los lista y tras un reinicio la tarea se pausa igual.
- `// DECISION`: `started_at` y `deadline` se guardan como segundos epoch porque el diseño no fija formato; el `ts` de la bitácora es ISO local con desfase (`pytime::now_local_iso_seconds`).
- `// DECISION`: `correccion-N.md` usa la N de la ronda verificada, la misma de `ronda-N/veredicto.json`; después `round += 1`.
- `// DECISION`: la similitud de churn es Sørensen–Dice sobre bigramas de caracteres, con el texto normalizado (sin tildes, minúsculas, solo letras y dígitos). El diseño pide ≥ 0,85 sin fijar métrica.
- `// DECISION`: `last_findings` guarda los `problema` corregidos de la última ronda sucia y se limpia en una ronda limpia: el churn se mide contra «la ronda anterior ya corregido».
- `// DECISION`: `resume(now)` es un punto de control: si la ronda ya superó el tope, sube `max_rounds` a la ronda actual; si el plazo venció, renueva la misma ventana de tiempo. Así «sigue» no vuelve a pausar de inmediato.
- `// DECISION`: en `start_autonomous_team` el `{agente}` del texto único de confirmación (9.6 solo tiene un hueco) es el del primer papel.
- `// DECISION`: el timeout del registro para las dos herramientas de arranque es 120 s (la confirmación por voz puede tardar y el bucle tarda en lanzar); estado y parada usan 20 s.
- `// DECISION`: el estado `preparing` se usa de verdad: la tarea nace en `preparing` y `executor_started` la pasa a `running` y añade la dirección a `scope`.
- No hizo falta tocar el runtime para la guarda 2: `AgentRuntime::get_last_user_text()` ya expone el último mensaje (callbacks + respaldo de sesión).

## Verificación
- `cargo fmt --all --check`: limpio.
- `cargo clippy -p ez-agent --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-agent`: **220 passed, 0 failed** (40 nuevas del módulo autónomo).
- Tamaños tras `cargo fmt` (tope 600): `artifacts.rs` 238, `mod.rs` 67, `state.rs` 408, `texts.rs` 115, `tools.rs` 488, `verdict.rs` 242, `tests/artifacts.rs` 247, `tests/state.rs` 305, `tests/tools.rs` 349, `tests/verdict.rs` 89.
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`, `remove_dir_all`, `unsafe`, `"cmd"`, `"/c"`, `-Command` ni `.unwrap()` en el código añadido.
- Ningún lock se retiene durante una llamada al host, a la UI o a disco: el estado puro se muta en memoria y la confirmación se pide antes de llamar a `host.start`.

## Fixtures consumidos
- Ninguno: el brief indica «(ninguno)» y no hay fixtures de autonomía en `parity/fixtures/`. 0 casos.

## Pruebas portadas (`parity/test-map.csv`)
- Ninguna fila: es funcionalidad nueva aprobada por el usuario, sin Python de referencia en `src/`; `test-map.csv` no tiene entradas de autonomía. Las 40 pruebas propias cubren transiciones (dos rondas limpias → `Done`, ronda sucia → corrección, churn, topes de rondas y tiempo, veredicto inválido → reintento y pausa, incidentes, pausa/reanudación/parada, reinicio), artefactos en un directorio temporal y las cuatro guardas (cada una rechaza; las cuatro pasan + «sí» → `start`).

## Pendiente o riesgos
- **El bucle real no existe en A1**: `AutonomousHost` solo tiene un doble de pruebas. Hasta que A3 llame a `install_host` (o registre el grupo con `build_autonomous_tools` + `RuntimeGuards`), el grupo queda vacío y las cuatro herramientas no se ofrecen al modelo: es intencional, para no exponer herramientas rotas.
- **Doble confirmación con `confirm_actions` activo**: si el usuario tiene activada la confirmación de operaciones no destructivas, la política de nivel 2 pediría una confirmación genérica además de la de activación. A3 debería excluir estas herramientas de esa pregunta o aceptarlo; no se tocó `execute` porque está fuera del alcance.
- **`resume` no tiene herramienta**: 9.5 menciona «sigue» con `resume` «vía la herramienta», pero 9.2 no define ninguna. El trait expone `resume()` y el estado su transición; A3 decide cómo recibe el «sigue» por voz.
- **Límites de `voice.json`**: `autonomous_max_rounds` y `autonomous_max_minutes` (9.8) no se leen aquí; las herramientas usan 5/120 como pide 9.2. Leerlos es de la fase de app/UI.
- **`install_host` es un slot único** (`OnceLock`): si A3 necesita sustituir el bucle en caliente, debe usar `runtime.add_tool_group` con `build_autonomous_tools`, no reinstalar.
- **`terminals`/`workdirs`**: `encargo.md` escribe las rutas que el host pase en `Encargo.workdirs`; si llega vacío, escribe el espacio. El host (A3) es quien conoce la carpeta real del espacio.
- **Métrica de churn**: al no estar especificada, la similitud es una decisión propia; los umbrales y textos están aislados en `verdict.rs` para ajustarlos con evidencia de uso.

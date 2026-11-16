# TMAP-6 — Mapa de pruebas Python → Rust, grupo 6

## Resultado
- Brief/tarea: TMAP-6
- Estado: completado

## Cambios
- `parity/test-map/TMAP-6.csv`: 313 filas del grupo (las 10 pruebas de Python), con crate, referencia Rust y estado; ninguna `pendiente`.
- `rust/crates/ez-agent/src/audit.rs`: pruebas de no-mutación, concurrencia y fallo de disco (huecos de `test_audit.py`).
- `rust/crates/ez-agent/src/runtime/tests/execute.rs`: pruebas nuevas de runtime (escritura permitida + audit, taint fallido, delegado, panic, shell, límite de archivo) y selección/notificación ampliadas. Se partió el módulo para no pasar de 600 líneas.
- `rust/crates/ez-agent/src/runtime/tests/execute/security.rs`: módulo nuevo con las pruebas de seguridad anteriores y la prueba `#[ignore]` del hallazgo de `max_file_mb`.
- `rust/crates/ez-tts/src/queue/tests.rs`: pruebas nuevas de `say` no bloqueante, colas acotadas y `finish` repetido.
- `rust/crates/ez-stt/src/client/tests.rs`: prueba nueva de error de conexión con ruta alternativa.
- `rust/crates/ez-swarm/src/incidents/tests.rs`: prueba nueva con las nueve pantallas exactas de `test_swarm_incidents.py`.
- `rust/crates/ez-hud/src/model/chat/header/tests.rs`: posición exacta del orbe y clic en ajustes.
- `rust/crates/ez-hud/src/model/chat/view/tests.rs`: pruebas nuevas de separación de controles, teclas de edición, PageUp/PageDown, botones close/new/voice, cancelación de borrado y nota larga; ampliadas las de copiar y escape.
- `rust/crates/ez-hud/src/model/menu/hit.rs`: cobertura de «sin clave no hay botón copiar».
- `rust/crates/ez-hud/src/model/menu/pointer/tests.rs`: copia fallida sin check.
- `rust/crates/ez-hud/src/paint/menu/advanced/tests/mod.rs`: placeholder «Sin clave» y edición que sigue ocultando la clave.
- `rust/crates/ez-hud/src/paint/chat/render/tests.rs`: ancho de la zona `copy_code` (icono).
- `rust/crates/ez-hud/src/paint/chat/render_rich/tests.rs`: cabecera de tabla que nunca se parte por redondeo.
- `rust/crates/ez-hud/src/win/chat/compose/tests.rs`: orbe de tamaño fijo y sin halo.
- `rust/crates/ez-hud/src/win/layered.rs`: `set_pos` con y sin handle (lazy drag).

## Decisiones
- `// DECISION` en código nuevo: ninguna; las decisiones son de mapeo y se listan aquí.
- Las pruebas de caché de `test_card_performance.py` basadas en monkeypatch de `_compose_card` se marcan `no_aplica`: en Rust la caché se llavea con `FrameKey`/`CardKey` y no hay costura para contar composiciones. La equivalencia funcional se apoya en `frame_key` y en el render de paridad.
- Los casos de `test_tasks.py` con entradas que no son mapa (`None`, `"texto"`, `[]`) se marcan `no_aplica`: `add_chain` recibe `&Map` y esos valores no existen en la API Rust.
- `test_las_preguntas_del_enjambre...` se marca `no_aplica` porque la prueba de Python sustituye `_queue_announcement` de `VoiceApp`; la regla vive en `events/branches.rs::event_swarm_notice`.
- Los destinos `file:///` y `~/` de documentos se mapean a `is_local_target` (fixture) porque comparten predicado con la conversión ya probada; no hay caso `parse` propio con esas formas.
- Se corrigió el crate de las filas de `test_tasks.py` a `ez-herdr` (dueño de `tasks.py`), el de `test_secret_field.py` a `ez-hud` y el de las intenciones de selección a `ez-agent`; el mapa maestro las repartía entre `ez-app`, `ez-core` y `ez-audio`.

## Verificación
- `cargo fmt --all -- --check`: ok.
- `cargo clippy -p ez-agent -p ez-hud -p ez-stt -p ez-swarm -p ez-tts --all-targets -- -D warnings`: ok (solo avisos del vendor `freetype-sys`).
- `cargo test -p ez-agent -p ez-hud -p ez-stt -p ez-swarm -p ez-tts`: todo en verde. Suites principales: ez-agent 260, ez-hud 524, ez-tts 60, ez-stt 37, ez-swarm 121; más pruebas de integración y paridad. 1 `#[ignore]` de hallazgo.
- pytest: no ejecutado (el Python es de solo lectura en esta tarea).
- Filas del grupo: 313 de 313, ninguna `pendiente`.

### Fixtures consumidos
| Fixture | Casos |
|---|---:|
| `parity/fixtures/audio.jsonl` | 265 |
| `parity/fixtures/audit.jsonl` | 20 |
| `parity/fixtures/chat_markdown.jsonl` | 185 |
| `parity/fixtures/chat_store.jsonl` | 85 |
| `parity/fixtures/hud_chat.jsonl` | 221 |
| `parity/fixtures/policy.jsonl` | 144 |
| `parity/fixtures/system_prompt.jsonl` | 31 |
| `parity/fixtures/tools_fs.jsonl` | 120 |

### Filas por archivo y estado
| Prueba Python | portada | paridad | nueva | no_aplica | Total |
|---|---:|---:|---:|---:|---:|
| test_audit.py | 0 | 23 | 3 | 0 | 26 |
| test_audit_audio.py | 5 | 2 | 5 | 0 | 12 |
| test_card_performance.py | 6 | 0 | 3 | 5 | 14 |
| test_chat_contexts.py | 19 | 9 | 3 | 0 | 31 |
| test_chat_markdown.py | 31 | 12 | 0 | 0 | 43 |
| test_chat_view.py | 32 | 22 | 8 | 0 | 62 |
| test_runtime_security.py | 40 | 10 | 10 | 0 | 60 |
| test_secret_field.py | 5 | 0 | 3 | 0 | 8 |
| test_swarm_incidents.py | 12 | 0 | 9 | 1 | 22 |
| test_tasks.py | 32 | 0 | 0 | 3 | 35 |
| **Total** | **182** | **78** | **44** | **9** | **313** |

## Hallazgos
1. `AgentRuntime::set_config` no propaga `general.max_file_mb` a las herramientas de sistema de archivos ya construidas (`FsContext` clona la configuración al crearse). En Python, `tools_fs` lee el límite actual en cada escritura. Prueba marcada `#[ignore = "TMAP: diferencia con Python, ver informe"]`: `runtime/tests/execute/security.rs::changing_max_file_mb_at_runtime_reaches_the_writer` (entrada: límite 0,001 MB tras crear el runtime y escribir 5000 caracteres; Python rechaza, Rust escribe). Impacto real bajo: la app no llama `set_config` sobre el runtime, solo lo usa el altavoz.
2. `test_un_connect_timeout_prueba_la_alternativa_con_el_presupuesto_restante` no puede verificar el presupuesto restante con un `reqwest` real; la prueba nueva comprueba que el error de conexión recorre la ruta alternativa; el cálculo del presupuesto queda cubierto por lectura de código, no por aserción.
3. `test_the_living_mark_moves_without_touching_the_rest_of_the_card` solo se cubre por la prueba del overlay en el pie (píxeles de la marca); no se mide el bbox del cambio entre dos fotogramas.
4. La combinación claro × escala 1,5 de `test_the_panel_renders_at_the_right_size` no tiene caso de fixture propio (sí claro 1,0/1,25/2,0 y oscuro 1,0/1,5/2,0).

## Pendiente o riesgos
- No se tocó código de producción; el hallazgo 1 queda documentado y con prueba `#[ignore]`.
- La cobertura de las posiciones del icono de copiar y del aire del texto del campo se apoya en píxeles de paridad, no en aserciones de geometría equivalentes a las del Python.
- `parity/test-map.csv` no se modificó; la fusión de `parity/test-map/TMAP-6.csv` queda para coordinación.

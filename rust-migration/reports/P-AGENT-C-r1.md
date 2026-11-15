# P-AGENT-C — Paridad de `ez-agent` (web, skills, enjambre, herdr, memoria y diario)

## Resultado
- Brief/tarea: P-AGENT-C (ronda 1)
- Estado: completado (986 de 1021 casos ejecutados y en verde; 35 casos
  estructuralmente no aplicables, documentados abajo con su motivo)
- `cargo fmt --all`: limpio
- `cargo clippy -p ez-agent --all-targets -- -D warnings`: limpio
- `cargo test -p ez-agent`: 162 unitarias + 6 archivos de paridad, todo en verde

## Casos por dominio

| Dominio | Casos en el fixture | Ejecutados | Estructurales | Resultado |
|---|---:|---:|---:|---|
| `tools_web` | 240 | 218 | 22 | 218/218 en verde |
| `tools_skills` | 68 | 68 | 0 | 68/68 en verde |
| `tools_swarm` | 54 | 54 | 0 | 54/54 en verde |
| `tools_herdr` | 393 | 393 | 0 | 393/393 en verde |
| `memory` | 229 | 216 | 13 | 216/216 en verde |
| `journal` | 37 | 37 | 0 | 37/37 en verde |
| **Total** | **1021** | **986** | **35** | **986/986 en verde** |

Cada archivo de prueba termina con `assert_eq!(run [+ skipped], cases_in_file)`
y un `assert!` sobre el número real de casos del fixture, así que un caso nuevo
en el `.jsonl` rompe la prueba hasta que se implemente o se documente.

No se instrumentó un contador de «aprobados al primer intento»; se detalla en
cada dominio qué defectos aparecieron en la primera corrida completa y cuáles
eran del arnés de pruebas (no del código Rust).

## Cambios
- `rust/crates/ez-agent/src/tools_web/fetch.rs`: costuras `WebTransport`,
  `HostResolver` y `FetchClock` (`FetchHooks`) equivalentes al `transport=`, el
  DNS falso y el reloj congelado del oráculo; `pin_requests` reproduce la
  conexión fijada a la IP (F36) del HTTP real; el cuerpo se valida también en
  la ruta inyectada; la redirección sin `Location` ahora es error.
- `rust/crates/ez-agent/src/tools_web/url.rs`: el host se toma de la autoridad
  cruda (Unicode) en vez del `host_str` en punycode; `pinned_request_target`
  portado literalmente y versiones JSON de las funciones de validación.
- `rust/crates/ez-agent/src/tools_web.rs`: `run_fetch_page_json`,
  `build_web_tools_with_hooks`, `run_web_search_json`; `clean_results` vuelve a
  aceptar los datos crudos del proveedor (`href`/`url`, `body`/`snippet`).
- `rust/crates/ez-agent/src/search.rs`: `SearchProvider::search_raw` (datos
  crudos como el `Any` de Python) y `DdgsSearchProvider::search_json` para la
  validación de tipos.
- `rust/crates/ez-agent/src/tools_skills.rs`: costuras `SkillHooks` (intérprete,
  ejecutor de scripts y escritor de aprobaciones) para reproducir el
  `subprocess.run` y el `_write_approvals` sustituidos por el oráculo.
- `rust/crates/ez-agent/src/tools_herdr/port.rs` (nuevo): `HerdrPort` y
  `PortError`; implementación de producción sobre `Arc<Herdr>`.
- `rust/crates/ez-agent/src/tools_herdr/delegate.rs`: usa `HerdrPort`;
  `handle_run_with_ids` para congelar los ids de cadena; `read_output` público;
  los manuales se leen como texto (LF).
- `rust/crates/ez-agent/src/tools_herdr/summaries.rs`: `labelled`/`with_labels`
  reciben el resolutor de nombres, no el adaptador completo.
- `rust/crates/ez-agent/src/tools_herdr/builder.rs`: `build_herdr_tools_with_port`
  y adaptación a `HerdrPort`.
- `rust/crates/ez-agent/src/tools_herdr/args.rs`: rama `Unexpected` para el
  «Error inesperado al usar Herdr» del Python; `wrapped_value`.
- `rust/crates/ez-agent/src/tools_herdr/voice.rs`: `close` consulta el texto del
  usuario dos veces (secuencia del Python) y captura pánicos de callbacks como
  el `except Exception` del original.
- `rust/crates/ez-agent/src/journal.rs`: `JournalHooks` (reloj y Papelera
  inyectables), `write_entry_value` y `load_entries_value` públicos.
- `rust/crates/ez-agent/src/tools_memory.rs`: `tool_timeout` pasa a público.
- `rust/crates/ez-agent/tests/tools_web_parity.rs` (+`.../fakes.rs`),
  `tools_skills_parity.rs`, `tools_swarm_parity.rs`,
  `tools_herdr_parity.rs` (+`.../fake.rs`), `memory_parity.rs`
  (+`.../support.rs`) y `journal_parity.rs`: las seis pruebas de paridad.
- `rust-migration/python-quirks.md`: tres hallazgos nuevos al final
  (Q-P-AGENT-C-01..03).
- `rust-migration/reports/P-AGENT-C-r1.md`: este informe.

## Defectos de Rust corregidos (el Python era la especificación)
- `tools_web`: el DNS y los mensajes usaban el host en punycode; Python usa el
  nombre Unicode de `urlsplit().hostname` (caso `pinned-idn` y validaciones).
- `tools_web`: una respuesta 3xx sin `Location` se trataba como página normal;
  Python devuelve «La página devolvió una redirección sin destino.».
- `tools_web`: `next_redirect` validaba el esquema antes que el tope de
  redirecciones; Python comprueba el tope primero (prueba unitaria ajustada).
- `tools_herdr`: los manuales se servían con CRLF (`include_str!`); Python los
  lee en modo texto y entrega LF (caso `delegate/ok/009-guide`).
- `tools_herdr`: `close` llamaba a `get_last_user_text` una vez; Python dos
  (caso `voice/002-close-notify`).
- `tools_herdr`: no existía la rama de excepción genérica
  («Error inesperado al usar Herdr») ni la captura de errores de callbacks en
  voz; añadidas con `PortError::Unexpected` y `catch_unwind`.
- `tools_skills`, `tools_swarm`, `memory` y `journal`: no aparecieron defectos
  de lógica; lo que faltaba eran las costuras de inyección que el propio
  oráculo usa (intérprete/runner, host/reloj del tablero, hooks del diario).

## Fixtures consumidos
- `parity/fixtures/tools_web.jsonl` (240)
- `parity/fixtures/tools_skills.jsonl` (68)
- `parity/fixtures/tools_swarm.jsonl` (54)
- `parity/fixtures/tools_herdr.jsonl` (393)
- `parity/fixtures/memory.jsonl` (229)
- `parity/fixtures/journal.jsonl` (37)

No se portaron filas sueltas de `parity/test-map.csv`: estos dominios se validan
contra sus fixtures, que es el criterio de esta fase.

## Casos estructurales no aplicables (35)

`tools_web` (22):
- 20 de `DdgsSearchProvider`: prueban el paquete opcional `ddgs` por dentro
  (`_engine` inyectado, `_load_ddgs`, `_new_engine`, normalización de dicts y
  reinicio del motor). El Rust no importa `ddgs`: consulta el backend HTML de
  DuckDuckGo con `reqwest`; no hay motor dinámico que inyectar. Los 8 casos de
  validación de argumentos sí pasan.
- 2 de `_extract_text` (`no-bs4`, `no-lxml`): el Rust usa un parser compilado;
  no puede «faltar» una dependencia opcional en tiempo de ejecución.

`memory` (13):
- 8 del CLI de Python (`build_parser` 4 y `main` 4): no hay binario de consola
  de memoria en el Rust.
- 1 de `module_constants`: el fixture publica las tablas privadas de alias,
  stopwords y nombres; el Rust no las expone (solo `normalize_kind`/`kind_file`).
- 3 de timeout de confirmación (`remember-timeout`, `forget-timeout`,
  `compact-timeout`): `tools_memory` usa `Instant::now()` y no tiene reloj
  inyectable; añadirlo exigiría una costura nueva en `tools_memory`.
- 1 `tools/manager`: el doble de `ConfirmationManager` del oráculo no tiene
  contrapartida directa en la API pública del manager Rust.

## Decisiones (`// DECISION`)
- `tools_web/fetch.rs`: se añaden `\x1c`-`\x1f` al `\s` del cierre de
  `untrusted_content` (ya existía); las nuevas costuras se usan solo en pruebas
  y en producción quedan vacías.
- `tools_web/url.rs`: `pinned_request_target` se conserva con la forma exacta
  del oráculo aunque producción fije la IP con `reqwest::resolve`; se documenta
  en el propio código.
- `tools_herdr/port.rs`: `HerdrPort` es la costura de pruebas; no cambia lo que
  hace el adaptador real.
- `tools_skills.rs`: `Some(None)` en `SkillHooks::interpreter` significa «no hay
  intérprete», como el `None` del Python.
- `journal.rs`: los hooks de reloj y Papelera existen para congelar el tiempo y
  no enviar archivos a la Papelera real durante las pruebas.

## Verificación
- `cargo fmt --all --check`: limpio.
- `cargo clippy -p ez-agent --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-agent`:
  - 162 pruebas unitarias de la librería: verdes.
  - `journal_parity`: 37/37.
  - `memory_parity`: 216/216 (13 documentados como no aplicables).
  - `tools_skills_parity`: 68/68.
  - `tools_swarm_parity`: 54/54.
  - `tools_herdr_parity`: 393/393.
  - `tools_web_parity`: 218/218 (22 documentados como no aplicables).
- Ningún `.rs` supera 600 líneas (mayor: `tests/tools_herdr_parity/fake.rs`,
  384; `src/tools_herdr/delegate.rs`, 585).

## Pendiente o riesgos
- Los 35 casos estructurales de arriba. Para cerrar los 20 de `ddgs` haría falta
  un motor crudo inyectable en `DdgsSearchProvider`; para los 3 de timeout, un
  reloj en `tools_memory`; para los 8 del CLI, un binario de memoria. Ninguno
  afecta al comportamiento de producción actual.
- Los `score` de FTS5 se comparan con tolerancia `f64_rel = 1e-9`; coinciden con
  el SQLite del oráculo en esta máquina, pero podrían variar si cambia la
  versión de SQLite (rusqlite `bundled`).
- Los ids del diario usan `SystemTime::now()` + UUID reales (el fixture los
  normaliza a `<id>`); el orden depende del reloj del sistema, como en Python.
- `tools_web` mantiene la desviación deliberada de usar el backend HTML directo
  en vez del paquete `ddgs`; está documentada en `search.rs`.

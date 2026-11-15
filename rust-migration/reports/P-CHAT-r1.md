# P-CHAT — Paridad de ez-chat (markdown, entrada de texto, traza, voz y almacén)

## Resultado
- Brief/tarea: P-CHAT
- Estado: completado

## Cambios
- `rust/crates/ez-chat/tests/chat_markdown_parity.rs`: 185 casos de `chat_markdown.jsonl` (`clean_target`, `is_local_target`, `parse_inline`, `parse`, `plain_text`), con serialización idéntica de bloques y spans.
- `rust/crates/ez-chat/tests/chat_text_input_parity.rs`: 299 casos de `chat_text_input.jsonl` (`sanitize` y máquina de estados de `TextInput` con instantáneas de texto/cursor/anchor/selección/línea/columna).
- `rust/crates/ez-chat/tests/chat_trace_parity.rs`: 54 casos de `chat_trace.jsonl` (`stamp`, `stamp_for`), deduciendo del propio fixture el desfase de la zona del oráculo.
- `rust/crates/ez-chat/tests/chat_speech_parity.rs`: 45 casos de `chat_speech.jsonl` (`spoken_version`, topes y aviso de pantalla).
- `rust/crates/ez-chat/tests/chat_store_parity.rs`: 85 casos de `chat_store.jsonl` (`ChatStore`, `ChatModel`, `ChatContexts`, `title`, `as_messages`, `seq_of`, `default_chat_path`, constantes) con disco temporal, reloj congelado y entorno por caso.
- `rust/crates/ez-chat/src/markdown.rs`: `is_local_target` ya no quita los `<>` antes de clasificar; Python solo recorta espacios (el `<>` lo quita `clean_target` en la línea de bloque).
- `rust/crates/ez-chat/src/store.rs`: `Source` gana `Other(String)` (Python persiste cualquier `source` sin validarlo en `append`); se inyecta un `Clock` (`with_clock`) para congelar `time.time()` como el oráculo; se expone `VERSION` y se re-exportan `seq_of`, `title` y `title_with`; `rewrite` recibe el reloj como cierre para leerlo solo al escribir la marca `resume`.
- `rust/crates/ez-chat/src/store/records.rs`: `title_with(text, limit)` y `seq_of(data, index)` reproducen `_title`/`_seq_of`; `rewrite` escribe la hora del `resume` con el cierre recibido.
- `rust/crates/ez-chat/src/store/tests.rs`: el `Source` ya no es `Copy`; la prueba existente clona la fuente.
- `rust/crates/ez-chat/src/session.rs`: `ChatModel` recibe el mismo `Clock` (`with_clock`) y lo usa en `add_user`, `add_note`, `begin_assistant`, `finish`, la carga inicial y el cambio de conversación; ajustes de `clone()` por `Source` no `Copy`.
- `rust/crates/ez-chat/src/trace.rs`: se extrae `format_stamp` y se añade `stamp_with_offset`/`stamp_for_with_offset` para comprobar la zona fija del oráculo sin depender de la zona del host; `stamp`/`stamp_for` siguen en hora local.
- `rust-migration/python-quirks.md`: añade `Q-P-CHAT-01` (la zona «UTC» del generador no se aplicó en Windows), `Q-P-CHAT-02` (`contexts/rebuild` omite su `seed` del `input`), `Q-P-CHAT-03` (`append` no valida `source`) y `Q-P-CHAT-04` (`recent(<=0)` devuelve todo).
- `rust-migration/reports/P-CHAT-r1.md`: este informe.

## Decisiones
- `// DECISION` en `store.rs`: el reloj se inyecta (`Clock`, `with_clock`) para reproducir el `time.time()` congelado del oráculo sin tocar el reloj del proceso; en producción se usa `pytime::timestamp`.
- `// DECISION` en `store.rs`: `Source::Other(String)` conserva el texto libre de `append`; `ingest` vuelve a `chat` al recargar, igual que Python.
- `// DECISION` en `trace.rs`: `stamp_with_offset` es una costura de prueba; la aplicación sigue usando la hora local.
- `// DECISION` en `chat_store_parity.rs`: `chat_store/contexts/rebuild` declara su `seed` reconstruido del generador porque el `input` del fixture no lo incluye; no se inventa ningún valor, se copia el de `parity/gen/chat_store.py:1112`.
- Los casos de error que pasan `None`/texto a funciones con firma Rust tipada (`clean_target(None)`, `stamp(None)`, `spoken_version(max_chars=None)`, etc.) se consumen comprobando esa frontera de tipo, como ya hacía `speech_text_parity`; no ejecutan código Rust con un valor imposible.
- `title_with` reproduce el caso límite `limit = 0` de Python (`flat[:-1]`), aunque no hay fixture para él.
- Las tres funciones que el fixture usa como privadas (`_title`, `_seq_of`, `_VERSION`) se hacen públicas; no cambian de comportamiento y no hay otros llamadores afectados.
- No se añadieron dependencias ni se tocó ningún `Cargo.toml`.

## Verificación
- `cargo fmt --all --check`: limpio.
- `cargo clippy -p ez-chat --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-chat`: **84 pruebas, 0 fallos** (79 unitarias existentes + 5 de paridad).
- `cargo check --workspace --all-targets`: limpio (el cambio de `Source` no rompe `ez-app` ni el resto).
- Casos de fixture ejecutados: **668** (185 + 299 + 54 + 45 + 85); 668 comparados y en verde.
- Líneas tras `fmt`: `chat_markdown_parity.rs` 147, `chat_text_input_parity.rs` 144, `chat_trace_parity.rs` 131, `chat_speech_parity.rs` 54, `chat_store_parity.rs` 599; el mayor de `src` es `markdown/blocks.rs` con 527. Ningún `.rs` supera 600.
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`, `remove_dir_all`, `"cmd"`, `"/c"`, `-Command`, `unsafe` ni `.unwrap()` fuera de `tests/` en lo añadido.
- `pytest`: no aplica; el Python de `py-baseline` es el oráculo y los fixtures ya estaban generados (solo lectura).

### Por dominio
| Dominio | Casos | Antes del primer fallo | Defectos Rust corregidos | Hallazgos de Python |
|---|---:|---:|---|---|
| `chat_markdown` | 185 | 185 | `is_local_target` limpiaba `<>` (no lo hace Python; ningún caso lo cubría) | ninguno |
| `chat_text_input` | 299 | 198 | ninguno (el primer fallo fue el adaptador de la prueba con `None`; tras corregirlo, 299/299) | ninguno |
| `chat_trace` | 54 | 0 | ninguno de lógica; faltaba una costura de zona fija para comparar (primer fallo en `stamp/001`) | `Q-P-CHAT-01` |
| `chat_speech` | 45 | 45 | ninguno | ninguno |
| `chat_store` | 85 | 13 | `Source` no representaba fuentes desconocidas; `append` debía persistirlas como Python. Faltaban `title_with`/`seq_of` públicos y el reloj inyectable (primer fallo en `contexts/rebuild` por el seed ausente del fixture) | `Q-P-CHAT-02`, `Q-P-CHAT-03`, `Q-P-CHAT-04` |

### Fixtures consumidos
`parity/fixtures/chat_markdown.jsonl` (185), `chat_text_input.jsonl` (299), `chat_trace.jsonl` (54), `chat_speech.jsonl` (45), `chat_store.jsonl` (85). Total: **668 casos**.

La columna «antes del primer fallo» cuenta los casos que el arnés ejecutó con éxito
antes de detenerse: el fallo de `chat_text_input` fue del adaptador de la prueba
(`None` → `""`), no del Rust; el de `chat_store` fue el `seed` ausente del fixture;
y el de `chat_trace`, la zona horaria. Tras corregir cada causa, los 668 pasan.

### Pruebas portadas (`parity/test-map.csv`)
No hay filas nuevas asignadas a ez-chat: las filas de `tests/test_chat_contexts.py`, `tests/test_chat_input.py`, `tests/test_chat_*.py` están mapeadas a `ez-hud` (UI) y siguen `pendiente`. Estas cinco pruebas cubren el oráculo JSONL de los módulos `voice/chat/*` y no modifican `parity/test-map.csv` (solo lectura).

## Pendiente o riesgos
- `chat_trace` no prueba `stamp`/`stamp_for` contra la zona local real del host: el oráculo quedó en UTC−5 y el Rust compara con ese desfase fijo. La lógica local conserva sus pruebas unitarias; queda `Q-P-CHAT-01` para regenerar el fixture con una zona explícita.
- La prueba de `chat_store` deja `APPDATA`/`XDG_CONFIG_HOME` apuntando a su carpeta temporal durante el resto del proceso. Es un binario de prueba con un solo `#[test]`, así que no afecta a otras pruebas; no se restauran para no superar el límite de 600 líneas.
- `Source` deja de ser `Copy`; ningún consumidor del workspace lo necesitaba, pero un cambio futuro que copie una fuente deberá clonarla.
- Los directorios temporales de las pruebas no se borran (regla de seguridad); son pequeños y únicos por proceso.

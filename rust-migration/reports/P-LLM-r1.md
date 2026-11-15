# P-LLM — Paridad de `ez-llm`

## Resultado
- Brief/tarea: P-LLM
- Estado: completado (760/760 casos de los seis dominios en verde)

## Cambios
- `rust/crates/ez-llm/tests/tool_json_parity.rs`: paridad de `tool_json.py` (169 casos).
- `rust/crates/ez-llm/tests/tool_markup_parity.rs`: paridad de `tool_markup.py` (111 casos).
- `rust/crates/ez-llm/tests/condenser_parity.rs`: paridad de `condenser.py`, con cliente guionizado que reproduce las peticiones del oráculo (41 casos).
- `rust/crates/ez-llm/tests/llm_sse_parity.rs`: paridad de `llm_sse.py`, con servidor HTTP local para los flujos reales de OpenAI/Anthropic (138 casos).
- `rust/crates/ez-llm/tests/llm_request_parity.rs`: paridad de `llm_request.py`, con servidor local que captura cada cuerpo de petición (82 casos).
- `rust/crates/ez-llm/tests/llm_tools_parity.rs` y `tests/support/llm_tools_{client,turn}.rs`: paridad de `llm_tools.py`, con cliente y runtime guionizados (219 casos). Se separó el arnés porque un solo archivo superaba las 600 líneas.
- `rust/crates/ez-llm/src/tool_json.rs`: variantes `*_checked` que devuelven el `AttributeError`/`TypeError` literal de CPython en los cuatro casos de error del oráculo; el comportamiento normal no cambia.
- `rust/crates/ez-llm/src/condenser.rs`: `conclusion_start_values` para reproducir el `TypeError` de una línea que no es texto.
- `rust/crates/ez-llm/src/sse.rs`: `SseReader::new_strict`, el decodificador del SDK de OpenAI no entrega el último bloque sin línea en blanco (el `iter_sse` de Anthropic sí lo entrega).
- `rust/crates/ez-llm/src/openai.rs`: los eventos `choices: []` se entregan (`Chunk::choices_empty`); el texto de error HTTP sigue el formato del SDK (`Error code: N` y, si el cuerpo trae `message`, ` - detalle`); los eventos `error` del flujo producen el `message` del evento (`APIError`), no `Error code: 500 - …`.
- `rust/crates/ez-llm/src/anthropic/stream.rs`: un `content_block`/`error` que no es objeto y un `index` no numérico fallan como el Python (`AttributeError`/`ValueError`), en vez de ignorarse.
- `rust/crates/ez-llm/src/errors.rs`: `ErrorKind` distingue error de flujo (`Api`), objeto inesperado (`Attribute`), número ilegible (`Value`) y JSON del flujo (`Protocol`), y `py_type_name`.
- `rust/crates/ez-llm/src/messenger/{mod,history,request}.rs` y `tool_loop/execution.rs`: visibilidad pública de los helpers que ejercen los casos (`seal_tool_calls`, `user_content`, `notice`, `with_volatile`, `remaining_tools`, `max_tokens_with`, `tools_for`, `base_tools`, `prompt_parts`, `check_cancel`, `apply_deferred_history`, `close_interrupted`…).
- `rust/crates/ez-llm/src/messenger/parity.rs`: hooks `#[doc(hidden)]` de estado que el Python expone como atributos (`_turn_active`, `_deferred_history`, `_pending_notes`, `_last_agent_address`, token/flujo de cancelación); se sacaron de `mod.rs` para respetar las 600 líneas.
- `rust-migration/python-quirks.md`: dos hallazgos nuevos (`Q-P-LLM-01`, `Q-P-LLM-02`).

## Por dominio

| Dominio | Casos | Verde | Defectos de Rust corregidos |
|---|---:|---:|---|
| `tool_json` | 169 | 169 | ninguno de comportamiento; solo faltaban variantes para los errores de tipo dinámico |
| `tool_markup` | 111 | 111 | ninguno |
| `condenser` | 41 | 41 | ninguno (el único fallo inicial fue del arnés, no del módulo) |
| `llm_sse` | 138 | 138 | 5: bloque SSE final incompleto, trozos con `choices: []`, texto de error HTTP, eventos `error` del flujo y validación de tipos en `translate_events` |
| `llm_request` | 82 | 82 | ninguno (el fallo inicial era el servidor de prueba heredando el modo no bloqueante) |
| `llm_tools` | 219 | 219 | ninguno de comportamiento; faltaba exponer helpers de prueba |

Ningún caso queda fallando. No se modificó ningún fixture.

## Fixtures consumidos
- `parity/fixtures/tool_json.jsonl` — 169 casos
- `parity/fixtures/tool_markup.jsonl` — 111 casos
- `parity/fixtures/condenser.jsonl` — 41 casos
- `parity/fixtures/llm_sse.jsonl` — 138 casos
- `parity/fixtures/llm_request.jsonl` — 82 casos
- `parity/fixtures/llm_tools.jsonl` — 219 casos

Total: 760 casos, todos con `assert_eq!(cases_run, cases_in_file)` y el mínimo real de cada archivo.

## Pruebas portadas (`parity/test-map.csv`)
Los fixtures cubren estos archivos de la suite Python; las filas del mapa siguen
marcadas `pendiente` y con crates antiguos, pero la cobertura equivalente queda
en los seis `tests/*_parity.rs` de arriba:
- `tests/test_messenger.py` (59 filas del mapa)
- `tests/test_context_and_json.py` (45 filas)
- `tests/test_anthropic_client.py` (30 filas)
- `tests/test_tool_markup.py` (7 filas)
- `tests/test_audit_watch.py` (4 filas del condensador)

## Decisiones
- `SseReader::new_strict` para OpenAI y `new` para Anthropic: son dos
  comportamientos reales distintos (el SDK descarta el evento final sin línea en
  blanco; `iter_sse` lo entrega). No se unificó para no falsear ninguno.
- `Chunk::choices_empty`: el SDK entrega el trozo con `usage` aunque `choices`
  venga vacío; el `Messenger` lo ignora como antes.
- Los casos `llm_tools/turn` comparan `last_rounds` normalizando `\d+ ms` a
  `0 ms`: el generador congela `time.monotonic` y en Rust ese campo solo mide
  latencia real. Es la única comparación no literal; el resto del texto es
  idéntico.
- `llm_tools/turn/0027` (`turno_agotado`) usa el reloj falso del oráculo; en
  Rust se traduce en un `stream` que tarda 1,1 s antes de abrirse con un plazo
  de 1 s, para que el `LLMTurnTimeout` sea real y no simulado.
- Los hooks `#[doc(hidden)]` de `messenger/parity.rs` existen porque el Python
  ejercita atributos privados (`_turn_active`, `_deferred_history`,
  `_pending_notes`, `_last_agent_address`); sin ellos no se pueden consumir esos
  casos.
- `max_tokens_with` se extrajo de `Messenger::_max_tokens` para poder pasar
  `caps` del oráculo; de paso corrige `max_output == 0` (Python usa
  `caps.max_output or 32_000`).

## Verificación
- `cargo fmt --all --check`: ok
- `cargo clippy -p ez-llm --all-targets -- -D warnings`: ok (sin avisos)
- `cargo test -p ez-llm`: 105 unitarias + 6 paritarias, 760 casos, 0 fallos
- `cargo check --workspace --all-targets`: ok
- `cargo test -p ez-llm` deja además en verde las 105 pruebas unitarias que ya
  existían (una se actualizó: `openai::tests::yields_empty_choice_events_and_stops_at_done`,
  porque el SDK sí entrega el trozo con `choices: []`).

## Pendiente o riesgos
- Desviación del brief: el arnés de `llm_tools` vive en
  `rust/crates/ez-llm/tests/support/llm_tools_{client,turn}.rs` (377 y 179
  líneas). El archivo `tests/llm_tools_parity.rs` solo no cabía en 600 líneas.
  Son módulos del test, no tocan `Cargo.toml`.
- `crates/ez-llm/src/messenger/parity.rs` es un módulo nuevo dentro del crate
  (permitido por el brief: `src/**/*.rs`); solo mueve hooks de estado, sin
  cambiar comportamiento.
- El caso `llm_tools/turn/0027` añade ~1,1 s a la suite; es el precio de no
  simular el reloj en Rust.
- Riesgo residual: los helpers `*_checked` de `tool_json` reproducen los
  `AttributeError` del Python, pero los llamadores de producción usan los
  envoltorios que ignoran el error; con un historial con mensajes que no son
  objetos, Rust deja de procesar el resto en vez de propagar. No hay ningún
  camino de producción que construya ese historial y el oráculo no lo exige.

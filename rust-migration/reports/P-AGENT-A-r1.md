# P-AGENT-A — Paridad de ez-agent, parte A (ronda 1)

## Resultado
- Brief/tarea: P-AGENT-A
- Estado: completado

## Cambios
- `rust/crates/ez-agent/tests/registry_parity.rs`: 68 casos de `parity/fixtures/registry.jsonl` (`schema`, `validate`, `run`, `render_for_llm`, `schemas`, `describe`, `register`, `truncated_note`), incluidos los manejadores `echo`/`contract`/`plain`/`none`/`text`/`fail`/`slow` y el plazo de 0 s.
- `rust/crates/ez-agent/tests/confirm_parity.rs`: 37 casos de `confirm.jsonl`; reproduce la UI dentro de `on_ask`, incluido `second_ask`, tokens atrasados, armado, cancelación y el caso de UI rota con hilo y plazo corto.
- `rust/crates/ez-agent/tests/audit_parity.rs`: 20 casos de `audit.jsonl` con reloj congelado, redacción de contenidos/secretos y las dos rotaciones.
- `rust/crates/ez-agent/tests/system_prompt_parity.rs`: 31 casos de `system_prompt.jsonl`; reproduce `_exercise` (escrituras con mtime creciente, callbacks rotas/contador, `build`/`build_parts`/`reload_if_changed`, BOM, UTF-8 inválido, CRLF y borrado).
- `rust/crates/ez-agent/tests/skills_parity.rs`: 41 casos de `skills.jsonl`; reconstruye el árbol de skills del oráculo (frontmatter válido/roto, CRLF, UTF-8 inválido, binario, colisión local/global, hash, `ensure_default_skills`, `max_catalog` y `refresh`).
- `rust/crates/ez-agent/tests/orchestration_parity.rs`: 46 casos de `orchestration.jsonl` (`parse_steps`, `chain_spec`, `describe_steps`, `flow_task`, `load_guide`, constantes).
- `rust/crates/ez-agent/src/skills.rs`: corrige la clase de error de YAML. Siempre decía `ScannerError`; el oráculo espera `ParserError` para una secuencia de flujo sin cerrar (`name: [unclosed`).
- `rust/crates/ez-agent/src/orchestration.rs`: `load_guide` devolvía el texto incrustado con el CRLF del checkout; Python `Path.read_text` entrega LF. Ahora normaliza CRLF/CR a LF (el único llamador, `tools_herdr::delegate`, acepta `String`).
- `rust/crates/ez-agent/src/audit.rs`: hace públicos `CONTENT_KEYS`/`SECRET_KEY_PARTS` y añade `log_at(entry, ts)`; `log` sigue usando el reloj real.
- `rust/crates/ez-agent/src/registry.rs`: hace público `TRUNCATED_NOTE` para comparar el caso `truncated_note` sin duplicar el texto.
- `rust-migration/python-quirks.md`: añade `Q-P-AGENT-A-01` (el `{{os}}` del prompt depende del host; el oráculo congela `Windows 11`).
- `rust-migration/reports/P-AGENT-A-r1.md`: este informe.

## Decisiones
- `AuditLog::log_at` es una costura de prueba: el fixture congela el reloj en `2025-01-01T00:00:00.000+00:00` y la línea se compara byte a byte; `log` no cambia de comportamiento.
- `load_guide` pasa de `Option<&'static str>` a `Option<String>` para poder normalizar los finales de línea; no hay otros llamadores que dependieran del `&'static str`.
- La clase `ParserError` de PyYAML se aproxima por el mensaje de `serde_yaml` (`expected ',' or ']'` / `'}'`); cualquier otro error se etiqueta `ScannerError`, como antes. `serde_yaml` no expone la clase de PyYAML.
- Dos casos de `registry` no son representables con los tipos Rust: `register` recibe un `dict` (Rust solo acepta `ToolSpec`) y `describe` con un `describe_fn` que devuelve un `dict` (Rust tipa `DescribeFn -> ToolInfo`). La prueba los consume y comprueba el texto de error del contrato Python, pero no ejecutan código Rust; se cuentan en los 68.
- En `audit`, la coerción `int(level)` de Python (2.9 → 2) la aplica el arnés antes de construir `AuditEntry`, cuyo campo es `i64`; `taint`/`result_ok` usan `bool(x)`.
- En `system_prompt`, `file_text` solo se compara en `default-template` (el único caso con `read_file: true`; el payload del fixture no incluye esa bandera).
- Los directorios temporales de las pruebas son únicos por ejecución (`uuid`); no se borran árboles completos (`remove_dir_all` prohibido) y no tocan el HOME real.

## Verificación
- `cargo fmt --all -- --check`: limpio.
- `cargo clippy -p ez-agent --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-agent`: **169 passed, 0 failed** (162 unitarias + `policy_parity` ya existente + 6 pruebas nuevas de paridad).
- Líneas tras `fmt`: `registry_parity.rs` 243, `confirm_parity.rs` 238, `audit_parity.rs` 221, `system_prompt_parity.rs` 237, `skills_parity.rs` 425, `orchestration_parity.rs` 119; `skills.rs` 597, `system_prompt.rs` 557, `confirm.rs` 523, `registry.rs` 469, `orchestration.rs` 269, `audit.rs` 237. Ningún `.rs` supera 600 líneas.
- Sin `todo!`, `unimplemented!`, `#[ignore]`, `TODO`, `println!`, `remove_dir_all`, `"cmd"`, `"/c"`, `-Command`, `unsafe` ni `.unwrap()` fuera de `tests/` en lo añadido.

### Por dominio
| Dominio | Casos | Primer intento | Defectos Rust corregidos | Quirks |
|---|---:|---:|---|---|
| `registry` | 68 | 68 | ninguno | ninguno (2 casos son de tipos, no de comportamiento) |
| `confirm` | 37 | 37 | ninguno | ninguno |
| `audit` | 20 | 20 | ninguno (solo costura `log_at`) | ninguno |
| `system_prompt` | 31 | 31 | ninguno | `Q-P-AGENT-A-01` |
| `skills` | 41 | 40 | `read_skill`: clase YAML `ParserError` | ninguno |
| `orchestration` | 46 | 40 | `load_guide`: CRLF → LF | ninguno |

### Fixtures consumidos
`parity/fixtures/registry.jsonl` (68), `confirm.jsonl` (37), `audit.jsonl` (20), `system_prompt.jsonl` (31), `skills.jsonl` (41), `orchestration.jsonl` (46). Total: **243 casos**.

### Pruebas portadas (`parity/test-map.csv`)
No hay filas de `test-map.csv` para estos fixtures: sus filas son pruebas unitarias de Python (`tests/test_confirm.py`, `tests/test_audit.py`, `tests/test_registry.py`, `tests/test_system_prompt.py`, `tests/test_skills.py`, `tests/test_orchestration.py`) y siguen marcadas `pendiente`. Las seis pruebas de paridad cubren el oráculo JSONL de esta tarea.

## Pendiente o riesgos
- `registry/register/not-a-spec` y `registry/describe/wrong-type` no ejecutan código Rust: son imposibles por tipos. Si se quiere una comprobación dinámica real, habría que añadir una API que acepte valores crudos (no existe en el Python portado); queda anotado, no implementado.
- La clasificación de errores YAML es heurística: un frontmatter roto distinto de `name: [unclosed` podría recibir `ScannerError` donde PyYAML diera otra clase. No hay fixture para más casos.
- En `audit`, `AuditEntry` obliga a convertir `level` a entero antes de llamar; el truncado de Python se reproduce en el arnés, no dentro de `log`.
- Las pruebas dejan directorios únicos en `%TEMP%` (no se borran árboles completos por la regla de seguridad); son pequeños y no afectan al repositorio.

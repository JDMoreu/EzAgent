# TMAP-3 — Mapa de pruebas Python → Rust, grupo 3

## Resultado
- Brief/tarea: TMAP-3
- Estado: completado

## Cambios
- `parity/test-map/TMAP-3.csv`: 315 filas (una por prueba Python del grupo) con crate, prueba o fixture Rust, estado y nota. Verificado que cada referencia existe y que el `python_test_id` coincide con `parity/test-map.csv`.
- `rust/crates/ez-core/src/agent_config.rs`: pruebas de la plantilla y los defaults, valores válidos, los diez tipos inválidos, TOML roto o no UTF-8, secciones que no son tablas, secretos por defecto y `ensure_dirs` con perfil aislado.
- `rust/crates/ez-core/src/permissions.rs`: el aviso de `destructive_always_confirm` incluye «siempre pide confirmación».
- `rust/crates/ez-core/src/paths.rs`: pruebas de escape por enlace simbólico, hermana con prefijo común (`work-evil`/`secreto2`), recorrido con puntos sobre `deny`, raíces vacías, flujo alternativo relativo, `\\?\C:` y los nombres reservados del parametrize.
- `rust/crates/ez-core/src/llm_providers.rs`: coherencia de presets/catálogo, guardado de perfiles con nombre sin temporales ni destruir un archivo corrupto, recorte de valores y `Debug` sin clave, y `fetch_models` con servidor local (Bearer, `x-api-key`, `limit=1000`, errores 401/no-JSON).
- `rust/crates/ez-agent/src/registry/tests.rs`: validación anidada (objetos y listas) y `error_kind`/tamaño exacto en `render_for_llm`.
- `rust/crates/ez-agent/src/runtime/tests.rs`: intenciones de archivos, `move_path` y `undo_last`; oferta de `load_skill` para Excel y Word.
- `rust/crates/ez-agent/src/runtime/tests/execute.rs`: refresco de permisos y skills solo con cambios, `read_roots` que avisa y se conserva, `error_kind` en la auditoría y token real de confirmación en el callback.
- `rust/crates/ez-agent/src/skills/tests.rs`: `refresh(false)` detecta cambios y altas y `force` recarga.
- `rust/crates/ez-agent/src/tools_web/tests.rs`: destino fijado con Host/SNI/IDNA e IP privada bloqueada, y transporte inyectado que conserva la URL original.
- `rust/crates/ez-chat/src/media/tests.rs`: límite de 25 MB, no-imagen y GIF por cabecera.
- `rust/crates/ez-herdr/src/herdr/tests/context.rs` (nuevo, declarado en `tests.rs`): contexto acotado sin esperar el primer snapshot, refresco en vuelo sin hilos duplicados y fallo que descarta el contexto y espera 3 s.
- `rust/crates/ez-hud/src/layout.rs`: orden de los grupos de «system», filas de «files» y pertenencia a `ACTION_KEYS`/`LABELS`.
- `rust/crates/ez-llm/src/anthropic/tests.rs`: dos streams reutilizan un solo `reqwest::Client`, cabeceras `x-api-key`/`anthropic-version` y `close` idempotente.

## Decisiones
- El mapa va por conducta, no por nombre: las filas `paridad` citan el caso de fixture concreto que ejerce la misma función y entrada, y la prueba que lo consume.
- `rust_crate` corregido cuando la conducta vive en otro crate: `test_chat_media.py` → `ez-chat` (no `ez-hud`) y `test_handoff_packet.py` → `ez-agent` (no `ez-app`/`ez-core`).
- Cuatro filas `no_aplica` por costuras de prueba de Python, no por falta de conducta: despacho de acciones de archivo y apertura de `permissions.toml` (la apertura usa Explorer/Win32 y no es interceptable), `reveal_file` sin shell (en Rust `Command` no tiene shell: el espía de `subprocess.Popen` no aplica) y `_load_ddgs` (el puerto Rust scrapea con `reqwest`; no existe el motor `ddgs`).
- Mapeos débiles anotados en la propia fila: el texto «+16 ajustes»/«16 visibles» lo pinta `draw_advanced_note` y las pruebas de dibujo no capturan texto (se verifica `ADVANCED_ROWS = 16`); `media.load_image` valida cabecera y dimensiones y el HUD decodifica a RGBA (DECISION de `media.rs`).
- No se tocó código de producción. Todas las pruebas nuevas están en módulos `tests` existentes o en `ez-herdr/src/herdr/tests/context.rs`.
- No hay `#[ignore]`: ninguna prueba nueva reveló una diferencia de conducta Python/Rust.

## Verificación
- `cargo fmt --all`: ok.
- `cargo clippy -p ez-core -p ez-agent -p ez-llm -p ez-chat -p ez-hud -p ez-herdr --all-targets -- -D warnings`: ok.
- `cargo test -p ez-core -p ez-agent -p ez-llm -p ez-chat -p ez-hud -p ez-herdr`: PENDING.
- pytest: no aplica; el Python es el oráculo de solo lectura.
- Fixtures consumidos (filas del CSV / casos que ejercen):
  - `llm_providers.jsonl` 228 casos (75 filas).
  - `agent_paths.jsonl` 44 casos (19 filas).
  - `handoff_packet.jsonl` 129 casos (16 filas).
  - `llm_request.jsonl` 82 casos (14 filas).
  - `paths_globs.jsonl` 34 casos (10 filas).
  - `llm_sse.jsonl` 138 casos (7 filas).
  - `hud_settings.jsonl` 1175 casos (1 fila; caso `section_groups/system`).
  - `swarm.jsonl` 644 casos (1 fila; 49 casos `service.handle`).
- Conteos por estado: `paridad` 143, `portada` 101, `nueva` 67, `no_aplica` 4; total 315.
- Por archivo Python (filas): advanced 6, agent_config 20, anthropic_client 30, audit_provider 24, audit_tools 53, chat_media 33, handoff_packet 16, herdr_context 3, llm_providers 78, paths 40, swarm_service 12.
- El detalle de cada fila (prueba o fixture que la cubre) está en `parity/test-map/TMAP-3.csv`; no queda ninguna `pendiente`.

## Pendiente o riesgos
- Las cuatro filas `no_aplica` son límites de las costuras de prueba (apertura con Explorer, spy de `Popen`, motor `ddgs`), no conducta sin portar.
- La prueba del enlace simbólico se salta si Windows no permite crearlo (misma condición que el `pytest.skip` de Python); en esta máquina se creó y pasó.
- Las pruebas de `fetch_models` y de streams de Anthropic levantan un `TcpListener` en `127.0.0.1`; no usan red externa.

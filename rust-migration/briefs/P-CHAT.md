# P-CHAT — Paridad de ez-chat (markdown, entrada de texto, traza, voz y almacén)
**Antes de nada lee `rust-migration/AGENTS.rust.md` y `rust-migration/briefs/_COMMON.md`.**

## Dominios (en este orden)
- `chat_markdown` → `parity/fixtures/chat_markdown.jsonl` → `rust/crates/ez-chat/tests/chat_markdown_parity.rs`
- `chat_text_input` → `parity/fixtures/chat_text_input.jsonl` → `rust/crates/ez-chat/tests/chat_text_input_parity.rs`
- `chat_trace` → `parity/fixtures/chat_trace.jsonl` → `rust/crates/ez-chat/tests/chat_trace_parity.rs`
- `chat_speech` → `parity/fixtures/chat_speech.jsonl` → `rust/crates/ez-chat/tests/chat_speech_parity.rs`
- `chat_store` → `parity/fixtures/chat_store.jsonl` → `rust/crates/ez-chat/tests/chat_store_parity.rs`

## Fuente Python (la especificación)
- src/ezagent/voice/chat/*.py
- Rust a probar: `rust/crates/ez-chat/src/`

## Archivos que puedes tocar (solo estos)
- rust/crates/ez-chat/src/**/*.rs (los módulos que haga falta corregir)
- `rust/crates/ez-chat/tests/<dominio>_parity.rs` (nuevos), `rust-migration/reports/P-CHAT-r1.md`, `rust-migration/python-quirks.md` (solo añadir al final)

## Notas específicas
- `chat_trace` depende de la zona horaria de la máquina que generó los fixtures (UTC fijada por el oráculo): fija `TZ`/la zona en tu prueba si el Rust la lee del entorno. `chat_store` compara volcados de SQLite (`history.db`).

## Qué es una prueba de paridad (léelo entero)

El código Rust de este dominio **ya está escrito** y compila; lo que falta es **demostrar que se comporta igual que el Python**. El oráculo (`parity/fixtures/<dominio>.jsonl`) lo generó el Python real de `py-baseline`: cada línea es `{case, fn, input, output, error}`. Tu trabajo, por dominio:

1. **Escribe `rust/crates/<crate>/tests/<dominio>_parity.rs`**: carga el `.jsonl` con `ez_parity::fixtures::load` (ruta `../../../parity/fixtures/<dominio>.jsonl`, relativa a `CARGO_MANIFEST_DIR`; el crate necesita `ez-parity` en `[dev-dependencies]`, **ya está declarado** en los crates que lo usan; si falta en el tuyo, anótalo en el informe y avísame: no toques `Cargo.toml`), ejecuta cada caso contra la función Rust equivalente (`fn` indica cuál; `input` son sus argumentos) y compara con `output`/`error`. Modelos a imitar: `rust/crates/ez-core/tests/llm_providers_parity.rs` y `config_parity.rs` (entorno y directorio temporal por caso, marcadores `<tmp>`), `rust/crates/ez-core/tests/pytime_parity.rs`.
2. **Todos los casos del archivo deben consumirse**: la prueba acaba con `assert_eq!(cases_run, cases_in_file)` y un `assert!(cases_in_file >= N)` con el número real; un caso sin implementar es un **fallo**, no un salto. Una sola función `#[test]` por dominio si los casos tocan el entorno, el directorio actual o variables globales (se ejecutan en serie dentro de ella); si no, varias están bien.
3. **Comparación:** JSON igual **estructuralmente** (cuida el orden de claves solo si el fixture lo exige: mira `output.text` de archivos escritos, que es byte a byte); textos de error **idénticos al carácter**; flotantes con la tolerancia de `parity/thresholds.toml` (`f64_rel = 1e-9`, `f32_rel = 1e-5`; enteros exactos); cuadros PNG con `ez_parity::image` y los umbrales de ese archivo (`pixels.shapes` sin texto, `pixels.text` con SSIM ≥ 0,995); audio con las tolerancias de `[audio.*]`. Si el fixture trae rutas con `<tmp>`, sustitúyelas por tu directorio temporal antes de comparar.
4. **Cuando un caso falla, el Rust está mal y tú lo arreglas**: el Python es la especificación y los fixtures son de **solo lectura**. Corrige el módulo Rust (solo dentro de los archivos que te asigna el brief) para que dé exactamente lo del oráculo. Si el Python hace algo absurdo (un error suyo), **se reproduce igual** y se anota en `rust-migration/python-quirks.md` (una entrada por hallazgo: archivo, línea, efecto, cómo se reproduce en Rust, recomendación). Nunca «arregles» el fixture ni relajes la comparación para que pase; si crees que un caso del oráculo es incorrecto (no determinista, depende de tu máquina), repórtalo con el caso exacto en el informe.
5. **Una prueba por dominio, y los dominios por orden**: no pases al siguiente hasta que el anterior esté al 100 %. Si te quedas sin presupuesto, entrega lo terminado y marca el resto como `parcial` con el **número de casos que fallan y la causa** de cada grupo (esto es información valiosa, no un fracaso).

## Reglas (las de siempre)

- Alcance: **solo** los archivos de tu lista (los de tu crate indicados, más tus `tests/*_parity.rs`, el informe y, si hay hallazgos, `rust-migration/python-quirks.md` añadiendo al final). `src/`, `tests/` de Python, `parity/` y `Cargo.toml` son de solo lectura. Si un arreglo exige tocar otro crate, **no lo hagas**: descríbelo en el informe (archivo, función, qué cambiar).
- `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md` siguen vigentes (patrones prohibidos, 600 líneas por módulo —los `tests/*_parity.rs` también—, `cargo fmt`, `cargo clippy -p <crate> --all-targets -- -D warnings`, sin `.unwrap()` fuera de `tests/`).
- Ejecuta desde `rust/`: `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>` (todo en verde, incluidas las pruebas que ya existían). Para obtener la verdad del Python si dudas, usa `C:/Users/JDMoreu/Documents/EzAgent/.venv/Scripts/python.exe -I -c "..."`.
- Entrega: un commit en tu rama y el informe `rust-migration/reports/<ID>-r1.md` (formato de AGENTS.md §6) con, **por dominio**: casos totales, casos que pasaban al primer intento, **defectos de Rust corregidos** (qué función y qué estaba mal), hallazgos de Python anotados y casos que siguen fallando (si los hay).

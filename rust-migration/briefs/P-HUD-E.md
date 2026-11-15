# P-HUD-E — Dejar `hud_chat_parity` al 100 % (cuadros del chat: mensajes, vista, panel y morph)
**Antes de nada lee `rust-migration/AGENTS.rust.md` y `rust-migration/briefs/_COMMON.md`.**

## Dominios (en este orden)
- `hud_chat` → `parity/fixtures/hud_chat.jsonl` → `rust/crates/ez-hud/tests/hud_chat_parity.rs`

## Fuente Python (la especificación)
- src/ezagent/voice/hud/chat_*.py y src/ezagent/voice/hud/chat/ (render de mensajes: texto enriquecido, código, tablas, citas, listas, encabezados, imágenes y documentos; fuentes y estilos)
- `parity/gen/hud_chat.py` (cómo se produjo cada cuadro, qué fuentes y escala usó)
- Rust a corregir: `rust/crates/ez-hud/src/{paint/chat,model/chat,win/chat}*` y la prueba `rust/crates/ez-hud/tests/hud_chat_parity.rs` + `hud_chat_parity/*`
- Cuadros dorados: `parity/golden/hud/chat/`

## Archivos que puedes tocar (solo estos)
- rust/crates/ez-hud/src/paint/chat*
- rust/crates/ez-hud/src/model/chat*
- rust/crates/ez-hud/src/win/chat*
- rust/crates/ez-hud/tests/hud_chat_parity.rs y rust/crates/ez-hud/tests/hud_chat_parity/** (**sí puedes editar el arnés**, no los fixtures ni los PNG)
- (no toques `ez-gfx`; `model/menu*`, `paint/menu*` y `paint/settings_window*` los trabaja P-HUD-D en paralelo)
- `rust/crates/ez-hud/tests/<dominio>_parity.rs` (nuevos), `rust-migration/reports/P-HUD-E-r1.md`, `rust-migration/python-quirks.md` (solo añadir al final)

## Notas específicas
- **Contexto:** P-HUD-C dejó `hud_chat` en 182/221 (los 158 casos sin PNG pasan); P-GFX-TEXT demostró que el texto de `ez-gfx` es idéntico a Pillow **con FreeType 2.14.3**, y el workspace ya lo compila (`rust/vendor/freetype-sys`). Aun así, en `rust/main` fallan **39 cuadros** con diferencias grandes (no 1 px): `render/*` (code, custom-lavanda, doc-denied/missing/other/pdf/preview, headings, image-denied/missing/ok/web, inline, lists, mixed, quote-rule, scale-150, table, table-narrow, unicode: SSIM 0,91–0,99, p995 hasta 240), `panel/*` (conversation, confirm-sending, custom-lavanda, input-multiline, long-scrolled, status-listening, viewing-previous: SSIM ≈ 0,98–0,99) y `morph/*` (open/close: SSIM ≈ 0,986). Reprodúcelos con `cd rust && cargo test -p ez-hud --test hud_chat_parity` (tarda ~70 s; falla al final con la tabla completa).
- **Qué buscar** (de más a menos probable): (a) **fuentes y estilos** de los spans enriquecidos —negrita, cursiva, monoespaciada del código, emoji/Unicode con fuente de respaldo— frente a las que carga el Python (revisa qué archivos de fuente usa `fonts`/`text` del HUD para cada estilo y cómo elige el respaldo); (b) métricas verticales de línea y alturas de bloque (code, quote-rule, table, headings: la altura o el margen pueden estar desplazados); (c) orden de pintura y fondos de tarjeta (documentos, imágenes, tablas); (d) el degradado/sombra del panel y del morph. Para cada familia, compara el cuadro producido con el dorado (puedes volcar el PNG producido a una carpeta temporal desde tu prueba y mirarlo con la herramienta de lectura de imágenes) y localiza la primera diferencia estructural.
- **Meta:** `cargo test -p ez-hud --test hud_chat_parity` en verde con los umbrales de `parity/thresholds.toml` (sin relajar ninguno) y `cargo test -p ez-hud` completo en verde. Si alguna causa está en `ez-gfx` (rasterizado, composición), descríbela con el caso exacto en el informe en vez de tocarla. Rarezas del Python → `python-quirks.md`.
- Informe: tabla por familia (mensajes simples, código, tablas, citas, listas, encabezados, imágenes, documentos, vista, panel, morph) con totales, dentro de umbral, peor diferencia y causa.

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

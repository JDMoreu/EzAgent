# P-GFX-TEXT — Texto idéntico a Pillow: altura de la máscara con acentos y antialias de glifos (ez-gfx)
**Antes de nada lee `rust-migration/AGENTS.rust.md` y `rust-migration/briefs/_COMMON.md`.**

## Dominios (en este orden)


## Fuente Python (la especificación)
- Pillow/FreeType del `.venv` (`C:/Users/JDMoreu/Documents/EzAgent/.venv/Scripts/python.exe -I -c ...`): `ImageFont.truetype(...).getbbox`, `.getmask2`, `ImageDraw.text`; y cómo los usa `src/ezagent/voice/hud/` (`text`/`fonts`, `_fit_text`, `FS_BODY`, `FS_ROW`)
- Rust a corregir: `rust/crates/ez-gfx/src/text*` (y `ez-gfx/src/freetype*` si existe); informe de F2-10 en `rust-migration/reports/` y el apartado de FreeType de `rust-migration/python-quirks.md`

## Archivos que puedes tocar (solo estos)
- rust/crates/ez-gfx/src/**/*.rs
- rust/crates/ez-gfx/tests/text_mask_parity.rs (nuevo) y su carpeta de apoyo
- `rust/crates/ez-gfx/tests/<dominio>_parity.rs` (nuevos), `rust-migration/reports/P-GFX-TEXT-r1.md`, `rust-migration/python-quirks.md` (solo añadir al final)

## Notas específicas
- **Problema (informe de P-HUD-B, `rust-migration/reports/P-HUD-B-r1.md`, sección «Pendiente»):** (1) la máscara de texto `FS_BODY` de `ez-gfx` mide **1 px más de alto** que la de Pillow en textos con acento (`"Automático"` 10 vs 9, `"Según el sistema"` 12 vs 11), por lo que el texto del menú baja 1 px (SSIM ≈ 0,9949 < 0,995); `FS_ROW` sí coincide. (2) en la vista rápida el antialias de los glifos da `p995 = 3–4` (máximo 8) frente al `p995 ≤ 2` de `[pixels.text]` en `parity/thresholds.toml`.
- **Qué hacer:** averigua cómo Pillow calcula la caja de tinta/altura de la máscara (`getbbox`/`getmask2`, `anchor`, `ascent`/`descent`, `FT_LOAD_*`, hinting, `layout_engine`, redondeo de la métrica del acento) y haz que `ez-gfx` produzca **la misma altura y los mismos bytes de cobertura**. Para tener verdad: escribe un script Python (con `-I`) que, para el tamaño y fuente de `FS_BODY`, `FS_ROW` y los demás tamaños que usa el HUD, imprima `bbox`, tamaño y hash de la máscara de un conjunto amplio de cadenas (acentos, ñ, ¿¡, mayúsculas acentuadas, símbolos, CJK/emoji que el HUD pinte si los hay), y compáralos con Rust en `tests/text_mask_parity.rs` (los valores esperados se **generan al vuelo ejecutando Python** si el intérprete existe, o se guardan como constantes en el propio archivo de prueba: no toques `parity/`).
- **Verificación de aceptación:** `cargo test -p ez-hud --test hud_menu_parity --test hud_peek_parity` (pruebas de P-HUD-B ya integradas; **no las modifiques**) debe pasar los 170 cuadros de menú y los 32 de la vista rápida con los umbrales de `thresholds.toml`, y las pruebas ya existentes de `hud_bar_parity`, `ez-gfx` y `ez-hud` no deben regresar (`cargo test -p ez-gfx -p ez-hud`). Si la corrección necesita tocar `ez-hud` (solo si la causa está allí), descríbela en el informe en vez de hacerla.
- No hay dominios de fixtures que consumir en esta tarea: ignora las instrucciones genéricas de «una prueba por dominio» de abajo, salvo en lo que aplique a `text_mask_parity.rs`.

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

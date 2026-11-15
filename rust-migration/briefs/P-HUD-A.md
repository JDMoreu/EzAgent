# P-HUD-A — Paridad de ez-hud, parte A (geometría, modelo de la barra y pintura de la barra con cuadros dorados)
**Antes de nada lee `rust-migration/AGENTS.rust.md` y `rust-migration/briefs/_COMMON.md`.**

## Dominios (en este orden)
- `hud_geometry` → `parity/fixtures/hud_geometry.jsonl` → `rust/crates/ez-hud/tests/hud_geometry_parity.rs`
- `hud_drop_model` → `parity/fixtures/hud_drop_model.jsonl` → `rust/crates/ez-hud/tests/hud_drop_model_parity.rs`
- `hud_bar` → `parity/fixtures/hud_bar.jsonl` → `rust/crates/ez-hud/tests/hud_bar_parity.rs`

## Fuente Python (la especificación)
- src/ezagent/voice/hud/ (tokens, anim, bar_motion, bar_theme, theme_accent, a11y, chat_geometry, menu_layout, layered pacing, drop, flow, marks, state)
- Rust a probar: `rust/crates/ez-hud/src/` y `rust/crates/ez-gfx/src/`
- Cuadros dorados: `parity/golden/hud/{bar,buttons,fluid,orbit,text}/` con `ez_parity::image` y `parity/thresholds.toml`

## Archivos que puedes tocar (solo estos)
- rust/crates/ez-hud/src/**/*.rs (los módulos que haga falta corregir)
- rust/crates/ez-gfx/src/**/*.rs (los módulos que haga falta corregir)
- `rust/crates/ez-hud/tests/<dominio>_parity.rs` (nuevos), `rust-migration/reports/P-HUD-A-r1.md`, `rust-migration/python-quirks.md` (solo añadir al final)

## Notas específicas
- `hud_bar` tiene **298 cuadros PNG** (barra 146, botones 50, órbita 50, Marea 40, texto 12) más curvas numéricas: reproduce cada cuadro con el estado del `.json` de entrada y compáralo con el umbral que corresponda (formas: ≤1 nivel por canal en el 99,9 % de los píxeles y ≤3 en cualquiera; texto: SSIM ≥ 0,995). **Aquí se decide si el HUD es igual**: cuando un cuadro no pasa, localiza la causa (rasterizado, composición, orden de operaciones en `f32`, redondeo `round_ties_even`) y arréglala en `ez-hud`/`ez-gfx`, y anota qué estadística falla (diferencia máxima, % de píxeles fuera). Si el texto de FreeType difiere en cobertura subpíxel (F2-10 lo documentó), investiga si Pillow aplica algún ajuste (por ejemplo, `layout_engine` o `FT_LOAD_*`) que no se replicó.
- Informe: tabla por vista (barra, botones, órbita, Marea, texto) con cuadros totales, cuadros dentro de umbral, peor diferencia y causa de los que fallan.

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

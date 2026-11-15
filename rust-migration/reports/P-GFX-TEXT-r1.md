# P-GFX-TEXT — Texto idéntico a Pillow: altura de la máscara y antialias de glifos

## Resultado
- Brief/tarea: P-GFX-TEXT
- Estado: **parcial**. El puerto de `ez-gfx::text` es correcto; el bloqueo es la
  versión de FreeType que fija el workspace, fuera del alcance del brief
  (`Cargo.toml` es de solo lectura y F2-10 es su dueño).

Lo demostrado:
- Con **FreeType 2.14.3** (el que enlaza Pillow 13.0.0 del `.venv`), las **181**
  máscaras de la tabla nueva coinciden con Pillow **byte a byte**, y
  `hud_peek_parity` pasa **154/154**.
- Con la dependencia actual (`freetype-sys 0.23.0` compila **FreeType 2.13.2**),
  el intérprete de TrueType mueve y engorda los acentos compuestos: la máscara
  de `FS_BODY` mide 1 px más de alto en `"Automático"` (10 vs 9) y
  `"Según el sistema"` (12 vs 11), y la cobertura de los glifos acentuados no
  coincide. Eso explica las diferencias de P-HUD-B. **No es un defecto del
  puerto**: con 2.14.3 el mismo código da los bytes exactos.
- `hud_menu_parity` con FreeType 2.14.3 pasa **93/170** cuadros. Los **77**
  restantes fallan por un hueco del arnés de P-HUD-B, no por el texto: los
  casos `state: "section"` traen `title`/`groups` pero no `section`, y
  `support.rs:523` solo reconstruye la página desde `input.section`; el cuadro
  producido para `section-system-dark-100@1200` muestra la página de
  apariencia en vez de «Sistema».

## Diagnóstico (cómo mide y rasteriza Pillow)
- El `.venv` usa **Pillow 13.0.0** con **FreeType 2.14.3** y
  `PIL._imagingft.HAVE_RAQM == False`, así que el layout es el **BASIC**
  (`text_layout_fallback`): avances de `FT_Load_Glyph(FT_LOAD_DEFAULT)` más
  kerning de `FT_Get_Kerning` redondeado con `PIXEL`, exactamente lo que porta
  `text/font.rs`.
- `text.py` crea una imagen `L` de `ceil(avance) + 2*pad` × `line_h + 2*pad` y
  dibuja con `ImageDraw.text((pad, pad), anchor="la")`. `getmask2` usa
  `FT_LOAD_DEFAULT | FT_LOAD_RENDER`; la altura que devuelve `text_size` es la
  del recorte a píxeles no cero, no la de la línea tipográfica. El ancla `la`
  deja la línea base en `pad + ascent`, con `ascent`/`descent` de
  `getmetrics()` = `PIXEL(size->metrics.ascender/descender)`. Todo eso coincide
  ya en el puerto (los 181 casos pasan con 2.14.3).
- `getbbox` usa `FT_Glyph_Get_CBox(FT_GLYPH_BBOX_PIXELS)` sobre el contorno
  **hintado**. La diferencia está ahí: el intérprete de TrueType de 2.13.2 y el
  de 2.14.3 producen contornos distintos para los compuestos acentuados. Para
  U+00B4 a 13 px, 2.14.3 devuelve los puntos `y` 505/572 y una máscara
  `[17,192,49] [10,15,0]`; 2.13.2 devuelve 512/630 y
  `[0,164,83] [48,202,2]`. No es el ancla, ni `ascent`, ni el redondeo del
  avance: es el resultado del bytecode de la fuente.
- Se probaron `FT_LOAD_DEFAULT`, `NO_HINTING`, `TARGET_LIGHT`, `TARGET_MONO`,
  `TARGET_LCD(_V)`, `FORCE_AUTOHINT`, `NO_AUTOHINT` y `PEDANTIC`, y
  `FREETYPE_PROPERTIES=truetype:interpreter-version=35`: ninguno reproduce la
  máscara de 2.14.3. La única variable que la reproduce es la versión de
  FreeType.

## Cambios
- `rust/crates/ez-gfx/tests/text_mask_parity.rs` (nuevo): 181 casos generados
  con Pillow/FreeType 2.14.3 (7 estilos del HUD y de la vista rápida × escalas
  1,0/1,25/1,5/2,0 × textos con acentos, `ñ`, `ü`, `¿¡`, mayúsculas acentuadas,
  elipsis, símbolos y cadenas ASCII). Compara tamaño lógico, tamaño físico y
  hash FNV-1a de la cobertura leída a través de `draw_text` (valida también la
  composición RGBA). Incluye una sonda de rasterizador con U+00B4 y los casos
  de texto vacío y solo espacios.
- `rust-migration/python-quirks.md`: añade `Q-PGFXTEXT-01` (la máscara depende
  del FreeType que empaquete la rueda de Pillow; no está fijado por el proyecto).
- `rust-migration/reports/P-GFX-TEXT-r1.md`: este informe.
- **No se modificó ningún `.rs` de `ez-gfx/src`**: no hay defecto que corregir
  allí. Tampoco se tocaron `Cargo.toml`, `Cargo.lock`, `ez-hud`, `parity/` ni el
  Python.

## Decisiones
- **Sin cambios en `ez-gfx/src`**: la experimentación con FreeType 2.14.3 da
  181/181 máscaras idénticas; cualquier cambio en el puerto (recorte, anclaje,
  tracking) rompería esa igualdad. El defecto está en el binario nativo que
  enlaza `freetype-sys 0.23.0` (FreeType 2.13.2).
- **Sonda U+00B4 en la prueba**: U+00B4 a `FS_BODY` 1,0 produce
  `[17,192,49] [10,15,0]` con 2.14.3 y `[0,164,83] [48,202,2]` con 2.13.2. La
  prueba detecta cuál está enlazado: con 2.14 exige bytes exactos; con 2.13
  comprueba avance exacto, alto ±1 px y ancho de máscara exacto, y deja
  constancia de por qué no puede comparar la cobertura. Al actualizar la
  dependencia, la prueba pasa sola al modo estricto sin tocar la tabla.
- **Se descartó un parche de compatibilidad en `ez-gfx`** (recortar o desplazar
  el acento 1 px): con 2.14.3 sobra y con 2.13.2 no reproduce la cobertura de
  Pillow (el contorno entero está hintado distinto: puntos `y` 505/572 frente a
  512/630 en el glifo agudo). Sería un parche que rompe la paridad al actualizar.
- **No se tocó el arnés de P-HUD-B** (`hud_menu_parity/support.rs`) por alcance;
  el arreglo se describe abajo.

## Verificación
- `cargo fmt --all`: limpio.
- `cargo clippy -p ez-gfx --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-gfx`: **77 unitarias + 3 de paridad, 0 fallos**.
- Prueba temporal con FreeType 2.14.3 (sin cambios en el repo): se compiló
  FreeType 2.14.3 aparte y se enlazó con
  `cargo --config 'patch.crates-io.freetype-sys.path=…'`:
  - `text_mask_parity`: **181/181 exactas** (rama estricta).
  - `hud_peek_parity`: **154/154** (32 cuadros que fallaban con 2.13.2).
  - `hud_bar_parity`: **361/361** (sin regresión).
  - `hud_menu_parity`: **93/170** cuadros; los 44 que dependían del texto pasan
    a estar dentro de umbral. Los 77 restantes siguen fallando por el arnés
    (ver «Pendiente»).
- Con la dependencia actual: `hud_peek_parity` falla en el primer cuadro
  (`compose-25-above`, max 8, p995 4, SSIM 0,99999) y `hud_menu_parity` deja
  121/170 cuadros fuera de umbral, como ya reportó P-HUD-B. No es una
  regresión de esta tarea: sin cambios en `ez-gfx/src`, el comportamiento es el
  mismo que antes del brief.

### Por dominio
| Dominio | Casos | Pasan al primer intento | Defectos de Rust corregidos | Hallazgos de Python |
|---|---:|---:|---|---|
| `gfx_text` (nuevo, `text_mask_parity`) | 181 | 181 con FreeType 2.14.3; 181 con invariantes de maquetación con 2.13.2 | 0 (el puerto ya era correcto) | Q-PGFXTEXT-01 |

### Fixtures y pruebas portadas
- Fixtures consumidos: **ninguno** (el brief dice que no hay dominio de fixtures
  para esta tarea; `parity/fixtures/gfx_text.jsonl` no existe).
- Filas de `parity/test-map.csv`: ninguna de texto.
- La verdad se generó con Pillow 13.0.0 / FreeType 2.14.3 ejecutando
  `src/ezagent/voice/hud/text.py` con el intérprete del `.venv`; los valores
  quedan incrustados como constantes para que `cargo test` no dependa del
  `.venv`.

## Pendiente o riesgos
- **Bloqueo de la dependencia nativa (F2-10, dueño de `Cargo.toml`)**: hay que
  enlazar FreeType **≥ 2.14.0**. `freetype-sys 0.23.0` es la última versión
  publicada (octubre de 2024) y compila 2.13.2; no existe una 0.23.x posterior.
  Las opciones son (a) un `freetype-sys` vendorizado/parcheado que compile
  2.14.3, o (b) otra dependencia que enlace esa versión. Ambas exigen cambiar
  `Cargo.toml`; este brief lo prohíbe. Con ese cambio, `text_mask_parity` pasa
  a estricta y `hud_peek` queda en 154/154 sin tocar nada más.
- **P-HUD-B, 77 cuadros de menú**: `hud_menu_parity/support.rs:523` solo
  reconstruye la página desde `input.section`. Los fixtures `section-*`,
  `advanced-*` y `secret-*` traen `state`, `title` y `groups` (sin `section`),
  así que el arnés dibuja la página por defecto. Arreglo: si `state ==
  "section"`, usar `input.title` y `input.groups` (o derivar la sección del
  título) antes de `menu.set_groups`. Es un cambio en una prueba de P-HUD-B,
  fuera del alcance de este brief.
- **Mientras no se actualice FreeType**, 44 cuadros de menú y 32 de la vista
  rápida no pueden pasar los umbrales; el texto con acentos se dibuja 1 px más
  arriba y con cobertura distinta. No hay corrupción ni bloqueo de UI.
- La tabla de 181 casos cubre los estilos y textos que pinta el HUD, no todo
  Unicode; los glifos CJK/emoji no los usa el HUD y Geist no los incluye.

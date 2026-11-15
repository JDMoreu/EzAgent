# P-HUD-E — `hud_chat` al 100 % (ronda 1)

## Resultado
- Brief/tarea: P-HUD-E
- Estado: completado

`cargo test -p ez-hud --test hud_chat_parity` en verde: **221/221 casos** y **63/63
cuadros** dentro de los umbrales de `parity/thresholds.toml` (`pixels.text`, SSIM
≥ 0,995 y p995 ≤ 2), sin relajar ninguno. Al empezar fallaban **39** cuadros.

## Cambios
- `rust/crates/ez-hud/src/paint/chat/base.rs`: `draw_text_at` ya no compone un
  *tile* float premultiplicado (que destruía el color de la tinta con cobertura
  baja y mezclaba como «source-over» recto); ahora construye la máscara de
  cobertura y la mezcla con la fórmula de `ImageDraw.text` de Pillow; ancla el
  texto por el «pen» (nuevas `ink_bearing`/`ink_top`) en vez de por la tinta
  recortada; redondea la línea base antes del desplazamiento entero; y la cursiva
  usa el `Image.transform(AFFINE, BICUBIC)` real (`shear_rows`/`cubic_kernel`) en
  lugar de un sesgo bilineal propio.
- `rust/crates/ez-hud/src/paint/chat/tests.rs`: tres pruebas nuevas (mezcla de
  `draw_bitmap`, sesgo afín contra un vector de Pillow y »bearing»/»ink top»).

No se tocó el arnés (`hud_chat_parity*`), ni los fixtures, ni los PNG, ni
`ez-gfx`, ni `Cargo.toml`.

## Decisiones
- `// DECISION` en `base.rs`: el blanco al rasterizar el texto solo sirve para
  obtener la cobertura; el color lo pone `blend_mask` (así el color no se pierde
  con coberturas de 1-5/255).
- Sondas de métricas en vez de tocar `ez-gfx`: `ink_bearing` compara el ancho de
  tinta de `"T"+texto` con el del texto (T es la tinta más a la izquierda y su
  «bearing» es 0 en los once estilos a 100/125/150/200 %), y `ink_top` compara
  altos contra `"Á"+texto` (Á es más alto que cualquier glifo del chat y no baja
  del renglón). Ambas llevan guarda: si el resultado se sale del rango que puede
  explicar la caja del contorno, se usa la medida de `baseline_top`.
- Coste asumido: dos `text_size` extra por tirada de texto (cacheadas por
  `ez-gfx`); el chat se pinta una vez por mensaje.

## Verificación
- `cargo fmt --all`: ok.
- `cargo clippy -p ez-hud --all-targets -- -D warnings`: ok.
- `cargo test -p ez-hud`: 488 pruebas de unidad ok + `hud_bar`, `hud_chat`,
  `hud_drop_model`, `hud_geometry`, `hud_menu_hit` en verde. **`hud_menu_parity`
  falla y ya fallaba antes de este cambio** (`git stash` + `cargo test -p ez-hud
  --test hud_menu_parity`): es el dominio de P-HUD-D.
- Fixtures consumidos: `parity/fixtures/hud_chat.jsonl` (221 casos, 63 PNG) y
  `parity/golden/hud/chat/*.png`. `parity/test-map.csv` no tiene filas de este
  dominio (todas las de `test_chat*` siguen `pendiente` y apuntan a pruebas de
  comportamiento, no a esta paridad).

### Por familia (cuadros PNG)

| Familia | Cuadros | Fallaban | Fallan | Peor antes (SSIM / máx) | Causa |
|---|---|---|---|---|---|
| Mensajes simples (`render/empty`, `inline`, `mixed`, `scale-150`, `unicode`, `custom-lavanda`) | 6 | 5 | 0 | 0,912 / 252 (`unicode`) | a, b |
| Código (`render/code`) | 1 | 1 | 0 | 0,977 / 215 | a, b, fila base |
| Tablas (`render/table`, `table-narrow`) | 2 | 2 | 0 | 0,943 / 234 | a, b |
| Citas (`render/quote-rule`) | 1 | 1 | 0 | 0,949 / 255 | a, b |
| Listas (`render/lists`) | 1 | 1 | 0 | 0,982 / 251 | b |
| Encabezados (`render/headings`) | 1 | 1 | 0 | 0,977 / 252 | b |
| Imágenes (`render/image-*`) | 4 | 4 | 0 | 0,953 / 255 | a, b |
| Documentos (`render/doc-*`) | 5 | 5 | 0 | 0,929 / 232 | a, b, c |
| Vista/panel (`panel/*`) | 31 | 13 | 0 | 0,981 / 221 | a, b |
| Morph (`morph/*`) | 12 | 5 | 0 | 0,987 / 218 | a, b |

Causas: **a** = el *tile* premultiplicado perdía el color de la tinta y mezclaba
distinto que `draw_bitmap`; **b** = el texto se anclaba por la tinta recortada y
no por el «pen» (1 px en las palabras que empiezan por `ñ`, `*`, `U`…), más el
redondeo de la línea base antes del desplazamiento entero; **c** = la fila vacía
que lleva la máscara del bbox del contorno (`*`, acentos).

## Defectos de Rust corregidos (función y qué estaba mal)
- `paint::chat::base::draw_text_at`: (1) dibujaba la tinta dentro de un *tile*
  float premultiplicado y lo volvía a pasar a RGBA directo, con lo que con
  cobertura 1-5/255 el color salía blanco puro; (2) lo componía con
  `alpha_composite` (source-over recto) en vez de la mezcla de `draw_bitmap`, que
  se nota sobre el chip del código y las celdas de tabla; (3) anclaba la tinta
  recortada en `ctx.px(x)` en vez de `ctx.px(x) + bearing`; (4) calculaba el tope
  en lógico y redondeaba la suma, desplazando 1 px la línea base fraccionaria.
- `paint::chat::base::shear_rows`: la cursiva usaba un sesgo propio con
  interpolación lineal sobre el *tile* premultiplicado; Pillow muestra en el
  centro del píxel con un núcleo cúbico sin renormalizar.
- `paint::chat::base::sample_row`: remuestreaba los cuatro canales del *tile*;
  ahora remuestrea la máscara de cobertura, que es lo que transforma el Python.

## Hallazgos de Python anotados
- `rust-migration/python-quirks.md` → Q-PHUDE-01 (`ImageDraw.text` mezcla
  premultiplicado), Q-PHUDE-02 (`Image.transform` con núcleo cúbico sin
  renormalizar y muestreo en el centro del píxel), Q-PHUDE-03 (ancla «ls» sobre
  el bbox del contorno + redondeo del ancla antes del desplazamiento entero). No
  son errores del Python: son comportamiento de Pillow 13.0.0 que el puerto
  tiene que copiar igual.

## Pendiente o riesgos
- `ink_bearing`/`ink_top` dependen de dos sondas (`T`, `Á`) y de que el bearing
  de `T` sea 0 en la fuente. Verificado con las Geist del proyecto en los once
  estilos a 100/125/150/200 %; con las fuentes de respaldo (Segoe UI/Consolas)
  podría desviarse 1 px. Cuando `ez-gfx` exponga el bbox del contorno de
  `draw_text` (hoy `text_size` devuelve la tinta recortada), sustituir las sondas
  por esa métrica.
- `hud_menu_parity` sigue en rojo por P-HUD-D (no lo toca este brief).
- Los cuadros se validan solo a 100/125/150/200 % de los fixtures; no hay caso
  PNG con escala fraccionaria distinta.

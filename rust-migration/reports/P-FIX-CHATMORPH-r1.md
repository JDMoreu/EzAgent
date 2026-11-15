# P-FIX-CHATMORPH — r1

## Resultado
- Brief/tarea: P-FIX-CHATMORPH (el chat abre sin la animación: el morph costaba ~60 ms por cuadro)
- Estado: completado
- Meta del brief: ≤ 10 ms por cuadro a 100 % y ≤ 16 ms a 150 %.
  Medido: **máximo 4,82 ms a 100 %** y **10,46 ms a 150 %** (mejor de 15 cuadros,
  release, ventana 472×672 / 708×1008), con `hud_chat_parity` en verde y sin tocar
  ningún umbral.

## Cambios
- `rust/crates/ez-hud/src/win/chat/compose.rs`: el lienzo se pide a un pozo por hilo
  (`Canvas`, se devuelve al soltar) en vez de asignar 11 MB nuevos por cuadro; el
  contenido se compone con una sola pasada (`composite_masked`) en lugar de
  materializar un `Array3` del tamaño del panel; `ContentCache` pasa a ser un tipo
  público opaco para que la medición use el camino cacheado real; `fluid_array`
  envuelve el búfer contiguo y usa una tabla byte→`f32`.
- `rust/crates/ez-hud/src/win/chat/compose/raster.rs`: todos los pases
  (componer, sombrear, marco del aro, contenido) recorren filas contiguas con
  `as_chunks` y escriben el premultiplicado en el mismo paso; sin temporales por
  cuadro. La cuantización a BGRA usa `as u8` saturante y `to_rgba_frame` una tabla
  `alpha·256 + canal` precalculada con el mismo redondeo a par.
- `rust/crates/ez-hud/src/win/chat/compose/shapes.rs`: cuerpo, aro y sombra
  fundidos en sus pases (cuerpo con o sin fluido, aro solo en la franja del borde,
  sombra con la ganancia como escalar en vez de `coverage.mapv(...)`).
- `rust/crates/ez-hud/src/win/chat/compose/tests.rs`, `window.rs`,
  `window/input.rs`, `window/lifecycle.rs`: el estado guarda `ContentCache` en vez
  de `Option<(Sig, Layer)>` (mismo ciclo de vida: se vacía al abrir, al
  reconfigurar la vista y al cerrar).
- `rust/crates/ez-gfx/src/resample.rs`: el reescalado salta los pases de
  premultiplicar/despremultiplicar cuando la imagen es opaca (el viaje por `RGBa`
  es la identidad) y el pase vertical acumula una fila entera de golpe (mismos
  taps y mismo orden por píxel).
- `rust/crates/ez-app/src/app/chat_wiring/ui.rs`: `chat_fluid` envuelve el
  `Vec<f32>` del HUD con `from_shape_vec` en vez de reconstruir 1,9 M de valores
  con `from_shape_fn` en cada cuadro.
- `rust/crates/ez-hud/tests/chat_morph_timing.rs`: medición reproducible del
  cuadro real de apertura y cierre con la caché de contenido caliente, a 100, 125
  y 150 %; imprime la tabla y comprueba un techo de 25 ms.

## Qué se cacheó u optimizó y por qué no cambia un píxel
- **Contenido**: ya lo cacheaba el port (`_cache` del Python) con la misma firma
  (`Sig`) y la misma invalidación; aquí solo se elimina el temporal
  `contenido × cobertura × máscara` y la composición se hace con exactamente las
  mismas operaciones (`src·matte` premultiplicado y su propio alfa como factor).
- **Lienzo**: el Python asigna `np.zeros((h, w, 4))` por cuadro; aquí se reutiliza
  el búfer del hilo y se pone a cero, que es el mismo estado inicial. El lienzo se
  reescribe entero en cada cuadro, así que solo importa que el tamaño coincida.
- **Forma**: el cuerpo se premultiplica y compone en una sola pasada con las mismas
  expresiones `body·solid + fluido·(1−solid)` (o `body·alfa` sin fluido) y
  `lienzo·(1−a) + color`; la sombra aplica la ganancia como escalar dentro del
  bucle (`coverage·k` era una multiplicación aparte) y el aro solo toca la franja
  que el Python ya recortaba con `_composite_frame`.
- **Reescalado del fluido**: `Image.resize` de Pillow premultiplica el RGBA antes de
  los pases; con alfa 255 `muldiv255(c, 255) == c`, así que saltarse los dos pases
  deja los mismos bytes. El pase vertical acumula por filas con los mismos taps y
  el mismo orden de suma por píxel (misma aritmética `i32`), y las pruebas de
  `ez-gfx` (incluidas las de Pillow) siguen en verde.
- **Cuantización**: `(x·255 + 0,5) as u8` satura igual que el `np.clip` del Python;
  `to_rgba_frame` precalcula `round_ties_even(canal·255/alfa)` con la misma fórmula.
- Nada de esto cambia geometría, colores ni orden de composición: los 12 casos
  `ChatPanel._compose` de `parity/fixtures/hud_chat.jsonl` siguen pasando contra
  los PNG dorados con los umbrales de `parity/thresholds.toml` intactos.

## Medición antes / después
Mismo arnés (`tests/chat_morph_timing.rs`, `live=true`, con orbe y línea de
estado), mejor de 15 cuadros, release, `cargo test --release -p ez-hud --test
chat_morph_timing -- --nocapture`. «Antes» = commit `d7854c8` con un shim de 8
líneas para exponer `compose_cached` sobre su `compose` (su misma caché de
contenido); «después» = este commit.

| escala | p | antes (ms) | después (ms) |
|---|---|---|---|
| 1,00 | 0,00 | 2,26 | 1,00 |
| 1,00 | 0,25 | 2,99 | 1,31 |
| 1,00 | 0,50 | 4,90 | 2,26 |
| 1,00 | 0,75 | **11,69** | **4,82** |
| 1,00 | 1,00 | 9,14 | 2,34 |
| 1,00 | cierre p=0,50 | 3,85 | 1,89 |
| 1,25 | 0,00 | 3,44 | 1,50 |
| 1,25 | 0,25 | 4,60 | 2,04 |
| 1,25 | 0,50 | 7,05 | 3,39 |
| 1,25 | 0,75 | 17,61 | 7,43 |
| 1,25 | 1,00 | 13,81 | 3,68 |
| 1,25 | cierre p=0,50 | 6,20 | 2,96 |
| 1,50 | 0,00 | 4,89 | 2,14 |
| 1,50 | 0,25 | 6,37 | 2,82 |
| 1,50 | 0,50 | 11,05 | 4,89 |
| 1,50 | 0,75 | **26,13** | **10,46** |
| 1,50 | 1,00 | 20,23 | 4,98 |
| 1,50 | cierre p=0,50 | 9,05 | 4,13 |

Peor cuadro: 26,13 → 10,46 ms a 150 %; 11,69 → 4,82 ms a 100 %. El
`MORPH_SECS = 0,28` ya cabe a 60 fps (antes a 150 % solo entraban ~10 cuadros).

### Referencia en Python (mismo equipo, intérprete del proyecto)
`ChatPanel._compose` llamado directamente, con `_cache` caliente como lo deja
`open()`; mismo tipo de escena y ventana (script de medición descrito al final).
warm / cold (cold = sin caché de contenido, como `compose_frame`):

| escala | p | warm (ms) | cold (ms) |
|---|---|---|---|
| 1,00 | 0,00 | 2,66 | 5,04 |
| 1,00 | 0,25 | 4,54 | 6,95 |
| 1,00 | 0,50 | 11,08 | 43,67 |
| 1,00 | 0,75 | 30,23 | 65,15 |
| 1,00 | 1,00 | 34,12 | 65,97 |
| 1,25 | 0,75 | 52,73 | 89,80 |
| 1,50 | 0,50 | 25,82 | 74,55 |
| 1,50 | 0,75 | 69,46 | 118,67 |
| 1,50 | 1,00 | 75,93 | 124,59 |

El Python tampoco cabía en 16,7 ms por cuadro salvo al principio del morph; de ahí
que su animación también se vea a tirones. La cifra de «58-67 ms» del brief
coincide con el caso *cold* del Python a 100 %, no con el Rust anterior (que ya
estaba en 11,7 ms warm a 100 %): ver «Pendiente o riesgos».

### Dónde se iba el tiempo (perfil por piezas, 150 %, p=0,75)
Medido por partes con el mismo lienzo caliente (mejor de 21 pasadas); «antes» es
el port previo, «después» el actual.

| pieza | antes (ms) | después (ms) |
|---|---|---|
| reescalado bicúbico del fluido (674×982) | 7,6 | 2,8 |
| conversión del fluido a `f32` | 4,0 | 2,4 (~0 en la app: `from_shape_vec`) |
| `_paint_shape` (cuerpo + sombra + aro, con fluido) | 19,9 | 5,2 |
| `_paint_content` | 5,5 | 1,2 |
| cuantización BGRA | 4,9 | 2,7 |
| lienzo nuevo por cuadro (páginas en frío) | ~1,9 | 0 (pozo) |
| **cuadro completo** | **26,1** | **10,5** |

## Decisiones
- `// DECISION` nuevas (marcadas en el código donde aplica):
  - `ContentCache` público y opaco: la medición tiene que ejercitar la caché de
    contenido; el panel lo guarda en `PanelState` con el mismo ciclo de vida.
  - Pozo de lienzos `thread_local` en vez de un búfer propiedad del panel: así la
    firma pública `compose_frame` no cambia y el óleo se devuelve al hilo que
    pinta. Cada hilo que pinte un panel guarda un lienzo propio (11 MB a 150 %).
  - `Image::resize` salta la ida y vuelta por `RGBa` solo si **todos** los píxeles
    son opacos (se comprueba en una pasada barata antes de reescalar).
  - Umbral de la prueba de tiempos: se comprueba ≤ 25 ms por cuadro (no 10/16)
    para no convertir el arnés en una prueba frágil en máquinas lentas; los
    números reales se imprimen y son los de la tabla.
  - `tests/chat_morph_timing.rs` se salta en debug (`cfg!(debug_assertions)`):
    un `cargo test -p ez-hud` sin `--release` no mide nada útil. No se usa
    `#[ignore]` (patrón que `_COMMON.md` prohíbe).
- Sin cambios en `Cargo.toml` ni dependencias nuevas.

## Verificación
- `cargo fmt --all` → ok.
- `cargo clippy -p ez-hud -p ez-gfx -p ez-app --all-targets -- -D warnings` → ok.
- `cargo test -p ez-hud -p ez-gfx` → ez-gfx 77 + 3 (`text_mask_parity`); ez-hud 490
  unitarias + `chat_morph_timing` + `hud_bar_parity`, `hud_chat_parity`,
  `hud_drop_model_parity`, `hud_geometry_parity`, `hud_menu_hit_parity`,
  `hud_menu_parity`, `hud_peek_parity`, `hud_settings_parity`: todo en verde.
- `cargo test --release -p ez-hud --test hud_chat_parity` → ok (221 casos, 12 de
  morph), umbrales sin tocar.
- `cargo test --release -p ez-hud --test chat_morph_timing -- --nocapture` → tabla
  de arriba; peor cuadro 10,46 ms (150 %).
- Fixtures consumidos: `parity/fixtures/hud_chat.jsonl` (221 casos; 12 de
  `ChatPanel._compose`, leídos para la escena de la medición) y los PNG de
  `parity/golden/hud/chat/morph-*.png` como comprobación de píxeles.
- Pruebas portadas nuevas: ninguna fila de `parity/test-map.csv` (esta fase no
  escribe pruebas de paridad); `tests/chat_morph_timing.rs` es medición.
- Métricas: ver tablas (antes → después y referencia Python).

## Pendiente o riesgos
- El cuadro real de la ventana suma, fuera de `compose`, `Frame::to_rgba_frame`
  (~1,8 ms a 150 %, ya con tabla) y `LayeredWindow::present` (premultiplica y
  `UpdateLayeredWindow`). No entra en el presupuesto del brief, que mide el
  compose, pero conviene tenerlo presente si algún día se apura el margen.
- El fluido sigue costando ~5 ms a 150 % dentro de los 10,5 ms: 2,8 de reescalado
  (bit a bit el de Pillow) y 2,4 de conversión a `f32` en el `ChatUi` de prueba.
  En la app esa conversión es ahora cero copias, pero `ez-hud/src/win/drop/ui/
  appearance.rs` (fuera del alcance de este brief) todavía arma un `Vec<u8>` y un
  `Vec<f32>` por llamada. Si se quiere más margen: exponer el `Image` reescalado
  por el trait `ChatUi` (toca también los fakes de `tests/hud_chat_parity`).
- La cifra «58-67 ms» del brief no se reprodujo en Rust: el commits previo daba
  11,7 ms (100 %, p=0,75) y 26,1 ms (150 %) en esta máquina; sí coincide con el
  Python *cold* a 100 % (65 ms). La animación se perdía igual: 26 ms a 150 % son
  ~10 cuadros en 0,28 s y el primer cuadro con contenido llega a p≈0,5.
- El pozo de lienzos guarda **un** búfer por hilo con el tamaño del último panel:
  si se pinta a dos escalas en el mismo hilo, el búfer se reasigna al cambiar de
  tamaño (correcto, solo se pierde el calor de páginas).
- `rounded_coverage` sigue fallando la caché en cada cuadro del morph (el radio
  interpola); se dejó como estaba porque su coste es de franja y ya no aparece en
  el perfil. Cachear por radio redondeado sería un cambio de píxeles potencial.

### Script de referencia de Python (no forma parte del repo)
Se ejecutó con `C:/Users/JDMoreu/Documents/EzAgent/.venv/Scripts/python.exe`
desde el árbol Python, con `sys.path` a `src/`: construye un `FakeUI` con
`DropModel` real, `ChatPanel`, `ChatView` con tres mensajes, `_form` y una
`SimpleNamespace` como ventana (el mismo apaño que `parity/gen/hud_chat.py`) y
cronometra `panel._compose(window, view, p, 0.0, live=False)` cinco veces por
punto (mínimo), calentando `_cache` antes. El original quedó en
`%TEMP%\opencode\chat_morph_py.py`; se reproduce con el `FakeUI`/`morph_frame`
de `parity/gen/hud_chat.py` cambiando el `save_frame` por un `perf_counter`.

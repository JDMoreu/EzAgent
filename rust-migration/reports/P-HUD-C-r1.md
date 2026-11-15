# P-HUD-C — Paridad de `ez-hud`, parte C (chat: modelo, vista, panel y cuadros)

## Resultado
- Brief/tarea: P-HUD-C
- Estado: parcial
- `hud_chat`: **182/221** casos dentro de umbral.
  - **Los 158 casos sin PNG pasan al 100 %** (constantes, `Ctx`, `wrap`, `_break`,
    `input_box_height`, `stamp_height`, `row_reveal`, `shape_progress`, `fluid_array`,
    `_hotkey_label`, `_clip`, `_date`, duración/pasos del panel y toda la API de
    `ChatView`: `key`, `edit`, `add_files`, `can_start_new`, `animating`,
    `pointer_down`, `pointer_move`, `wheel`).
  - **24/63 cuadros PNG** dentro de los umbrales: `render_message` 1/21,
    `ChatView.render` 16/30, `ChatPanel._compose` 7/12.
  - **39 PNG fallan solo por el rasterizado de glifos de `ez-gfx`** (pendiente de
    P-GFX-TEXT): la maquetación, el texto (contenido, tamaño y posición) y las
    zonas pulsables coinciden; lo que difiere es la cobertura de los bordes de
    cada glifo. Ver «Pendiente o riesgos».

## Cambios
- `rust/crates/ez-hud/tests/hud_chat_parity.rs` y
  `hud_chat_parity/{support,view,view_input,panel,render,morph,logic}.rs`,
  `hud_chat_parity/markdown/{mod,blocks,inline}.rs`: prueba de paridad del dominio
  (221 casos); reconstruye el mundo de cada caso (conversaciones, historial,
  archivos del oráculo, tema personalizado, forma y ventana del morph) y compara
  estructura, zonas pulsables y PNG con `ez_parity`.
- `rust/crates/ez-hud/src/model/chat/footer.rs`: `AUDIO_W` pasa a público para el
  caso de constantes.
- `rust/crates/ez-hud/src/model/chat/scroll.rs`: `set_offset_raw` (asignación de
  `view.scroll` sin recortar, como el campo Python; el siguiente maquetado recorta).
- `rust/crates/ez-hud/src/model/chat/view.rs`: `set_scroll`, `set_focus_instant` y
  `set_history_state` para restaurar el estado que los fixtures reconstruyen.
- `rust/crates/ez-hud/src/paint/chat/render_rich/{common,documents}.rs` y
  `render_rich.rs`: `clip`, `date_of` y `family_color` públicos (casos `_clip`,
  `_date` y constantes de familias).
- `rust/crates/ez-hud/src/win/chat/window.rs`, `window/anim.rs`, `mod.rs`: `Anim`,
  `AnimState`, `duration`, `elapsed_for`, `step` y `hotkey_label` públicos.
- `rust/crates/ez-hud/src/win/chat/compose.rs`: `Scene` y `WindowRect` públicos y
  `compose_frame` (un cuadro sin caché de contenido) para los cuadros del morph.
- `rust-migration/reports/P-HUD-C-r1.md`: este informe.

## Decisiones
- **Parser de markdown en las pruebas**: `ez-hud` recibe el parser por el seam
  `ChatHost` y no depende de `ez-chat` (y no se puede tocar `Cargo.toml`). Las
  pruebas traen una copia del parser ya portado (`ez-chat::markdown`) sobre los
  tipos de bloque de `ez-hud`; sin él no se pueden reproducir alturas
  (`max_scroll`), entradas ni imágenes.
- **Ficha de archivos del oráculo**: `render_message` recibe un `ChatHost` que
  sirve las fichas de `input.files` (tipo, texto, RGBA, bytes y `mtime_wall`) sin
  tocar el disco. El tamaño en bytes de los PNG de prueba no está en el fixture;
  se toma de la ficha que el Python escribió en `hits` (solo afecta a metadatos,
  no al dibujo: las tarjetas de imagen no muestran tamaño).
- **Raíz temporal**: el cursor de `view-add-files` depende de la longitud del
  `HOME` con que se generó el fixture; se rellena una raíz sintética hasta 53
  caracteres para reproducirlo (ver «Pendiente o riesgos»).
- **Cuadros del morph**: `FakeUi` reproduce la barra del generador (48 pasos de
  `DropModel` a 30 Hz) y `chat_fluid` cachea el fluido grueso por instante.

## Verificación
- `cargo fmt --all`: limpio.
- `cargo clippy -p ez-hud --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-hud --lib`: **485 passed, 0 failed**.
- `cargo test -p ez-hud --test hud_chat_parity`: 221 casos ejecutados
  (`cases_run == cases_in_file == 221`); falla solo el veredicto de píxeles de
  39 cuadros (lista abajo).
- Fixtures consumidos: `parity/fixtures/hud_chat.jsonl`, **221 casos**; cuadros
  leídos de `parity/golden/hud/chat/` (63 PNG) y umbrales
  `parity/thresholds.toml` (`[pixels.text]`).
- Defectos de Rust corregidos: **ninguno**. Los 158 casos de lógica y toda la
  estructura (altura, `image_px`, `plain`, `hits`) de los 63 cuadros pasaron en
  cuanto la prueba reconstruyó bien las entradas; no hubo que cambiar ninguna
  fórmula del port.
- Pruebas portadas (`parity/test-map.csv`): no aplica; el dominio se cubre con
  el fixture `hud_chat` (no hay filas de pytest para estos casos en el mapa).

### Categoría «pendiente de ez-gfx» (39 cuadros)
Todos fallan por la cobertura del antialias de los glifos, no por maquetación:
el mejor desplazamiento global es (0, 0) y la diferencia media fuera de los
bordes de tinta es ~2/255; en los píxeles de borde hay coberturas de 224 frente
a 76, etc. Ejemplos y números (max/p995/SSIM):

- `render`: `code` 215/83/0,977; `custom-lavanda` 255/233/0,958; `doc-denied`
  168/142/0,939; `doc-missing` 59/54/0,990; `doc-other` 227/149/0,935;
  `doc-pdf` 217/152/0,929; `doc-preview` 232/171/0,946; `headings`
  252/38/0,977; `image-denied` 46/43/0,992; `image-missing` 51/44/0,994;
  `image-ok` 255/102/0,972; `image-web` 168/135/0,953; `inline`
  255/228/0,978; `lists` 251/38/0,982; `mixed` 255/240/0,968; `quote-rule`
  255/240/0,948; `scale-150` 250/114/0,971; `table` 234/176/0,943;
  `table-narrow` 193/45/0,983; `unicode` 252/184/0,912. (`render/empty`: pasa.)
- `panel`: `confirm-sending-dark-100` 227/37/0,986; `conversation-dark-100`
  227/46/0,986; `conversation-dark-150` 211/31/0,992; `conversation-dark-200`
  217/85/0,981; `conversation-light-100` 227/51/0,986; `conversation-light-125`
  217/32/0,987; `conversation-light-200` 221/78/0,981;
  `conversation-small-380x520` 227/71/0,983; `custom-lavanda-dark-100`
  227/54/0,986; `error-dark-100` 231/0/0,995; `input-multiline-light-100`
  227/51/0,986; `long-scrolled-light-100` 228/65/0,983;
  `status-listening-light-100` 227/51/0,986; `viewing-previous-real-light-100`
  227/3/0,994.
- `morph`: `close-dark-75` 177/11/0,990; `open-circle-100` 218/51/0,986;
  `open-dark-100` 206/49/0,986; `open-light-100` 218/51/0,986;
  `open-light-75` 177/30/0,986.
- Pasan (muestran que el canal de composición está bien): los `empty-*`, los
  `history-*`, `note-system`, `sending-stop`, `streaming-empty`,
  `input-empty-caret`, `render/empty` y los morph con `p ≤ 0,5` o reducido.

## Pendiente o riesgos
- **Bloqueo externo (no tocar: P-GFX-TEXT es dueño de `ez-gfx`)**: la máscara de
  glifo de `ez-gfx` no coincide con la de Pillow/Freetype en los bordes
  (antialias), así que los cuadros con cuerpo de texto no alcanzan
  `ssim_min = 0,995` ni `p995 = 2`. No se parcheó nada en `ez-hud`: la posición
  del texto, los saltos, los tamaños y las zonas pulsables son idénticos. Cuando
  P-GFX-TEXT ajuste la cobertura, estos 39 casos deberían pasar sin tocar esta
  prueba.
- **Caso dependiente de la máquina**: `hud_chat/api/view-add-files` guarda
  `cursor = 164`, que depende de la longitud del `HOME` del generador (53
  caracteres, invisible tras el marcador `<tmp>`). La prueba lo reproduce
  rellenando la raíz sintética; conviene regenerar el fixture con una raíz fija
  si se quiere que el caso sea portable.
- **API pública añadida por la prueba**: `set_scroll`, `set_focus_instant` y
  `set_history_state` existen porque los fixtures restauran campos que en Python
  son privados; no cambian el comportamiento. Si se prefiere no ampliar la API,
  el arnés puede vivir en `src/` como prueba unitaria, pero entonces no sería
  una prueba de integración del crate.
- **`ChatPanel`**: los 12 cuadros del morph se comprueban con
  `compose_frame` (sin caché) y una `FakeUi` que reproduce la barra; la
  integración real con `ChatWindow` sigue siendo de F6.

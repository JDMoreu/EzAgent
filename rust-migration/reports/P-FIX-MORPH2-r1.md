# P-FIX-MORPH2 — r1

## Resultado
- Brief/tarea: P-FIX-MORPH2 (el chat abre y cierra sin la transformación esfera/píldora ↔ panel)
- Estado: completado
- Causa de la apertura (reproducida): `ChatPanel::open` tomaba el instante de
  arranque **antes** de pintar el historial (`prepare_view`) y de arrancar la
  ventana nativa. Con un historial real eso suma más que `MORPH_SECS = 0,28` y el
  primer cuadro compuesto ya salía con `p = 1` (`state=Open`): la transformación
  entera caía en el primer fotograma. Medido en la app real con la preparación
  retrasada a propósito: `p=1.0` en el primer cuadro (antes) → 16 cuadros con
  `0 < p < 1` (después).
- Causa del cierre: los cuadros intermedios sí se componían y presentaban, pero
  el primer cuadro del cierre podía volver a pintar la capa de contenido (la
  firma cambia al dejar de «tickear»: `blink` y `anim_tick`), y `view.render`
  con historial real cuesta cientos de ms en frío (medido: 1,02 s para 40
  mensajes en la primera pasada del proceso; 6 ms en caliente). Si supera los
  `CLOSE_SECS = 0,20`, el segundo cuadro ya cae en `p = 0`. Además el bucle
  esperaba el intervalo **anterior** al cuadro y no se le despertaba al cambiar
  de estado, así que se perdían el primer cuadro del cierre y parte del morph.

## Cambios
- `rust/crates/ez-hud/src/win/chat/window/lifecycle.rs`: `t0` se toma después de
  `prepare_view` y de `window.start()` (Python lo hace después de `view.render` y
  de construir la ventana); `install_view` va tras el arranque nativo y el bucle
  se despierta (`window.wake()`) antes de `show()`. En `close()` se despierta el
  bucle al fijar `Closing` para que el primer cuadro sea el panel entero.
- `rust/crates/ez-hud/src/win/chat/compose.rs`: al cerrar se reutiliza la capa de
  contenido que dejó el panel abierto (`reuse`); ya no se vuelve a pintar el
  historial a mitad del cierre. Con caché vacía (primer cuadro, fixtures) se
  calcula como siempre.
- `rust/crates/ez-hud/src/win/chat/window/chrome.rs`: el bucle de pintura usa el
  intervalo del cuadro **recién pintado** (60 fps del morph, no los 30 del
  reposo) y descuenta el tiempo de composición de la espera (como
  `wait_delay` del Python): de ~19-20 ms por cuadro a ~17 ms. Nuevo
  `ChatWindow::wake`.
- `rust/crates/ez-hud/src/win/chat/window.rs`: `tracing::debug!` por cuadro del
  morph (p, estado, `finished`) en `ezagent::hud::chat`, la traza que pidió el
  brief.
- `rust/crates/ez-hud/src/win/chat/test_support.rs`: `FakeUi::slow_view` (segundos
  que duerme `make_view`) para simular un historial que tarda más que el morph.
- `rust/crates/ez-hud/src/win/chat/window/tests.rs`: dos pruebas nuevas:
  `a_slow_history_still_animates_the_opening` (abre de verdad y exige ≥3 cuadros
  con `0 < p < 1`) y
  `closing_paints_intermediate_frames_before_removing_the_window` (≥3 cuadros
  intermedios y la ventana presente hasta `finished`).
- `rust/crates/ez-hud/src/win/chat/compose/tests.rs`:
  `the_closing_frames_reuse_the_open_content_layer` fija la política de caché del
  cierre (comparando la firma guardada con la que provocaría un repintado).

## Decisiones
- `t0` después de `window.start()`, no solo después de `prepare_view`: el
  arranque nativo medido en la app es de 0,09-0,29 s y también se comía el morph.
  Es una desviación mínima del Python (que lo fija antes de `start`), en la
  dirección que pide el brief y que ya documenta el propio Python («el primer
  cuadro con contenido costaba ~100 ms»).
- Reutilizar la capa de contenido durante `Closing`: la única diferencia de
  firma es el parpadeo del cursor y el tick de streaming, que no son contenido.
  En el peor caso el cursor se ve en su fase anterior durante 0,20 s dentro de un
  panel que se encoge. Los fixtures de paridad componen con caché vacía, así que
  no cambia ni un píxel de lo dorado.
- Despertar el bucle de pintura en `close()` en vez de esperar al siguiente tick
  de 30 fps: se ganan hasta 33 ms de los 200 del cierre.
- La prueba de apertura crea una ventana nativa real (como la app). Si no hay
  escritorio, `open()` devuelve `false` y la prueba se salta sin marcar `#[ignore]`
  (patrón prohibido).

## Verificación
- `cargo fmt --all` → ok.
- `cargo clippy -p ez-hud --all-targets -- -D warnings` → ok.
- `cargo test -p ez-hud` → 498 unitarias + `chat_morph_timing`, `hud_bar_parity`,
  `hud_chat_parity`, `hud_drop_model_parity`, `hud_geometry_parity`,
  `hud_menu_hit_parity`, `hud_menu_parity`, `hud_peek_parity`,
  `hud_settings_parity`: todo en verde.
- `cargo test --release -p ez-hud --test hud_chat_parity` → ok (221 casos, 12 de
  morph), umbrales sin tocar.
- `cargo test -p ez-app chat_wiring` → 12 en verde.
- Regresión comprobada al revés: con el `t0` viejo la prueba de apertura falla con
  `la apertura solo pintó 0 cuadros intermedios: [1.0]`; sin la política de
  reutilización falla la del cierre (`blink: 1` esperado, `blink: 0` repintado).

### Medición en la app real (release, `run-dev.cmd`)
Sonda temporal **no commiteada** en `chat_wiring.rs` (abre/cierra el chat cada 3 s)
y `make_view` retrasado 400 ms a propósito; `RUST_LOG` a debug temporal. La app se
lanzó con las variables de `rust/run-dev.cmd` y el log se leyó de stderr.

| | antes | después |
|---|---|---|
| `make_view` (historial simulado) | 0,42-0,49 s | igual |
| `prepare_view` | 0,003-0,05 s | igual |
| `window.start()` | 0,09-0,29 s | igual |
| primer cuadro de apertura | `p=1.0 state=Open` | `p≈2e-10 state=Opening` |
| apertura | 1 cuadro (el panel ya abierto) | 16 cuadros `0<p<1` en ~280 ms |
| cierre | (se perdía el primer cuadro y el ritmo) | 13 cuadros `1→0` en ~208 ms |
| ritmo de cuadro | 19-20 ms | 17,0 ms |

Secuencia real de apertura tras el arreglo (log): `p = 2,4e-10, 0,0010, 0,0077,
0,0257, 0,0600, 0,1182, 0,2037, 0,3251, 0,4994, 0,6620, 0,7829, 0,8720, 0,9323,
0,9702, 0,9904, 0,9985, 1,0`. Cierre: `0,99999999999, 0,9975, 0,9808, 0,9334,
0,8443, 0,6787, 0,4524, 0,2474, 0,1195, 0,0443, 0,0099, 0,00039, 0,0` y solo
después la barra retira la ventana (`poll` → `teardown`).

### Coste del contenido (por qué el cierre podía perder la animación)
`ChatView::render` con `view_with` de mensajes reales, release, primera pasada del
proceso / siguiente: 1 mensaje 82 / 2,9 ms; 10 → 263 / 6,1 ms; 40 → 1 023 / 6,1 ms;
100 → 2 522 / 6,1 ms. Con historial real el primer cuadro que recompone contenido
puede pasarse de los 200 ms del cierre; la política nueva lo evita.

## Pendiente o riesgos
- El coste en frío de `ChatView::render` (atlas de glifos y entradas por mensaje)
  sigue ahí: afecta a la **primera** apertura del proceso (el historial se pinta
  antes del morph, ya previsto por el Python) y a cada mensaje nuevo del
  asistente. Queda fuera del alcance de este brief (`model/chat/view`), pero con
  él se iría más fino: medido 1 s para 40 mensajes en frío.
- La ventana se sigue retirando en el `poll` de la barra, hasta 33 ms después del
  cuadro `p=0`; no afecta a la parte visible del cierre y se dejó como el Python.
- El parpadeo del cursor durante el cierre puede quedar en su fase anterior
  (decisión de reutilizar la capa). Invisible en la práctica: el campo de texto
  se desvanece a partir de `p < 0,52`.
- Fixtures consumidos: `parity/fixtures/hud_chat.jsonl` (221 casos; los 12 de
  `ChatPanel._compose` se leyeron para entender escenas y valores; no se añaden
  pruebas de paridad en esta fase).
- Pruebas portadas de `parity/test-map.csv`: ninguna (fase «código primero»).

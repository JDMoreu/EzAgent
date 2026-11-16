## Resultado

- Brief/tarea: P-FIX-LOCKAUDIT
- Estado: completado

## Cambios

- `rust/crates/ez-hud/src/win/peek/surface.rs` (nuevo): trait `PeekSurface` (start/stop/show/hide/resize/set_pos/is_visible/size/position/set_interval/mark_dirty) con la regla escrita: el controlador aplica los cambios de ventana **sin** ningún lock tomado. Existe para que la prueba de regresión ponga una ventana que, como Win32, no vuelve hasta que su hilo atiende el mensaje; en producción solo lo implementa `PeekWindow`.
- `rust/crates/ez-hud/src/win/peek.rs`: declara `mod surface`.
- `rust/crates/ez-hud/src/win/peek/window.rs`: `impl PeekSurface for PeekWindow` (delega en los métodos que ya existían; sin cambios de comportamiento).
- `rust/crates/ez-hud/src/win/peek/controller.rs`: `Peek::win` pasa a `Mutex<Option<Arc<dyn PeekSurface>>>`; `advance` calcula la fila bajo el puntero con el estado ya prestado en vez de llamar a `pointer_row` (que volvía a bloquear `self.lock` en el mismo hilo: cuelgue seguro con la tarjeta abierta y el puntero encima); se elimina `pointer_row` (sin más usos).
- `rust/crates/ez-hud/src/win/peek/tests.rs`: doble `SyncSurface` (su `resize` espera al cuadro siguiente, con plazo) y dos pruebas nuevas: la de estrés del incidente (hilo barra con filas que cambian de tamaño + hilo de pintura, con plazo de 20 s) y la del re-lock de `advance`.
- `rust/crates/ez-hud/src/win/drop/ui/card_input.rs`: `place_card` suelta el lock de la tarjeta antes de `start`/`resize`/`set_pos`/`show`; `menu_closed` retira la tarjeta y la para en su propio hilo (`ezagent-card-stop`) porque el aviso llega desde dentro del menú, con su lock tomado.
- `rust/crates/ez-hud/src/win/drop/ui/render.rs`: `peek.tick` (mueve/muestra/oculta la ventana de la vista rápida, otro hilo) va después de soltar el lock de la barra; el `poll` del chat se hace con el `Arc` clonado, no con el guard de lectura.
- `rust/crates/ez-hud/src/win/drop/ui/lifecycle.rs` y `.../appearance.rs`: `close_now`/`poll`/`is_open`/`set_mic_muted` se llaman con el `Arc` del chat ya clonado (el `RwLock` de lectura es un temporal de la sentencia y quedaba tomado durante la llamada).
- `rust/crates/ez-hud/src/win/chat/window/lifecycle.rs`: `ChatPanel::close` clona el `Arc` de la ventana antes de `set_keyboard_enabled`/`wake`.
- `rust/crates/ez-hud/src/win/drop/ui/tests/support.rs`: el doble de ventana conoce su barra (`Weak<DropUI>`) y detecta, como el hilo real, si el lock del menú (en `stop`) o el de la tarjeta (en `resize`) estaba tomado; `wait_for` espera una bandera con plazo.
- `rust/crates/ez-hud/src/win/drop/ui/tests/locks.rs` (nuevo): pruebas de `place_card` y de `menu_closed` con esos dobles.
- `rust/crates/ez-hud/src/win/drop/ui/tests.rs`: `the_menu_opens_and_closes_the_card` espera el stop diferido (misma aserción, sin cuelgue).

## Hallazgos

Leyenda: **A** = hilo que toma el lock, **B** = hilo que lo pide después.

| # | Archivo · función | A / B y orden de locks | Estado |
|---|---|---|---|
| 1 | `peek/controller.rs` · `place`, `hide_now`, `apply(Plan::Hide)` | A = `ezagent-drop-paint` (barra) toma `Peek::win` y llama `resize`/`set_pos`/`show`/`hide`; B = `ezagent-peek-paint` pide `Peek::win` en `render`. Si la operación de ventana espera a B (mensajes síncronos de Win32), abrazo mortal. | **Corregido ya en `100d5da`**; se deja igual y se añade la prueba que falla con el código anterior |
| 2 | `peek/controller.rs` · `tick` → `advance` → `pointer_row` → `row_at` | Un solo hilo (`ezagent-drop-paint`): `tick` tiene `Peek::lock` y `row_at` vuelve a pedirlo. `parking_lot` no es reentrante. | **Corregido** (cuelgue seguro en el primer cuadro con la tarjeta abierta y el puntero sobre ella) |
| 3 | `drop/ui/render.rs` · `render_frame` → `peek.tick` | A = `ezagent-drop-paint` con `BarState` tomado mueve la ventana de la vista rápida; B = hilo de esa ventana pide `BarState` a través de `PeekUi` (p. ej. `frame_anchor`/`hub_extent` si el host llama a la barra). | **Corregido** (`tick` fuera del lock) |
| 4 | `drop/ui/card_input.rs` · `place_card` | A = hilo de la barra/interfaz con `card` tomado hace `start`/`resize`/`set_pos`/`show`; B = `ezagent-card-paint` (`render_card` → `card_window()` → `card`) y el hilo de la ventana de la tarjeta (`clamp_card`, `card_moved`). | **Corregido** |
| 5 | `drop/ui/card_input.rs` · `menu_closed` (callback `close` del menú) | A = quien recibe `Menu::update` → `notify_close` (`model/menu/clock.rs:120`), con `Mutex<dyn MenuPanel>` **y** `Mutex<Menu>` tomados, y para la tarjeta con `join`; B = hilo de la ventana de la tarjeta en un gancho del puntero pide el lock del menú (`card_pointer`/`card_hand` → `menu_call`). | **Corregido** (stop en `ezagent-card-stop`) |
| 6 | `chat/window/lifecycle.rs` · `ChatPanel::close` | A = quien cierra con el lock de `ChatWindow` tomado llama `set_keyboard_enabled`/`wake`; B = `ezagent-chat-paint` lo pide al clonar en `render`. | **Corregido** (regla; riesgo bajo: solo publican estado) |
| 7 | `drop/ui/lifecycle.rs`, `.../appearance.rs` · `hide`, `close`, `set_muted`, `chat_is_open` | A = quien llama retiene el `RwLock` de lectura de `chat` durante `close_now` (que hace `join` del hilo del chat)/`poll`/`is_open`/`set_mic_muted`. | **Corregido** (el panel nunca pide el lock de escritura, así que hoy no colgaba) |

Sin riesgo (revisado y documentado):

- `layered.rs` · `on_pointer_*`/`on_key`/…: el lock `hooks` se retiene durante el callback, pero ningún camino vuelve a `set_hooks` ni a otro `on_*` desde él.
- `drop/ui.rs` · `ensure_window`: toma `win` mientras crea el objeto de la barra y consulta `bar` (orden `win`→`bar`); no existe el orden inverso en ningún camino (revisadas todas las tomas de `bar.lock()`).
- `chat/window.rs` · `drain_events`: `events` tomado durante los callbacks del panel; ese canal solo lo lee su propio hilo.
- `wndproc.rs`: todos los `state.lock()` son temporales de una sentencia y no cruzan un callback; el WndProc corre en el hilo de su ventana.
- `ez-app/src/app/**`: los callbacks del HUD van a `Queue` (sin locks), y los guards de `cfg` se sueltan antes de llamar a la barra (`hide_hud`, `apply_instant_setting`, `speak_test_phrase`, `startup::start`). No hay ningún lock de la app tomado durante una operación de ventana.

## Decisiones

- `// DECISION` en `drop/ui/card_input.rs` (`stop_card_async`): si no se puede crear el hilo que para la tarjeta, se registra y la ventana se queda hasta el cierre de la app; colgar la interfaz es peor.
- `PeekSurface` es un trait (no un `cfg(test)` en `PeekWindow`) para poder inyectar el doble síncrono sin abrir ventanas reales, igual que `HudWindow`/`WindowFactory` en `win::drop` (patrón ya existente en el crate).
- El doble `SyncSurface` espera el cuadro con plazo de 3 s (bajo la suite en paralelo un cuadro puede tardar) y la prueba de estrés tiene plazo de 10 s para la barra y 20 s para el hilo.

## Verificación

- `cargo fmt --all`: sin cambios pendientes.
- `cargo clippy -p ez-hud --all-targets -- -D warnings`: limpio.
- `cargo clippy --workspace --all-targets -- -D warnings`: limpio (solo avisos del `vendor`).
- `cargo test -p ez-hud`: 504 passed, 0 failed.
- `cargo test -p ez-app`: 294 + 1 + 1 passed (incluye `mic_lock`), 0 failed.
- `cargo test -p ez-app --features replay --test replay_parity`: verde (ver nota al final).
- Pruebas nuevas y comprobación de que fallan con el código anterior (revert temporal, ejecutado y restaurado):
  - `the_card_never_moves_with_its_lock_taken` con el `place` anterior (`drop(slot)` fuera): «`resize` esperó a un cuadro con el lock de la ventana tomado».
  - `a_tick_with_the_pointer_over_the_card_does_not_relock_the_state` con `pointer_row`/`row_at` dentro de `advance`: «`tick` volvió a pedir el lock del estado en el mismo hilo» (2 s de plazo; no cuelga la suite).
  - `placing_the_card_does_not_hold_its_lock` con el `place_card` anterior: «`place_card` movió la tarjeta con su lock tomado».
  - `closing_the_menu_stops_the_card_without_the_menu_lock` con el `menu_closed` anterior: «la tarjeta se paró con el lock del menú tomado».
- Fixtures consumidos: ninguno (el brief no define fixtures para esta tarea).
- Filas de `parity/test-map.csv` portadas: ninguna (no es una tarea de portado).

## Pendiente o riesgos

- El hallazgo 2 (re-lock de `advance`) no es el que describía el brief: es otro cuelgue seguro de la vista rápida, encontrado al escribir la prueba de estrés. Conviene comprobar en la app real que la tarjeta abre con el puntero encima sin congelar la barra (antes se congelaba en ese mismo huelgo).
- `ez-app` · `HudConfig::save`/`remember` hacen la escritura a disco con `cfg.write()` tomado, así que un guardado lento de Ajustes puede retrasar un cuadro de la barra (`save_bar_position` corre en el hilo de pintura). No es un interbloqueo y queda fuera de este brief; anotado para una tarea de rendimiento.
- El stop diferido de la tarjeta crea un hilo por cierre (`ezagent-card-stop`, igual que `ezagent-chat-stop`): si se abriera y cerrara el menú en bucle muy rápido se acumularían hilos por milisegundos. No se ha visto en la app y el patrón ya existía para el chat.
- `cargo test -p ez-app --features replay --test replay_parity` tarda ~7 min; se ejecutó con el resto de la entrega.

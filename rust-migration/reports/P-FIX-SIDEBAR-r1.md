## Resultado
- Brief/tarea: P-FIX-SIDEBAR
- Estado: completado

## Cambios
- `rust/crates/ez-hud/src/win/drop/ui/card_input.rs`: el resaltado de la barra lateral (`sidebar_hover`) se refresca en cada movimiento con o sin botón (`track_sidebar_hover`, llamado desde `card_pointer(Move)`, desde la rama de sección de `card_pointer_down` y desde `card_hand`); al pulsar una sección el hover pasa a esa fila, así que la sección anterior deja de estar marcada.
- `rust/crates/ez-hud/src/win/drop/ui/menu.rs`: al abrir la tarjeta se limpia `sidebar_hover` (la tarjeta nace con el puntero en la barra, no en la lista).
- `rust/crates/ez-hud/src/win/drop/ui/tests.rs`: declara el submódulo de pruebas nuevas.
- `rust/crates/ez-hud/src/win/drop/ui/tests/sidebar.rs`: pruebas de regresión: la secuencia del usuario (A → B → C, con pulsación y movimientos) deja **solo** la sección nueva marcada, ninguno conserva hover al salir de la lista, y la tarjeta no reabre con un hover viejo.

## Decisiones
- `// DECISION` en `track_sidebar_hover`: `drop.py` solo actualiza `_sidebar_hover` en `move` **con el botón pulsado** (`drop.py:2110-2111`), de modo que la sección anterior conserva el fondo de hover y se ve «activa» junto a la nueva. Se comprobó con el Python del `.venv`: para la misma secuencia de estados el cuadro sale **píxel a píxel idéntico** al de Rust antes del arreglo (dos filas marcadas: la activa en MINT al 14 % y la anterior en `GROUP_HOVER`). El brief pide corregirlo en Rust; el arreglo se limita al estado del puntero, sin tocar el dibujo (que ya pasa la paridad).
- `// DECISION` en `card_hand`: `on_pointer_move` de la ventana (fuera de alcance: `win/wndproc.rs`, `win/drop/card.rs`) solo llega con un botón pulsado, igual que en Python. `card_hand` es lo único que el bucle de ventanas llama en cada movimiento sin botón, así que ahí se refresca el hover de la barra; el resultado es el hover «de verdad» que el usuario espera.
- `// DECISION` en `toggle_menu`: limpiar el hover al abrir evita que una tarjeta reabierta muestre una fila resaltada con el puntero en otra parte.

## Verificación
- `cargo fmt --all`: ok.
- `cargo clippy -p ez-hud --all-targets -- -D warnings`: ok, sin avisos.
- `cargo test -p ez-hud`: **497** pruebas de librería + 9 binarios de integración, todas en verde, incluidos `hud_settings_parity` (1 caso), `hud_menu_parity` (1) y `hud_menu_hit_parity` (1).
- Prueba de regresión: falla con el código anterior (`git stash` de los dos módulos) y pasa con el arreglo.
- Python del `.venv`: sondeo de la misma secuencia de estados (`drop.py` + `settings_window.py`, menú real) para comparar píxeles; antes del arreglo Rust y Python coincidían exactamente, dos filas marcadas en los mismos pasos.
- Métricas: `dialog` de ajustes, filas marcadas tras pulsar otra sección: 2 → 1.

## Pendiente o riesgos
- Si el puntero sale de la ventana de la tarjeta por el borde de la lista, `WM_MOUSELEAVE` no tiene gancho en `win/drop/ui/**` (solo limpia el hover de la barra de la ventana); el hover de la sección se apaga con el primer movimiento dentro de la tarjeta, al pulsar o al reabrirla. Para cerrarlo del todo habría que tocar `win/wndproc.rs`, fuera del alcance del brief.
- No se añadieron casos de paridad nuevos (fase sin pruebas de paridad); los fixtures `parity/fixtures/hud_settings.jsonl` y `hud_menu.jsonl` solo se leyeron para conocer formatos.
- La sección anterior tampoco conserva «pulsado»: la barra lateral no tiene estado `pressed` en Python ni en Rust; lo único dinámico era el hover.

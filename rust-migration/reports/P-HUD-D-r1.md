# P-HUD-D — `hud_menu` al 100 % (cuadros del menú y de la ventana de ajustes)

## Resultado
- Brief/tarea: P-HUD-D
- Estado: completado
- `hud_menu`: **323/323** casos dentro de umbral (153 de lógica + 170 cuadros PNG).
- `cargo test -p ez-hud`: 485 pruebas unitarias + 7 binarios de paridad, todo en verde.
- No hizo falta corregir código de `ez-hud`: los 45 fallos iniciales eran huecos del arnés de la prueba.

## Cambios
- `rust/crates/ez-hud/tests/hud_menu_parity.rs`: recoge todos los fallos de cuadros y falla al final con la tabla completa (caso, PNG, max, p995, p999, ssim) en vez de abortar en el primero.
- `rust/crates/ez-hud/tests/hud_menu_parity/frames.rs`: corrige el reloj de los cuadros de animación (`trigger_s + muestra`, no la muestra como tiempo absoluto), separa el replay de cuadros fijos (`_still`) del de animaciones (`_animation`), y reproduce **todas** las muestras previas de una animación porque `draw` cambia el estado del pie y del botón Guardar. Ese era el fallo de `anim-save@33/50/100`.
- `rust/crates/ez-hud/tests/hud_menu_parity/support.rs`: reconstruye la sección de los cuadros desde `title`/`groups` (los `Menu.draw` de sección no guardan `section`), usa el `viewport` del oráculo cuando no hay `screen_h`, deduce `show_advanced` solo de los casos que lo pasan (no de los estados `settings-system`/`hotkey`), activa `theme=custom` en `select-theme_preset`, y hace que el callback de guardado devuelva `false` en `footer-error`/`save-error`.
- `rust/crates/ez-hud/tests/hud_menu_parity/recorder.rs`: `Recorder` extraído a su propio módulo para que `support.rs` quede por debajo de 600 líneas.
- `rust-migration/python-quirks.md`: añadido Q-PHUDD-01 (el degradado del scroll no se recorta al alto animado durante el cierre).

## Decisiones
- **Cuadros de sección sin `section`**: se busca la sección cuyo `section_title` y lista de grupos coinciden con `input.title`/`input.groups`; si no hay coincidencia (cuadros completos con título «AJUSTES») se dejan todos los grupos.
- **Viewport de pantalla pequeña**: `small-top`/`small-middle` no guardan `screen_h`; se fija `max_h = input.viewport` (equivalente a `visible_menu_height(480)` para el dibujo).
- **Valores no serializados en los cuadros**: además de los estados ya cubiertos por P-HUD-B, se deducen `show_advanced` por nombre de caso (`window-min-*`, `window-hotkeys-custom-*`, `window-capture-*`, `window-conflict-*`, `window-dirty-*`) y `theme=custom` para `select-theme_preset`.
- **Animaciones**: cada caso se reproduce desde 0 pasando por las muestras anteriores de `animation.samples_ms` y dibujando en cada una, como hace `parity/gen/hud_menu.py`; sin eso el pie no conserva el estado que el Python veía al pintar.
- **Sin cambios en `ez-hud`**: los valores de altura, interpolación, `round_ties_even`, orden de pintura y textos ya coincidían con el Python; los fallos restantes eran de reconstrucción de estado en la prueba.

## Verificación
- `cargo fmt --all --check`: limpio.
- `cargo clippy -p ez-hud --all-targets -- -D warnings`: limpio.
- `cargo test -p ez-hud`:
  - `src/lib.rs`: 485 passed.
  - `hud_bar_parity`, `hud_drop_model_parity`, `hud_geometry_parity`, `hud_menu_hit_parity`, `hud_menu_parity`, `hud_peek_parity`, `hud_settings_parity`: 1/1 cada uno.
  - `hud_menu_parity`: 323/323 (0 cuadros fuera de umbral).
- Fixtures consumidos: `parity/fixtures/hud_menu.jsonl` (323 casos; 170 cuadros contra `parity/golden/hud/{menu,settings}/*.png` y 153 casos de lógica/geometría/secuencias).
- Filas de `parity/test-map.csv`: no aplica; esta tarea es paridad de cuadros generados, no el port de pruebas unitarias de Python. Las filas de `test_hotkey_field.py`, `test_settings_window.py` y `test_save_button.py` que tocan el menú siguen `pendiente` y no forman parte de este brief.

### Cuadros por familia (170)

| Familia | Casos | En umbral | Peor `max` | Peor `p995` | Peor SSIM | Causa de la diferencia residual |
|---|---:|---:|---:|---:|---:|---|
| Reposo (`full-*`) | 10 | 10 | 148 | 0 | 0,999738 | Antialias de texto localizado (1–2 px) |
| Avanzado (`advanced-*`) | 4 | 4 | 155 | 0 | 0,999710 | Antialias de texto localizado |
| Secciones (`section-*`) | 11 | 11 | 140 | 0 | 0,999861 | Antialias de texto localizado |
| Scroll (`scroll-*`) | 4 | 4 | 0 | 0 | 1,000000 | Byte a byte |
| Pantalla pequeña (`small-*`) | 2 | 2 | 0 | 0 | 1,000000 | Byte a byte |
| Selectores (`select-*`) | 8 | 8 | 1 | 0 | 1,000000 | Un canal, 1 nivel |
| Edición (`edit-*`) | 4 | 4 | 66 | 0 | 0,999969 | Antialias del cursor/texto |
| Secreto (`secret-*`) | 3 | 3 | 42 | 0 | 0,999977 | Antialias de texto |
| Atajos (`hotkey-*`) | 4 | 4 | 1 | 1 | 1,000000 | Un canal, 1 nivel |
| Hover/pulsación (`hover-*`, `press-*`) | 8 | 8 | 0 | 0 | 1,000000 | Byte a byte |
| Pie (`footer-*`) | 7 | 7 | 0 | 0 | 1,000000 | Byte a byte |
| Animación abrir (dark/light) | 16 | 16 | 0 | 0 | 1,000000 | Byte a byte |
| Animación cerrar | 8 | 8 | 0 | 0 | 1,000000 | Byte a byte |
| Animación selector | 8 | 8 | 137 | 0 | 0,999759 | Antialias del selector |
| Animación toggle | 8 | 8 | 0 | 0 | 1,000000 | Byte a byte |
| Animación slider | 8 | 8 | 0 | 0 | 1,000000 | Byte a byte |
| Animación scroll | 8 | 8 | 1 | 0 | 1,000000 | Un canal, 1 nivel |
| Animación guardar | 8 | 8 | 0 | 0 | 1,000000 | Byte a byte |
| Animación probar voz | 8 | 8 | 0 | 0 | 1,000000 | Byte a byte |
| Animación medidor | 8 | 8 | 0 | 0 | 1,000000 | Byte a byte |
| Ventana de ajustes (`window-*`) | 25 | 25 | 40 | 1 | 0,999962 | Antialias de texto/iconos |
| **Total** | **170** | **170** | — | — | — | — |

Los `max` altos con `p995 = 0` son píxeles aislados de antialias de glifos; el umbral de texto (`p995 ≤ 2`, `SSIM ≥ 0,995`) los admite y ningún cuadro queda fuera.

## Pendiente o riesgos
- Nada pendiente del brief. Los `Menu.draw`/`settings_window` no serializan `values` ni el estado interno del pie; si se regeneran los fixtures con otros estados, el arnés tendrá que ampliar las deducciones de `support.rs` (hoy cubre todos los casos del archivo).
- Q-PHUDD-01 queda como rareza reproducida: el cierre deja ver el degradado del scroll por debajo del alto retraído. Cambiarlo exigiría regenerar `anim-close-*` y no forma parte de este brief.

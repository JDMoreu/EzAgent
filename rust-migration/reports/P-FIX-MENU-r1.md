## Resultado

- Brief/tarea: P-FIX-MENU
- Estado: completado

## Cambios

- `rust/crates/ez-hud/src/win/menu_panel.rs` (nuevo): `LiveMenuPanel` implementa `MenuPanel` delegando en el `Menu` de `model::menu` (tema, escala, movimiento reducido, alto/ancho, grupos, abrir/cerrar, `resize`, `set_testing`, `set_settings`, `set_llm_models`, `set_live_scale`, `on_applied`, nivel de entrada, `update`, `height`, `viewport_height`, `width`, `frame_key`, `is_animating`, `draw` con `draw_with(&mut Advanced)`, `hit`, puntero, rueda, teclado/edición/pegar, `finish_text_edit`) y `LiveMenuFactory` es la única fábrica real; los `MenuCallbacks` de la barra se traducen a los del modelo (guardar, cambio en vivo, probar voz, ocultar, cerrar, edición de texto, acción, copiar y captura de atajo, más `modifier_source`).
- `rust/crates/ez-hud/src/win/menu_panel/tests.rs` (nuevo): el cuadro del adaptador coincide byte a byte con `Menu::draw_with` en el mismo `Layer` y la tarjeta no queda vacía; abrir y pulsar «Guardar» llega al callback de guardado; `set_live_scale`/`set_llm_models` llegan a `MenuSettings`.
- `rust/crates/ez-hud/src/win/mod.rs`: declara `pub mod menu_panel;`.
- `rust/crates/ez-hud/src/win/drop/ui/menu.rs`: `_ensure_menu` usa `LiveMenuFactory` cuando nadie instala otra fábrica (la app y la demo no instalan ninguna, igual que el Python, que construye `Menu` directamente).
- `rust/crates/ez-hud/src/win/drop/ui/card_input.rs`: `_hide_from_menu` cierra con `try_lock` porque el mismo hilo ya cerró el menú en `activate_hit(Hide)`; sin el `try_lock` el callback se autobloquea (el Python es monohilo y su `close` repetido es inocuo).
- `rust/crates/ez-hud/src/win/drop/ui/tests.rs`, `.../tests/live_menu.rs` (nuevo): con la fábrica real `toggle_menu` abre el menú (`menu_open() == true`), `render_card()` devuelve un cuadro del tamaño de la tarjeta con píxeles, y un clic en «Guardar» llega a `DropHost::save_settings`.
- `rust/crates/ez-hud/src/model/menu/geometry.rs`: los callbacks del modelo pasan a `Send + Sync`; el menú real vive detrás de `Arc<Mutex<dyn MenuPanel>>` y cruza al hilo de la ventana de ajustes.
- `rust/crates/ez-hud/src/model/menu/{clock.rs,external.rs,edit/tests.rs,pointer/tests.rs,sliders/tests.rs,wheel/tests.rs,voice_test/tests.rs}`: las pruebas del modelo pasan de `Rc<RefCell>`/`Rc<Cell>` a `Arc<Mutex>`/`AtomicUsize` (misma cobertura, sin cambios de aserciones).
- `rust/crates/ez-app/src/app/core/init.rs`: `install_menu_provider` instala `hud_menu_settings` como `menu_settings_provider` en el arranque de `VoiceApp` (`app.py:571`), con `Weak` para no retener la app.
- `rust/crates/ez-app/src/app/core/tests.rs`: prueba de que, tras el arranque, `toggle_menu` llega a crear el menú (proveedor instalado + fábrica real), sin abrir ventanas.

## Decisiones

- `// DECISION`: callbacks del modelo con `Send + Sync` (el Python no los necesita). Alternativa descartada: `unsafe impl Send`, prohibido fuera de los módulos del §5 de `AGENTS.rust.md`.
- `// DECISION`: la fábrica real es el valor por defecto de `ensure_menu` en vez de un parámetro nuevo de `create_ui`/`DropUiOptions`: `lifecycle.rs` y `ui.rs` no están en la lista de archivos del brief, y así la app, la demo y el arnés de replay la obtienen sin cableado extra (el Python tampoco tiene fábrica). Una fábrica instalada explícitamente sigue ganando.
- `// DECISION`: `hide_from_menu` con `try_lock`; el `close` normal sigue igual y el repetido es inocuo, como en el Python.
- `// DECISION`: `modifier_source` sale de `win::settings_window::os_modifiers()` (el constructor del `Menu` de Python lo trae por defecto y `_ensure_menu` no lo pasa); sin él la captura de atajos perdería ctrl/alt/shift/win.
- `// DECISION`: `set_scale` ignora el error del modelo, porque `toggle_menu` ya atrapaba el `ValueError` de una escala no positiva.
- `// DECISION`: `frame_key` del contrato (`u64`) es el hash `DefaultHasher` del `FrameKey` del modelo (`frame_key` se usa solo como clave de caché del cuadro quieto).
- `// DECISION`: `config_from_setting` publica `SettingValue::None` como texto vacío: `ConfigValue` no tiene nulo y ninguna clave editable vale `None` al cambiar en vivo.
- `// DECISION`: las pruebas nuevas abren el menú con `set_motion("reduced")` en vez de dormir 320 ms; la tarjeta se pinta y se puede pulsar en el mismo instante.

## Verificación

- `cargo fmt --all` (y `--check`): ok.
- `cargo clippy -p ez-hud --all-targets -- -D warnings`: ok.
- `cargo clippy -p ez-app --all-targets -- -D warnings`: ok.
- `cargo test -p ez-hud`: 493 pruebas unitarias + 8 de integración, todas en verde; incluye `hud_menu_parity`, `hud_menu_hit_parity` y `hud_settings_parity`. 10 ejecuciones seguidas sin fallos.
- `cargo test -p ez-app`: 290 unitarias + 4 de integración, todas en verde.
- Prueba real: `cargo build --release -p ez-app --bin ezagent-voice` y luego `ezagent-voice.exe --demo-ui` con las DLL de `run-dev.cmd`. Sondeando las ventanas de nivel superior: la barra (`drop`) aparece al inicio; la tarjeta (`card`) aparece **visible a los 18,5 s y desaparece a los 21,0 s**, es decir, exactamente en los dos pasos «menu» de la demo (uno abre, el otro pliega); en «close» se oculta la barra y al terminar se destruyen las ventanas. La captura de pantalla de ese instante muestra la tarjeta completa: cabecera «AJUSTES» con el botón «–», barra lateral (Apariencia/Escucha/Voz/Modelos/Sistema), «Aspecto» con Forma=Píldora, Tamaño=100 %, Movimiento=Según el sistema y Tema=Automático con las tres muestras de paleta, y el pie con «Guardar» y «TODO GUARDADO». Que se vean las filas de deslizador, selector y paleta confirma que el adaptador pinta con `draw_with` (con `draw` saldrían en blanco).
- Fixtures: `parity/fixtures/hud_menu.jsonl` (323 casos), `hud_menu_hit.jsonl` (846) y `hud_settings.jsonl` (1 175) se consultaron para el formato; esta fase no escribe pruebas de paridad, así que no se consumen casos nuevos. Las filas de `parity/test-map.csv` afectadas (`tests/test_save_button.py::test_menu_live_changes_go_through_the_non_persisting_path`, `test_settings_window.py::test_menu_resize_updates_dimensions_immediately`, `test_hotkey_field.py::test_closing_the_menu_ends_the_capture`, entre otras) siguen marcadas `pendiente` y les corresponde la fase de paridad.

## Pendiente o riesgos

- El brief pedía «pásala al construir la barra en la app y en la demo»; se resolvió como valor por defecto en `ensure_menu` (una línea) en lugar de añadir un parámetro a `create_ui`/`DropUiOptions`, que están fuera de los archivos permitidos. App y demo la usan; si una tarea futura quiere inyectar otra, ya puede hacerlo por `DropUiOptions.menu_factory`.
- Riesgo conocido y preexistente (no lo toca este cambio): las pruebas de `win::drop::ui` publican la paleta global con `DropUI::set_theme`/`set_custom_base` sin el guard de `tokens::test_guard`, así que `theme::tests` puede fallar en paralelo si se añaden pruebas largas. Las nuevas no escriben la paleta (el cuadro del adaptador se reintenta si su versión cambia) y la suite quedó verde 10/10.
- Las filas de ajustes dependientes de F5-12 (`hotkey_field`) siguen con el `ModifierSource` del sistema: la captura de atajos usa `os_modifiers()`, pero no se revalidó a mano el flujo completo de grabar un atajo en la app real.

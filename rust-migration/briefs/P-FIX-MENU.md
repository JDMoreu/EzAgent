# P-FIX-MENU — El menú de ajustes no se abre en la app real: adaptador vivo de MenuPanel/MenuFactory y proveedor de ajustes
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- src/ezagent/voice/hud/drop.py (1664-2235: `toggle_menu`, `_ensure_menu`, `_close_menu`, reparto del puntero, tarjeta de ajustes) y src/ezagent/voice/hud/menu.py (la clase `Menu` que la barra crea y dibuja)
- src/ezagent/voice/app.py: `_hud_menu_settings` (lo que se instala como `hud.menu_settings_provider`), los callbacks del menú y `handle_menu_event`
- Rust ya portado: `rust/crates/ez-hud/src/model/menu*` (modelo `Menu`), `rust/crates/ez-hud/src/paint/menu*` (pintura; **usa `Menu::draw_with(buf, now, &mut Advanced)`**, no `draw`, o las filas avanzadas salen en blanco: informes `rust-migration/reports/F5-10a-r1.md` y `F5-10b-r1.md`), los contratos `MenuPanel`, `MenuFactory` y `MenuCallbacks` en `rust/crates/ez-hud/src/win/drop/ui/traits.rs` y su uso en `win/drop/ui/menu.rs` y `card_input.rs`; la prueba falsa `FakeMenuFactory` en `win/drop/ui/tests/support.rs`
- `rust/crates/ez-app/src/app/menu_wiring.rs` y `menu_wiring/*` (F6-06b), `rust/crates/ez-app/src/app/settings_apply.rs` (`toggle_menu`), `app/demo.rs` (la demo instala un proveedor pero tampoco tiene fábrica), y dónde se crea la barra (`create_ui` / `DropOptions` / `AppParts.hud_factory`)

## Archivos que se tocan (solo estos)
- rust/crates/ez-hud/src/win/menu_panel.rs (nuevo; o `win/drop/ui/live_menu.rs`) con la implementación real de `MenuPanel` sobre el `Menu` de `model/menu` y la pintura de `paint/menu`, y una `MenuFactory` real que lo crea; su declaración en el `mod.rs` correspondiente
- rust/crates/ez-hud/src/win/drop/ui/** y rust/crates/ez-hud/src/model/menu*, rust/crates/ez-hud/src/paint/menu* solo si hace falta un cambio mínimo y aditivo (p. ej. exponer algo `pub`)
- rust/crates/ez-app/src/app/menu_wiring.rs, rust/crates/ez-app/src/app/menu_wiring/*.rs, rust/crates/ez-app/src/app/core/*.rs (pasar la fábrica real a la barra e instalar `set_menu_settings_provider` con `hud_menu_settings`), rust/crates/ez-app/src/app/demo.rs (que la demo también tenga fábrica)
- rust-migration/reports/P-FIX-MENU-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- parity/fixtures/hud_menu.jsonl
- parity/fixtures/hud_menu_hit.jsonl
- parity/fixtures/hud_settings.jsonl

## Notas específicas
- **Síntoma real (reportado por el usuario):** en la app Rust, pulsar el botón de menú de la barra (o el atajo `ctrl+alt+p`, o «panel» del menú de bandeja) no abre nada. **Causa ya localizada:** `DropUI::toggle_menu` (`win/drop/ui/menu.rs`) vuelve sin hacer nada porque (1) la app real nunca llama a `set_menu_settings_provider` (solo la demo) y (2) **no existe ninguna implementación real de `MenuFactory`/`MenuPanel`** (solo `FakeMenuFactory` de pruebas), así que `ensure_menu` devuelve `None`. F5-05 dejó escrito que F6 debía aportar el adaptador y nadie lo hizo.
- **Qué hacer:** implementa `MenuPanel` delegando en el `Menu` real (tema, escala, movimiento reducido, alto disponible, ancho, alto máximo, grupos/sección, abrir/cerrar, `resize`, `set_testing`, `set_settings`, `set_llm_models`, `set_live_scale`, `on_applied`, `set_input_level`, `update`, `height`, `viewport_height`, `width`, `frame_key`, `is_animating`, `draw` con `draw_with` y las filas avanzadas, `hit`, puntero, rueda, teclado/edición de texto/pegar, `finish_text_edit`), con `MenuCallbacks` conectados a los callbacks del `Menu` (guardar, cambio en vivo, probar voz, ocultar, cerrar, edición de texto, acción, copiar, captura de atajo) igual que `drop.py` conecta los de `menu.py`. Crea la `MenuFactory` real, pásala al construir la barra en la app y en la demo, e instala el proveedor `hud_menu_settings` en el arranque de `VoiceApp`.
- **Pruebas:** que `cargo test -p ez-hud` y `cargo test -p ez-app` sigan en verde (incluidas `hud_menu_parity`, `hud_menu_hit_parity` y `hud_settings_parity`) y añade pruebas nuevas: con la fábrica real, `toggle_menu` abre el menú (`menu_open() == true`), se dibuja algo no vacío en la tarjeta, un clic en «Guardar» llega al `DropHost::save_settings`, y un cuadro dibujado por el adaptador coincide con el dibujado directamente por `Menu::draw_with` (mismo `Layer`). **Comprobación real:** ejecuta `rust/target/release/ezagent-voice.exe --demo-ui` (tras `cargo build --release -p ez-app --bin ezagent-voice`; las DLL se toman con `rust/run-dev.cmd --demo-ui`): en los pasos «menu» de la demo el menú debe abrirse; descríbelo en el informe (no hace falta captura).
- Respeta `// SAFETY:` y las demás reglas; nada de `unwrap` fuera de pruebas.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/P-FIX-MENU-r1.md` y un commit en tu rama.

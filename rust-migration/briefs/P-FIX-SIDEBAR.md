# P-FIX-SIDEBAR — Ajustes: al cambiar de sección quedan dos secciones marcadas como activas a la vez
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- src/ezagent/voice/hud/settings_window.py (barra lateral de secciones: elemento activo, hover, pulsación y cualquier animación del resaltado al cambiar de sección) y drop.py (cómo la barra cambia de sección: `_section`, `set_groups`, reparto del puntero de la tarjeta)
- Rust: `rust/crates/ez-hud/src/paint/settings_window*`, `rust/crates/ez-hud/src/win/settings_window.rs`, `rust/crates/ez-hud/src/win/drop/ui/{menu,card_input}.rs` y el estado de hover/activo de la barra lateral

## Archivos que se tocan (solo estos)
- rust/crates/ez-hud/src/paint/settings_window* , rust/crates/ez-hud/src/win/settings_window.rs, rust/crates/ez-hud/src/win/drop/ui/** (solo lo de la barra lateral y el cambio de sección)
- pruebas nuevas en `ez-hud`
- rust-migration/reports/P-FIX-SIDEBAR-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- parity/fixtures/hud_settings.jsonl
- parity/fixtures/hud_menu.jsonl

## Notas específicas
- **Síntoma real (reportado por el usuario, app Rust):** en el panel de Ajustes, al seleccionar una sección de la barra lateral se ven **dos secciones marcadas a la vez**: la nueva y otra que se queda con el aspecto de activa (el usuario lo describe como si el «active» se quedara en otra sección mientras cambia). En Python solo queda marcada la sección actual. Los cuadros quietos de las secciones pasan la paridad, así que es un problema de **estado dinámico**: hover que no se limpia al salir o al cambiar de sección, la pulsación (`pressed`) que no se suelta, la sección anterior que no se des-resalta, o una animación de transición del resaltado que no termina o no se interpola como en Python.
- **Qué hacer:** reproduce la secuencia del usuario en una prueba (abrir el menú, mover el puntero sobre la sección A, pulsar, mover a la sección B, pulsar, y luego a C; dibujar cuadros en cada paso) y compara con lo que haría Python en la misma secuencia (estado de hover/activo/pulsado por elemento y, si puedes, los píxeles de la barra lateral con el renderizado de `settings_window.py` del `.venv`). Corrige la causa en Rust y deja la prueba que lo fija: tras cambiar a una sección, **solo** esa aparece como activa y ninguna otra conserva hover/pulsado sin el puntero encima.
- `hud_settings_parity`, `hud_menu_parity`, `hud_menu_hit_parity` y todo `cargo test -p ez-hud` en verde; `cargo clippy -p ez-hud --all-targets -- -D warnings`.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/P-FIX-SIDEBAR-r1.md` y un commit en tu rama.

# PEND-IMG — Imágenes del chat en todos los formatos que abre el Python (no solo PNG)
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- `src/ezagent/voice/chat/**` y `src/ezagent/voice/hud/chat_*.py`: qué formatos de imagen acepta y muestra el chat (Pillow: JPEG, GIF, WebP, BMP…), cómo se escalan y qué se muestra si no se puede abrir
- Rust: `rust/crates/ez-hud/src/paint/chat/render_rich/**` (fichas de imagen), `rust/crates/ez-app/src/app/chat_wiring/host.rs` (lectura de medios), la nota «imágenes del chat solo PNG» en `rust-migration/reports/code-first-progress.md`; el crate `image` está en el workspace solo con la feature `png`

## Archivos que se tocan (solo estos)
- rust/Cargo.toml (**solo** para añadir al crate `image` las features de los formatos que abre el Python: `jpeg`, `gif`, `webp`, `bmp` y las que haga falta) y rust/Cargo.lock
- rust/crates/ez-hud/src/paint/chat/** y rust/crates/ez-app/src/app/chat_wiring/** (decodificación y medios)
- pruebas nuevas (con imágenes de prueba pequeñas generadas con Pillow del `.venv` en un directorio temporal o en `rust/crates/ez-hud/tests/data/`) y rust-migration/reports/PEND-IMG-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- parity/fixtures/hud_chat.jsonl

## Notas específicas
- Que el chat muestre JPEG, GIF (primer cuadro, como Pillow), WebP y BMP igual que el Python: mismo escalado, misma ficha y el mismo comportamiento ante un archivo corrupto o un formato no soportado. Compara el resultado con Pillow (decodifica las mismas imágenes con el `.venv` y compara los píxeles con tolerancia de `parity/thresholds.toml`). `hud_chat_parity` debe seguir en verde sin tocar sus PNG. `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p ez-hud -p ez-app` en verde.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/PEND-IMG-r1.md` y un commit en tu rama.

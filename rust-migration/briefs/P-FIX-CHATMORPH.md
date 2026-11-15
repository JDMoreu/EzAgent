# P-FIX-CHATMORPH — El chat abre sin la animación de Python: el morph tarda ~60 ms por cuadro en release (presupuesto 16,7 ms)
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- src/ezagent/voice/hud/chat_ui.py (`ChatPanel`: `open`, `_render`, caché de base y contenido, `_compose`) y chat_paint.py; src/ezagent/voice/hud/drop.py (`chat_fluid`)
- Rust: `rust/crates/ez-hud/src/win/chat/compose.rs` y `compose/*` (la composición del morph), `win/chat/window.rs` (`render`, `frame_interval`), `win/chat/window/anim.rs` (`MORPH_SECS = 0.28`), `rust/crates/ez-app/src/app/chat_wiring/ui.rs` (`chat_fluid` convierte a `Array3` con `from_shape_fn` en cada cuadro), y lo que llamen de `ez-gfx` (desenfoque, composición, máscaras)
- La prueba de paridad `rust/crates/ez-hud/tests/hud_chat_parity/morph.rs` (cómo construir una escena de morph con una UI falsa)

## Archivos que se tocan (solo estos)
- rust/crates/ez-hud/src/win/chat/** y rust/crates/ez-hud/src/paint/chat/**
- rust/crates/ez-gfx/src/** **solo** si el cuello de botella está ahí, con cambios que no alteren ningún píxel (las pruebas de paridad de `ez-gfx` y `ez-hud` deben seguir en verde)
- rust/crates/ez-app/src/app/chat_wiring/ui.rs (la conversión de `chat_fluid`)
- rust/crates/ez-hud/benches/ o una prueba `#[ignore]` de medición en rust/crates/ez-hud/tests/chat_morph_timing.rs (nueva)
- rust-migration/reports/P-FIX-CHATMORPH-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- parity/fixtures/hud_chat.jsonl

## Notas específicas
- **Síntoma real (reportado por el usuario):** al abrir el chat en la app Rust no se ve la animación de la versión Python (la forma de la barra que se transforma en el panel). **Medición ya hecha** (release, ventana 472×672, `compose_frame` como en `hud_chat_parity/morph.rs`): p ≤ 0,25 tarda ~3 ms, pero p ≥ 0,5 tarda **58-67 ms por cuadro**; la variante reducida, 19 ms. Con `MORPH_SECS = 0.28` caben 4-5 cuadros en vez de ~17: la animación prácticamente desaparece. El código del morph es equivalente al de Python y sus cuadros pasan la paridad; el problema es de **rendimiento**.
- **Qué hacer:** (1) escribe una medición reproducible (`tests/chat_morph_timing.rs` con `#[ignore]` o un bench) del camino **real** de un cuadro de apertura y de cierre: `ChatPanel::render` / `compose::compose` con su caché de contenido, a 100 %, 125 % y 150 % de escala; mide también el Python equivalente (`chat_ui.py`) con el intérprete del proyecto para tener la referencia; (2) perfila y encuentra dónde se van los ~60 ms (sospechosos: desenfoques/sombras recalculados por cuadro, el fluido `chat_fluid` y su conversión `from_shape_fn`, composición a tamaño completo de capas que solo cambian de escala, contenido re-rasterizado en cada cuadro); (3) optimízalo sin cambiar un solo píxel: cachea lo invariante entre cuadros como hace el Python, evita copias, usa los recorridos por filas contiguos. **Meta:** cada cuadro del morph ≤ 10 ms en release a 100 % y ≤ 16 ms a 150 %, con `cargo test -p ez-hud --test hud_chat_parity` y todo `cargo test -p ez-hud -p ez-gfx` en verde, sin relajar umbrales.
- Informe: tabla antes/después por p (0, 0,25, 0,5, 0,75, 1) y escala, el tiempo de Python como referencia, qué se cacheó u optimizó y por qué no cambia la salida.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/P-FIX-CHATMORPH-r1.md` y un commit en tu rama.

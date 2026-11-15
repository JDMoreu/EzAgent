# P-FIX-MORPH2 — El chat abre y cierra sin la transformación esfera/píldora ↔ panel de la versión Python
Excepción a AGENTS.md: rama de migración a Rust; aplica `rust-migration/AGENTS.rust.md`.
**Antes de nada lee `rust-migration/briefs/_COMMON.md` y `rust-migration/AGENTS.rust.md`.**

## Fuente Python (la especificación, léela completa)
- src/ezagent/voice/hud/chat_ui.py COMPLETO: `open` (fíjate en que `self.anim = _Anim("opening", time.monotonic(), 0.0)` se fija **después** de `self.view.render(...)` y de construir `ChatWindow`), `close`, `_render`, `_step`/`_elapsed_for`, `_interval`, cómo el bucle de la ventana pide cuadros y cuándo se oculta/muestra la forma de la barra (`set_form_hidden`)
- Rust: `rust/crates/ez-hud/src/win/chat/window/lifecycle.rs` (`open`: hoy toma `now = monotonic()` **antes** de `prepare_view` —que pinta todo el historial— y de `window.start()`, y usa ese `now` como `t0` en `install_view`), `win/chat/window.rs` (`render`, `frame_interval`, `set_interval`), `win/chat/window/anim.rs` (`step`, `MORPH_SECS = 0.28`, `CLOSE_SECS = 0.20`), `win/chat/window/chrome.rs` (bucle de pintado de la ventana: cuándo despierta, cuándo pinta) y `win/layered*` (presentación); la barra: `win/drop/ui/appearance.rs` (`set_form_hidden`) y su pintado

## Archivos que se tocan (solo estos)
- rust/crates/ez-hud/src/win/chat/** y rust/crates/ez-hud/src/win/layered*.rs / rust/crates/ez-hud/src/win/layered/** si la causa está en el bucle de pintado
- rust/crates/ez-hud/src/win/drop/ui/** solo si hace falta (la forma de la barra durante el morph)
- pruebas nuevas en `ez-hud`
- rust-migration/reports/P-FIX-MORPH2-r1.md

Los módulos de tu crate ya existen como esqueleto vacío y están declarados en
su `lib.rs`; reemplaza el contenido. Si un módulo se pasa de 600 líneas,
divídelo en una carpeta con submódulos (`foo.rs` + `foo/bar.rs`).

## Fixtures de referencia (solo para entender formatos; no hay pruebas de paridad en esta fase)
- parity/fixtures/hud_chat.jsonl

## Notas específicas
- **Síntoma real (reportado por el usuario, app Rust en release con su historial real):** al abrir el chat no se ve la transformación de la esfera/píldora en el panel, y al cerrarlo tampoco se ve el panel encogiéndose de vuelta a la forma; en Python ambas se ven. Los cuadros del morph ya son rápidos (P-FIX-CHATMORPH: ≤ 5 ms a 100 %), así que el problema es de **temporización/ciclo de vida**, no de velocidad de composición.
- **Hipótesis fuerte para la apertura (verifícala):** `open` fija `t0` antes del trabajo caro (pintar el historial real en `prepare_view` y crear/arrancar la ventana nativa); si eso tarda más de 0,28 s, el primer cuadro ya sale con `p = 1` y la animación desaparece. El Python fija `t0` después. **Para el cierre:** averigua por qué no se ven los cuadros intermedios (¿el bucle de la ventana no se despierta tras `close()`?, ¿pinta a 30 fps o espera un cambio?, ¿se retira la ventana antes de pintar?, ¿la forma de la barra reaparece de golpe?).
- **Mide en real, no supongas:** añade una traza de depuración (por ejemplo `tracing::debug!` con `p`, estado y marca de tiempo por cuadro pintado, o un contador en una prueba con ventana falsa) y comprueba que la apertura y el cierre pintan **cuadros intermedios** a ~60 fps durante `MORPH_SECS`/`CLOSE_SECS`. Escribe pruebas que lo fijen: abrir con un historial grande (cientos de mensajes) sigue produciendo cuadros con `0 < p < 1`; cerrar produce cuadros con `0 < p < 1` antes de retirar la ventana. Comprobación real opcional: un ejemplo temporal (no commiteado) con la configuración real que abra y cierre el chat y registre los `p` de los cuadros presentados.
- `hud_chat_parity` y todo `cargo test -p ez-hud` deben seguir en verde; `cargo clippy -p ez-hud --all-targets -- -D warnings`.

## Criterios de aceptación
- Todo lo público del Python está portado, con los mismos textos y valores por defecto.
- `cargo fmt --all`, `cargo clippy -p <crate> --all-targets -- -D warnings` y
  `cargo test -p <crate>` limpios, ejecutados desde `rust/`.
- Informe en `rust-migration/reports/P-FIX-MORPH2-r1.md` y un commit en tu rama.

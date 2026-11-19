# DX-1c — Nota de relevo (de mcode a OpenCode)

mcode se quedó sin créditos a mitad de DX-1c. Retomas su trabajo en `work/DX-1c`
(worktree `EzAgent-wt/DX-1c`). Lee `DX-1c.md` (el encargo) y esta nota.

## Lo que ya hay en la rama (de mcode, sin informe)
- `888b91e` virtualización del chat (fase B) y `ee02fd7` (avance): el alto de cada mensaje
  se **mide** sin pintar (`model/chat/view/measure.rs`, `paint/chat/render.rs` partido en
  `stack_blocks` + `render_blocks` + `measure_blocks`), y `draw.rs` solo maqueta el
  viewport ± una pantalla. Prueba nueva `ez-hud/tests/chat_virtualization.rs`.
- No hay informe ni mediciones antes/después. Revisa que lo hecho compile y pase antes de
  seguir.

## Lo que cambió en `eco/main` desde que la rama nació (tienes que integrarlo primero)
- DX-1b se revirtió por una caída al abrir el chat y **volvió** con su corrección **DX-1d**:
  hook de pánicos (`ez-core/src/panics.rs`), `catch_panic` en los callbacks de Windows,
  `paint_content` que recupera la vista si la maquetación entra en pánico
  (`win/chat/window.rs`), `place_caret` que no indexa líneas vacías
  (`model/chat/footer.rs`) y `composite_caret` que recorta contra la capa de contenido.
- DX-2 (acuses cortos) y otras olas (NV-1, IR-1, HD-10) que no tocan el chat.

**Paso 1**: `git merge eco/main` en tu rama. Si hay conflictos, **conserva los arreglos de
DX-1d** y adapta lo tuyo encima. Corre las pruebas de `ez-hud` y `ez-app` después de la
fusión y antes de seguir.

## Regla nueva de interfaz (no negociable)
Un cambio del chat no se da por bueno sin la prueba de humo de la app real. Usa los
ejemplos que dejó DX-1d, en release:
`cargo run --release -p ez-app --example diag_chat_smoke -- chat` y `-- real`, que deben
terminar con `RESULT chat_open=true alive_secs=10`. Añade un modo o un caso con un
historial grande (al menos 300 mensajes sintéticos con markdown, código, emojis y mensajes
vacíos) y desplazamiento hasta arriba y hasta abajo: vivo, sin `PÁNICO` en el registro
temporal. Nada pasa de 1 GB.

## Entrega
Lo que pide `DX-1c.md` (tablas antes/después con 10, 100 y 1 000 mensajes) + el humo de
arriba con salida literal. Informe `rust-migration/reports/DX-1c-r1.md`, commit en
`work/DX-1c`. Compila en primer plano y espera el resultado. No toques la app abierta de la
persona (PID 19576) ni su `whisper-server`.

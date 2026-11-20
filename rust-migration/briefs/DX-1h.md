# DX-1h — La prueba de la sombra única falla en depuración (en release pasa)

**Lee `rust-migration/reports/DX-1f-r1.md` y `DX-1g-r1.md`.** Rama `work/DX-1h` desde
`eco/main` (ya tiene DX-1f + DX-1g).

## Lo que pasa (comprobado por el agente principal, 20-nov 00:50)
`cargo test -p ez-hud --lib the_panel_paints_one_soft_shadow` **falla siempre en debug** y
**pasa en release**. En debug, en el estado `Open`, el perfil del borde no tiene sombra:
`[0, 0, …, 0, 255, 255, …]` (de cero pasa directo al cuerpo), y salta la aserción «the
shadow is stepped, not soft (Open)» (`ez-hud/src/win/chat/window/tests.rs:785`).

## Qué se hace
1. Averigua por qué en debug no hay sombra en `Open`: si es un artefacto de la prueba
   (tiempos de reloj real, progreso del morph, cuadro tomado antes de componer, caché) o si
   el producto de verdad no pinta la sombra en alguna condición (eso sería un fallo del
   producto, y el arreglo va ahí). Evidencia en el informe.
2. Arréglalo para que la prueba sea determinista y pase **en debug y en release** sin
   aflojar lo que comprueba (una sola sombra, suave, pico ≤ 40). Comprueba que una segunda
   llamada a `shadow_under` la sigue haciendo fallar en los dos modos.
3. `cargo test -p ez-hud` (debug) y `cargo test --release -p ez-hud` en verde, y humo `chat`
   y `anim` en release.

## Archivos
`ez-hud/src/win/chat/*` (solo si el fallo es del producto) y sus pruebas. Informe
`rust-migration/reports/DX-1h-r1.md`, commit en `work/DX-1h`. Compila en primer plano, sin
dos `cargo` a la vez. No toques la app de la persona.

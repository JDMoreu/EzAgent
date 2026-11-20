# DX-7 — Las pruebas con ventanas se abren en la pantalla secundaria

**Lee `AGENTS.md` y `rust-migration/briefs/_COMMON-ECO.md`.** Rama `work/DX-7` desde
`eco/main`.

## Pedido de la persona (20-nov)
«Que las pruebas se ejecuten en la pantalla secundaria y no en la primaria, para que no me
estorben mientras trabajo.»

Pantallas de la persona hoy: `DISPLAY1` principal 1920×1080 en (0,0); `DISPLAY2`
secundaria **vertical** 1080×1920 en (−1080,−393).

## Lo que pasa hoy
`ez-hud/src/win/monitor.rs` `work_area()` usa `SPI_GETWORKAREA` (siempre la principal); la
barra se coloca con `default_position` y se recorta con `clamp_position`/`clamp_pos`
contra esa área (`win/drop/ui.rs`), así que una posición guardada en la secundaria vuelve
a la principal. El chat y la vista rápida se colocan respecto de la barra.

## Qué se hace
1. Una variable **solo para pruebas**, `EZAGENT_TEST_MONITOR=secondary` (`# DECISION`): si
   está, el área de trabajo que usan la barra, el chat, el menú y la vista rápida es la del
   primer monitor **no principal** (`EnumDisplayMonitors` + `GetMonitorInfoW`, bandera
   `MONITORINFOF_PRIMARY`). Si no hay secundaria, aviso por `tracing` y la principal. Sin la
   variable, nada cambia (la app de la persona sigue igual). Revisa que **todos** los
   cálculos de posición y recorte usen esa área (no queden ventanas en la principal).
2. Todos los humos la ponen **por omisión**: `ez-app/examples/diag_chat_smoke.rs`,
   `diag_subagents_smoke.rs`, `ez-hud/examples/panic_boundary.rs` y
   `scripts/tests/humo-whisper.ps1` (que lanza la app real: pásale la variable al proceso).
   Con `--primary` (o `EZAGENT_TEST_MONITOR=primary`) se pueden ver en la principal.
3. El humo imprime en qué monitor quedaron sus ventanas (rectángulo de la barra y del
   chat) y **falla** si alguna ventana suya cae fuera del monitor pedido.
4. Pruebas de unidad de la elección del monitor (con monitores falsos: con y sin
   secundaria, secundaria vertical a la izquierda con coordenadas negativas).

## Archivos
`ez-hud/src/win/monitor.rs` y lo mínimo de `win/drop/ui.rs`/`win/chat/window/*`/`win/peek/*`
que calcule posición con el área (coordina: DX-1i toca la animación del chat y DX-6 la
barra/vista rápida; no cambies su lógica, solo el área que usan), los ejemplos y el guion
de humo nombrados, y sus pruebas.

## Reglas
Puedes abrir ventanas **solo en la pantalla secundaria** y solo para comprobar este
cambio, con el humo ya apuntando a ella; nunca en la principal. No toques la app de la
persona, su `whisper-server` ni la ventana de Herdr. Compila en primer plano, sin dos
`cargo` a la vez.

## Entrega
fmt, clippy y test de `ez-hud` y `ez-app` en verde; salida del humo `chat` mostrando que la
barra y el chat quedaron en `DISPLAY2`. Informe `rust-migration/reports/DX-7-r1.md`, commit
en `work/DX-7`.

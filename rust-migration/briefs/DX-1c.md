# DX-1c — Fase B del chat: maquetar solo lo que se ve (virtualización)

**Lee `AGENTS.md`, `rust-migration/reports/DX-1-r1.md` §4 paso 2 y
`rust-migration/reports/DX-1b-r1.md` (fase A, ya integrada).** Rama desde `eco/main`.

## Problema que queda
Tras la fase A la app ya no se congela, pero el primer render del chat sigue costando
~74 ms por mensaje (en el hilo de pintura): con 33 mensajes el contenido tarda ~2,4 s en
aparecer, con 100 ~7 s. `draw_messages` (`ez-hud/src/model/chat/view/draw.rs`) maqueta
todos los mensajes antes de saber cuáles se ven.

## Qué se hace
- Alto de cada mensaje **sin rasterizar** (medición de texto por bloque) para el
  desplazamiento total; maquetar y rasterizar solo los mensajes del viewport ± un margen
  (una pantalla arriba y abajo).
- Zonas clicables y selección solo para lo maquetado; al desplazarse se maqueta lo que
  entra (con la caché de la fase A).
- La paridad visual (`hud_chat_parity*`, `parity/thresholds.toml` sin tocar) debe seguir
  igual: si un caso de paridad usa un historial que no cabe en pantalla, explica la
  diferencia y adapta solo esa prueba con justificación.

## Pruebas (acotadas, < 1 GB)
Abrir con 1 000 mensajes cortos: el primer cuadro maqueta como mucho lo visible + margen
(contador de `layouts()`), y su tiempo no crece con el historial (10 vs 1 000 mensajes,
mismo orden de magnitud). Desplazarse hasta arriba maqueta lo que entra. Medición
antes/después con `examples/diag_chat_history_cost.rs` (10, 100 y 1 000; corte de 1 GB).

## Criterios
fmt, clippy y test de `ez-hud` y `ez-app` en verde. Informe `rust-migration/reports/DX-1c-r1.md`
con las tablas de antes y después.

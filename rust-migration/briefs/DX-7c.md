# DX-7c — Mantener toda la ventana del chat en la secundaria

Continúa `work/DX-7`; lee `AGENTS.md`, `_COMMON-ECO.md`, DX-7 y DX-7b. La validación independiente pasó formato, clippy y pruebas de `ez-app`/`ez-hud` con un hilo, y las dos mutaciones fueron detectadas. El humo real `diag_chat_smoke chat`, con `EZAGENT_TEST_MONITOR=secondary`, confirmó `DISPLAY2=(-1080,-393,0,1527)` y barra `(-671,1469,-409,1522)` dentro; el chat quedó `(-773,792,-307,1528)`, un píxel fuera. Salió con código 1; sin pánico.

Corrige la colocación o dimensiones reales para que toda la ventana nativa del chat, incluidos márgenes y sombras, quede dentro del monitor. No relajes la prueba ni alteres la animación de DX-1i. Añade prueba de límites que cubra este caso. El alcance es el de DX-7. Sin ventanas ni agentes reales; no compiles ni ejecutes pruebas para cuidar los recursos. Deja commit e informe `DX-7c-r1.md` con lo pendiente.

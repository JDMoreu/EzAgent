# DX-1i — La apertura del chat debe ser el cierre al revés (mismo color y estilo, rápida)

**Lee `AGENTS.md`, `rust-migration/briefs/_COMMON-ECO.md` y los informes
`DX-1f-r1.md`, `DX-1g-r1.md` y `DX-1h-r1.md`.** Rama `work/DX-1i` desde `eco/main`.

## Lo que ve la persona (20-nov, app recompilada con DX-1f + DX-1g)
«La animación al abrir el chat no tiene el mismo color y estilo que el hub. Debería ser
igual a la animación que hace cuando se cierra el chat, pero a la inversa, donde da la
impresión de que el hub se transforma en el chat, y que sea rápida.»

El cierre ya se ve bien: el chat se transforma en la barra (el «hub»). La apertura debe
ser **exactamente ese recorrido al revés**: arranca con la forma, el tamaño, la posición,
el color y el estilo de la barra tal como está en pantalla (su forma `pill`/`circle`, su
tema, su escala) y se transforma en el panel del chat.

## Qué se hace
1. **Diagnóstico con evidencia**: con el diario de cuadros de DX-1f y el arnés de
   `diag_chat_smoke` (solo ventanas de su propio proceso), guarda los cuadros presentados
   de la apertura y del cierre (PNG en `rust-migration/reports/DX-1i/`, más una tira con
   la apertura junto al cierre invertido). Mide para cada uno: duración, número de
   cuadros, curva de progreso, y de qué forma, tamaño, posición y color parte la apertura
   frente a dónde termina el cierre. Explica qué rama del código hace que difieran.
2. **Arreglo**: la apertura usa el mismo morph que el cierre con el progreso invertido
   (misma geometría de la barra al empezar, mismos colores y tema, misma curva invertida),
   en la **misma duración que el cierre** como mucho (rápida; indica en el informe los ms
   de antes y después). El contenido del chat aparece al final, como hoy. Sin volver a
   maquetar en el hilo de la interfaz (DX-1b), sin pánicos (DX-1d), con la apertura
   animada (DX-1f) y una sola sombra (DX-1g).
3. **Prueba automática**: un modo del humo (p. ej. `anim-espejo`) o una prueba de unidad
   que compare cada cuadro de la apertura con el cuadro equivalente del cierre invertido
   (forma, tamaño, posición y color medio; tolerancia razonable y documentada) y **falle**
   si la apertura parte de otro color o estilo, o si dura más que el cierre.
4. Paridad (`hud_chat_parity`, umbrales sin tocar): si cambia el diseño de la apertura,
   cambia el oráculo Python a la vez y regenera solo los PNG del morph de apertura con el
   generador de siempre, como hizo DX-1g, y explícalo.
5. Humo `chat`, `real`, `big` y `anim` en release, en los temas `light`/`dark` y las formas
   `pill`/`circle`: vivos y sin `PÁNICO`.

## Archivos
`ez-hud/src/win/chat/*`, `ez-hud/src/model/chat/*` solo lo del morph, el oráculo Python del
chat si cambia el diseño, `parity/golden/hud/chat/morph-open-*`, y
`ez-app/examples/diag_chat_smoke.rs`. Nada fuera de eso.

## Entrega
fmt, clippy y test de `ez-hud` y `ez-app` en **debug y release**; los PNG y la tira de
comparación; tabla antes/después. Informe `rust-migration/reports/DX-1i-r1.md`, commit en
`work/DX-1i`. Compila en primer plano, sin dos `cargo` a la vez. **La persona tiene la app
abierta (PID 14124) con su `whisper-server`: no los toques.**

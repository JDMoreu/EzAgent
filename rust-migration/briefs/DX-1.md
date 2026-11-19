# DX-1 — Diagnóstico: la app de voz se congela al abrir el chat

**Encargo de diagnóstico y propuesta. No cambies el código del producto.** Rama base:
`eco/main` (la app de voz `ez-app` + `ez-hud` + `ez-chat` no cambió en esta rama).

## Síntoma (lo cuenta la persona)
Al abrir el chat, la app se congela: no deja hacer clic, minimizar, silenciar ni escribir,
aunque **parece seguir escuchando**. Sospecha de la interfaz (cómo dibuja) y de que
empeora cuando la app lleva mucho tiempo abierta o con muchas cosas abiertas.

## Datos que ya hay (del agente principal, 19-nov ~10:20)
- Proceso abierto: `rust/target/release/ezagent-voice.exe` (compilado el 16-nov),
  464 MB de memoria, 50 hilos, 627 handles, **149 s de CPU en ~35 min** de uso normal.
- La app solo registra en stderr (`ez-app/src/app/main.rs` `init_logging`), no en archivo:
  no hay rastro del congelamiento. `%APPDATA%\EzAgent\logs\` tiene `latency.jsonl` y
  `audit.jsonl` (tiempos por turno y llamadas a herramientas) que sí puedes leer.
- `ez-hud/src/win/ui_loop.rs`: un único hilo con una ventana de mensajes; todo el trabajo
  de la interfaz se ejecuta ahí. El audio corre en otros hilos (eso explicaría que «siga
  escuchando»).
- Mapa: ventana del chat `ez-hud/src/win/chat/` (`window.rs`, `compose.rs`), modelo y
  dibujo `ez-hud/src/model/chat/` (`view/canvas.rs`, `draw.rs`, `entries.rs`), datos y
  markdown `ez-chat/`, cableado `ez-app/src/app/chat.rs` y `chat_wiring*`.

## Hipótesis a confirmar o descartar (con evidencia)
1. Dibujar o maquetar el chat cuesta en proporción a todo el historial (markdown, medida
   de texto, imágenes decodificadas) en cada cuadro, en el hilo de la interfaz.
2. Superficie *layered* grande actualizada entera en cada cuadro o animación.
3. El hilo de la interfaz espera un candado que otro hilo retiene durante E/S, una
   llamada al LLM o al guardar el historial.
4. La cola de `UiLoop::post` se inunda (p. ej. una tarea por token en streaming) y deja
   sin atender los mensajes de entrada de Windows.
5. Algo crece con el tiempo de uso: temporizadores que se reprograman sin cancelarse,
   historial en memoria, objetos GDI/USER que no se liberan.
Puede haber más de una causa: ordénalas por probabilidad.

## Lo que puedes hacer
- Leer todo el código. Medir con pruebas o ejemplos **propios en tu rama** (p. ej.
  `rust/crates/ez-hud/examples/diag_chat_*.rs` o pruebas `#[test]` de medición), sin
  cambiar el producto: dibujar el chat con 10, 100, 1 000 y 5 000 entradas y medir
  tiempos; contar temporizadores o tareas encoladas; buscar candados retenidos en E/S.
- Leer (sin cambiar nada) el proceso abierto de la persona: memoria, hilos, objetos GDI
  y USER (`GetGuiResources`) con PowerShell. **No** lo cierres, no le inyectes nada y no
  lo depures adjuntándote.
- Mirar `rust-migration/tools/orquestacion/stackdump.py` y `symbolize.py` (herramientas
  anteriores para volcar pilas) y proponer un procedimiento para que la persona capture
  la pila del hilo de la interfaz la próxima vez que se congele.

## Entrega
`rust-migration/reports/DX-1-r1.md`:
1. Causa o causas, cada una con evidencia (archivo:línea, mediciones con números,
   salida de comandos) y nivel de confianza.
2. Arreglo propuesto: qué cambiar, en qué archivos, cómo se evita que vuelva (incluye
   registro a archivo con rotación para tener rastro) y qué pruebas lo cubrirían.
3. Riesgos del arreglo y lo que no pudiste confirmar.
Haz commit del informe y de tus ejemplos o pruebas de medición en tu rama.

# DX-1b — Arreglar el congelamiento del chat (fase A del diagnóstico DX-1)

**Lee `AGENTS.md`, `rust-migration/reports/DX-1-r1.md` (diagnóstico validado por el agente
principal: las causas 1, 3 y el tope de 400 están confirmadas en el código) y este
brief.** Rama `work/DX-1b`, hecha desde `work/DX-1`: tienes la medición
`rust/crates/ez-app/tests/diag_chat_history_cost.rs` para medir antes y después.

## Qué se implementa (pasos del informe §4)
1. **Paso 1**: `prepare_view` (`ez-hud/src/win/chat/window/lifecycle.rs:105-114`) deja de
   maquetar el historial en el hilo que abre; el primer cuadro de contenido lo pinta el
   hilo de pintura. Abrir el chat no puede bloquear el hilo principal de la app.
2. **Paso 5**: `ChatPanel::render` (`ez-hud/src/win/chat/window.rs:328-388`) no pinta con
   `PanelState` tomado: toma, copia lo necesario, suelta, pinta y vuelve a tomar solo para
   guardar. La entrada (clic, teclado, cerrar) tiene que responder aunque un cuadro tarde.
3. **Paso 3**: caché de maquetación (`ez-hud/src/model/chat/view/entries.rs`) con tope por
   **bytes** (32 MB de teselas) y expulsión LRU real (la menos usada recientemente y no
   visible), sin clonar `msg.text` en la clave (id + longitud + hash de 64 bits).
4. **Paso 4**: el parpadeo del cursor y el tic del orbe no invalidan la capa de contenido
   (`ez-hud/src/win/chat/compose.rs`): firma de contenido separada de la del cuadro; el
   cursor y el orbe se pintan aparte.
5. **Paso 6**: `signature()` lee la versión del modelo sin copiar el historial
   (`fn version(&self) -> i64` en el trait `ChatModel` y su implementación en
   `ez-app/src/app/chat_wiring/model.rs`).
6. **Paso 7**: registro a archivo con rotación diaria en `%APPDATA%\EzAgent\logs\`
   (`ez-app/src/app/main.rs` `init_logging`, con `tracing-appender` que ya es dependencia,
   `non_blocking` y el guard vivo), conservando 7 días; y los tiempos de cuadro del chat
   (`FrameStats`, p50/p95/máx por minuto) alimentados desde el render del chat.
7. **Paso 8**: la ventana *layered* del chat presenta a la cadencia que pide el panel
   (no a 60 fps fijos) cuando no hay animación.
La virtualización (paso 2) **no** entra en esta fase.

## Archivos
Los que nombran los pasos, en `ez-hud`, `ez-app` y, si hace falta para `version()`,
`ez-chat`. Ningún otro. Si una prueba de paridad visual (`ez-hud/tests/hud_chat_parity*`,
umbrales de `parity/thresholds.toml`) cambia, **no** relajes umbrales: explica la causa.

## Pruebas (de la tabla del informe; todas con historiales ≤ 200 mensajes y < 1 GB)
- Abrir el chat no maqueta en el hilo que abre (contador de `render` en la vista).
- Un cuadro lento no bloquea la entrada (hilo de entrada concurrente mientras se pinta).
- La caché no se vacía sola con más mensajes que el tope antiguo (usa 450 mensajes
  cortos y una tesela pequeña para no gastar memoria): el segundo cuadro es mucho más
  barato que el primero.
- En reposo el contenido se repinta 0 veces por segundo (conteo de renders con el
  cursor parpadeando).
- `signature()` no llama a `snapshot()` (modelo falso que cuenta llamadas).
- El registro a archivo existe y rota (carpeta temporal).
- Medición antes/después con `diag_chat_history_cost` (10 y 100 mensajes; **nunca** más
  de 1000 y siempre con el corte de 1 GB): pega las tablas en el informe.

## Reglas
No cierres ni toques la app de voz abierta de la persona, no ejecutes agentes reales, no
toques la ventana de Herdr. Una medición a la vez y con tope de 1 GB. La medición
`diag_chat_history_cost` **no** se fusiona como prueba normal: al final conviértela en
pruebas rápidas acotadas (las de arriba) y deja la medición larga fuera de `tests/`
(p. ej. `rust/crates/ez-app/examples/diag_chat_history_cost.rs`).

## Criterios de aceptación
`cargo fmt`, `cargo clippy --all-targets -- -D warnings` y `cargo test` de `ez-hud`,
`ez-app` (y `ez-chat` si lo tocaste) en verde; si una prueba falla solo bajo carga y pasa
aislada, demuéstralo. Informe en `rust-migration/reports/DX-1b-r1.md` con las tablas de
antes y después.

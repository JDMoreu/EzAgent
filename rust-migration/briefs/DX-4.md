# DX-4 — Al abrir la app, nada pendiente se reanuda solo: se pregunta

**Lee `AGENTS.md`, `rust-migration/briefs/_COMMON-ECO.md`, `rust-migration/reports/DX-2-r1.md`
(acuses cortos) y `NV-3-r1.md`/`NV-3b-r1.md` (la app como padre).** Rama `work/DX-4` desde
`eco/main`.

## Pedido de la persona (20-nov)
«Cuando abra la conversación de EzAgent, no reanudes automáticamente la tarea pendiente.
Pregúntame si quiero reanudarla. Esto pasa cuando cierro la aplicación y el proceso no ha
terminado: a veces continúa solo y yo quería hacer otra cosa.»

## Lo que hace hoy (visto por el agente principal; confírmalo y busca más)
- `app/startup.rs` llama a `resume_pending_tasks` (`app/watch.rs:422`): vuelve a vigilar
  **todas** las tareas y **cadenas** (`then`) de `tasks.json` y hace `check_task_now`; si la
  fuente de una cadena ya terminó, el siguiente paso puede lanzarse sin preguntar.
- `app/startup.rs` arranca el **enjambre** guardado (`swarm.start()`), que vuelve a abrir y
  despertar agentes.
- El modo autónomo ya vuelve en pausa (`autonomous/host/restore.rs`): úsalo de modelo.
- Revisa también: el reintento automático de NV-2/NV-3 (`planes.toml`), la confirmación
  pendiente que se rearma (F48 en `ez-agent/src/runtime.rs`) y si el historial del chat
  hace que el LLM retome solo una petición vieja.

## Qué se hace
1. **Inventario**: lista en el informe cada camino por el que algo pendiente puede actuar
   (lanzar, enviar texto a un agente, despertar el enjambre, reintentar) tras reiniciar la
   app, con archivo y línea.
2. **Nada actúa solo al arrancar**: tareas, cadenas, enjambre, reintentos y confirmaciones
   pendientes quedan **en espera de la persona**. Seguir vigilando de forma pasiva lo que
   ya está corriendo (para avisar si termina) está permitido; **lanzar, enviar o despertar
   no**, hasta que la persona diga que sí.
3. **Se pregunta una vez**, al abrir, por voz y en el chat, con una frase escrita por el
   código (español neutro, breve; los acuses cortos siguen DX-2): p. ej. «Quedó pendiente
   <qué> (<agente>). ¿Quieres que lo reanude?». Si hay varias cosas, una sola pregunta que
   las nombre. «Sí / dale / reanuda» → reanuda; «no / déjalo / después» → no hace nada y no
   vuelve a preguntar en esta sesión (sigue guardado; «reanuda lo pendiente» lo retoma);
   «descártalo» → lo borra. Sin respuesta → no hace nada.
4. **Pruebas** con los dobles de la app (sin tocar `tasks.json` real, `EZAGENT_LOGS_DIR`
   temporal): cadena pendiente + reinicio → no se lanza el paso siguiente hasta el «sí»;
   enjambre guardado → no se arranca hasta el «sí»; «no» → nada se lanza y no se repite la
   pregunta; «descártalo» → se borra; sin pendientes → no se pregunta nada.

## Archivos
`ez-app/src/app/*` (arranque, vigilancia, enjambre, atajos de respuesta) y, si hace falta,
`ez-agent` solo para la confirmación pendiente; sus pruebas. Nada de `ez-hud` (DX-1i lo
tiene abierto).

## Reglas
**Prohibido abrir ventanas**: nada de `diag_chat_smoke` ni de lanzar la app hasta que el
agente principal lo autorice (la persona está en una entrevista). Solo pruebas sin
ventanas. No toques la app ni el `whisper-server` de la persona. Compila en primer plano,
sin dos `cargo` a la vez.

## Entrega
fmt, clippy y test de `ez-app` (y `ez-agent` si lo tocas) en verde. Informe
`rust-migration/reports/DX-4-r1.md` con el inventario, commit en `work/DX-4`.

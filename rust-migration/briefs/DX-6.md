# DX-6 — El estado de las tareas queda a la vista: notificación persistente, no solo voz

**Lee `AGENTS.md`, `rust-migration/briefs/_COMMON-ECO.md`, y los informes de NV-3/NV-3b
(la app como padre) y DX-2 (acuses).** Rama `work/DX-6` desde `eco/main`.

## Pedido de la persona (20-nov)
«Cuando me avises de que un agente terminó, el aviso debe quedar como una notificación de
tarea pendiente, para que yo tenga claro que hay algo en curso y pueda ver el estado de la
tarea sin que me lo tengas que decir explícitamente. Quiero un feedback sencillo y
persistente del estado de la tarea, no solo un aviso hablado.»

## Lo que ya existe (no lo dupliques; constrúyelo encima)
- `ez-hud/src/paint/marks.rs`: la **órbita de actividad**, una marca por tarea (hasta 5):
  `working` respira en menta, `needs_you` ámbar, `failed` rojo, `waiting`/`stale` neutros,
  `done` (sin ver) punto menta.
- `ez-hud/src/win/drop/ui/lifecycle.rs`: `set_activity` (foto para la órbita y la vista
  rápida), `show_activity`, `announce_activity` (un *toast* breve), y en
  `win/drop/ui/render.rs` `on_activity_open`/`on_activity_dismiss`.
- `ez-hud/src/win/peek/*`: la **vista rápida**.
- `ez-app/src/app/activity.rs` (`activity_tick`) alimenta esa foto; `app/watch.rs` avisa
  «Oye, X terminó…»; NV-3 (`app/subagents.rs`) anuncia los subagentes de `voz`.

## Qué se hace
1. **Diagnóstico**: qué ve hoy la persona cuando un agente termina (voz, *toast*, marca,
   vista rápida), cuánto dura cada cosa, y por qué no le queda claro que hay algo
   pendiente: si la marca `done` se borra sola o al anunciar, si los subagentes de NV-3
   (`bloqueado`, `fallido`, `terminado`, `validado`) entran en la foto, si el *toast*
   desaparece sin dejar rastro. Con archivo y línea.
2. **Persistente y sencillo**: toda tarea que la app vigila o lanzó queda **visible** en la
   barra mientras importe: en curso, terminó sin revisar, falló, bloqueada o necesita algo.
   «Terminó» queda como **pendiente de revisar** (no desaparece) hasta que la persona lo
   abre, lo descarta, o pregunta por él («¿qué hizo X?»). Un indicador simple en la barra
   que diga cuántas hay pendientes (sin texto largo; «nunca solo color», como las marcas).
3. **Ver el estado sin preguntar**: un clic en la órbita/indicador abre la vista rápida con
   la lista: nombre del agente, qué se le pidió (corto), estado y hace cuánto. Desde ahí se
   abre la terminal del agente o se descarta el aviso (lo que ya hacen `on_activity_open` y
   `on_activity_dismiss`). La voz sigue avisando como hoy, una vez.
4. **Coherencia** con DX-4 (lo pendiente al reabrir la app también se ve aquí, en espera) y
   con NV-3 (los estados del registro de `voz`).
5. **Pruebas**: del modelo (la marca/indicador persiste tras el anuncio y hasta abrir o
   descartar; los estados de NV-3 aparecen; el contador es correcto) y de la app con sus
   dobles (un agente que termina → aviso hablado + elemento pendiente que persiste; abrir o
   descartar lo quita). La paridad visual del HUD que exista debe seguir en verde; si cambia
   el diseño de la barra o de la vista rápida, cambia el oráculo y regenera solo lo afectado,
   explicándolo.
6. **Humo de la app real** (ventanas): **no lo corras** hasta que el agente principal lo
   autorice; deja listo un modo en un ejemplo (p. ej. `diag_activity_smoke`) que simule una
   tarea que termina y compruebe que el indicador persiste y la vista rápida la lista.

## Archivos
`ez-hud/src/paint/marks.rs`, `ez-hud/src/win/drop/*` y `ez-hud/src/win/peek/*` (no
`win/chat`, que lo tiene DX-1i), `ez-app/src/app/activity.rs`, `app/watch.rs` (solo el aviso
de terminado; DX-4 toca el arranque en ese archivo: coordina mínimo), `app/subagents.rs`, el
ejemplo nuevo y sus pruebas.

## Reglas
**Prohibido abrir ventanas** hasta que el agente principal lo autorice (la persona está en
una entrevista). No toques la app ni el `whisper-server` de la persona, ni la ventana de
Herdr. Sin agentes reales. Compila en primer plano, sin dos `cargo` a la vez.

## Entrega
fmt, clippy y test de `ez-hud` y `ez-app` en verde. Informe `rust-migration/reports/DX-6-r1.md`
con el diagnóstico, commit en `work/DX-6`.

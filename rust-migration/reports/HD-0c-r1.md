## Resultado
- Brief/tarea: HD-0c (correcciones C0-4, C0-5 y C0-3 de la revisión de HD-0b, más el humo real sin encargo)
- Estado: completado

## Cambios
- `rust-migration/tools/orquestacion/supervise3.sh`: resuelve el ejecutable real de OpenCode antes de lanzar (C0-4): `EZ_OPENCODE_BIN`, o `where.exe opencode.cmd` → `node_modules/@opencode/cli/bin/opencode.exe` de esa carpeta, o `opencode.cmd`; lo comprueba con `--version` y, si falla, avisa y usa el modo `run`. El id de sesión pasa a `ses_<id en minúsculas y solo a-z0-9>s1` (C0-5), también en `session_id`. Un `hc get` fallido o vacío se trata como «desconocido» y se reintenta hasta 2 min sin dar el intento por terminado (C0-3); antes de lanzar, `free_name` retira cualquier terminal con ese nombre (matando la que siga viva), para no repetir `name_taken`.
- `rust-migration/tools/orquestacion/pruebas/probar.sh`: actualiza las aserciones de sesión a `ses_…s1`; añade el caso 7 (get fallido simulado, no cuenta como terminado), el caso 8 (nombre ya vivo, no se lanza duplicado), el caso opt-in de humo real (`EZ_HUMO_REAL=1`) y un atajo `EZ_SOLO_HUMO=1` para repetir solo el humo; `field` tolera respuestas fallidas de hc.
- `rust-migration/tools/orquestacion/pruebas/fake-hc.sh` (nuevo): hc falso que hace fallar las primeras N llamadas `get` y delega el resto al hc real, para simular el reinicio del núcleo de C0-3.
- `rust-migration/reports/HD-0c-r1.md` (nuevo): este informe.
- `cleanup_worker.ps1`: sin cambios (no hizo falta).

## Decisiones
- `// DECISION` en el código: (1) el id de sesión es estable por trabajador y ahora empieza por `ses_` para cumplir lo que exige OpenCode; (2) `EZ_RESOLVE_OPENCODE=1` solo imprime la resolución y sale, para verificar C0-4 sin lanzar un agente real; (3) el modo `run` ya no depende de que `BIN` sea literalmente `opencode`, sino de que no haya binario de prueba, porque en producción `BIN` es ahora la ruta nativa.
- C0-3: tras 2 min de estado desconocido se sigue con el siguiente intento (no se declara «terminado»); `free_name` mata y retira la terminal previa con el mismo nombre antes de lanzar, de modo que nunca se lanza mientras una terminal con ese nombre siga viva. Se conserva el avisado al padre y el registro §13 en todos los casos.
- El humo real usa la misma envoltura (`env.exe` + `OPENCODE_CONFIG_CONTENT` + `--standalone --auto --session ses_…`) pero sin `--prompt`; para no tocar la configuración del usuario se fijan `OPENCODE_CONFIG_DIR` y `XDG_STATE_HOME` en carpetas temporales. El `status` de hc llega como `{'app': 'opencode', 'state': 'idle'}`, así que la comprobación busca `idle` dentro del objeto.
- El contrato (`docs/designs/subagentes-herdr.md`) manda sobre los briefs; aquí no hay discrepancia: HD-0c es la herramienta bash de orquestación, no el Rust de `ez-herdr`.
- No se lanzaron agentes reales con encargo: el único OpenCode real es el humo sin `--prompt`, permitido expresamente por el brief.

## Verificación
- `bash -o igncr -n rust-migration/tools/orquestacion/supervise3.sh` → `supervise3.sh: bash -n OK`.
- `bash -o igncr -n rust-migration/tools/orquestacion/pruebas/probar.sh` → `probar.sh: bash -n OK`.
- `bash -o igncr -n rust-migration/tools/orquestacion/pruebas/fake-parent.sh` → sin errores; `bash -o igncr -n rust-migration/tools/orquestacion/pruebas/fake-hc.sh` → sin errores.
- Resolución real (sin agente): `EZ_RESOLVE_OPENCODE=1 bash supervise3.sh HD0CHUMO …` → `BIN=C:/Users/JDMoreu/AppData/Local/pi-node/current/node_modules/@opencode/cli/bin/opencode.exe`, `MODE=interactive`, `SID=ses_hd0chumos1`.
- Ejecución real, sin cuota y con el humo real una sola vez, dentro de una terminal de hc (`hc run --bg --json --name hd0c-final -- env.exe EZ_HUMO_REAL=1 bash -o igncr rust-migration/tools/orquestacion/pruebas/probar.sh hd0c-final`; sin `EZ_SHOW`). Estado final `finished`, `exit_code=0`. Salida de `hc read t14b --lines 800` (pegada literal):

```
[pruebas] terminal padre falso: t14c
[pruebas] caso 1: opencode interactivo falso, argv/env, informe, registro y aviso
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T1] intento 1 en t14d (padre t14c, agente opencode)
[HD0T1] informe completado en el intento 1
[HD0T1] código de supervise3.sh: 0
OK padre: t14c
OK nombre: hd0t1
OK ez.agent: opencode
OK ez.model: opencode/hy4-flash-free
OK cambios sin confirmar: 0
OK argv env.exe opencode: 1
OK id de sesión ses_: --session ses_hd0t1s1
OK OPENCODE_CONFIG_CONTENT: 1
OK registro estado: terminado
OK registro historial: en_proceso,terminado
OK registro by: supervisor
OK registro notified: terminado
OK registro summary: Estado: completado
OK registro report: C:/Users/JDMoreu/AppData/Local/Temp/hd0-probar-6866/opencode/rust-migration/reports/HD0T1-r1.md
OK aviso terminado opencode
OK aviso resumen del subagente
OK aviso informe en aviso
OK aviso ruta del informe en aviso
OK aviso validación en aviso
[pruebas] línea «Informe» recibida: [EzAgent · aviso] hd0t1 (OpenCode · opencode/hy4-flash-free) terminó. Resumen del subagente: «Estado: completado». Informe: /tmp/hd0-probar-6866/opencode/rust-migration/reports/HD0T1-r1.md. Para validarlo: ez validate hd0t1 --pass|--fail --file <evidencia>
[pruebas] caso 2: mcode falso en modo tui, informe, commit y cierre
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T2] intento 1 en t14f (padre t14c, agente mcode)
[HD0T2] informe completado en el intento 1
[HD0T2] código de supervise3.sh: 0
OK padre: t14c
OK nombre: hd0t2
OK ez.agent: mcode
OK ez.model: minimax/MiniMax-M3.1-Flash
OK estado cerrado: finished
OK cambios sin confirmar: 0
OK aviso terminado mcode
[pruebas] caso 3: sin informe (EZ_WORKER_CMD) reintenta, falla y avisa
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T3] intento 1 en t14g (padre t14c, agente opencode)
[HD0T3] intento 1 terminó sin informe; reintento
[HD0T3] intento 2 en t14j (padre t14c, agente opencode)
[HD0T3] intento 2 terminó sin informe; reintento
[HD0T3] sin informe completado tras 2 intentos
[HD0T3] código de supervise3.sh: 2
OK registro fallido estado: fallido
OK registro fallido detalle: sin informe completado tras 2 intentos
OK registro fallido notified: fallido
OK aviso fallido
[pruebas] caso 4: opencode interactivo reintenta con la misma sesión
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T4] intento 1 en t14k (padre t14c, agente opencode)
[HD0T4] intento 1 terminó sin informe; reintento
[HD0T4] intento 2 en t14m (padre t14c, agente opencode)
[HD0T4] informe completado en el intento 2
[HD0T4] código de supervise3.sh: 0
OK reintento misma sesión: 2
OK reintento conserva entorno: 2
[pruebas] caso 5: opencode interactivo con TUI se cierra tras informe y commit
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T5] intento 1 en t14n (padre t14c, agente opencode)
[HD0T5] informe completado en el intento 1
[HD0T5] código de supervise3.sh: 0
OK opencode tui cerrado: finished
OK opencode tui sin cambios: 0
[pruebas] caso 6: EZ_OPENCODE_MODE=run conserva el modo anterior
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T6] intento 1 en t14p (padre t14c, agente opencode)
[HD0T6] informe completado en el intento 1
[HD0T6] código de supervise3.sh: 0
OK modo run sin envoltura: 1
OK modo run sin OPENCODE_CONFIG_CONTENT: 1
[pruebas] caso 7: C0-3 un hc get fallido/vacío no cuenta como terminado
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T7] intento 1 en t14q (padre t14c, agente opencode)
[HD0T7] aviso: estado desconocido de t14q; reintento leyendo hasta 2 min
[HD0T7] informe completado en el intento 1
[HD0T7] código de supervise3.sh: 0
[pruebas] caso 8: C0-3 no lanza si el nombre ya está vivo
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T8] intento 1 en t14s (padre t14c, agente opencode)
[HD0T8] informe completado en el intento 1
[HD0T8] código de supervise3.sh: 0
OK caso 8 una sola terminal con el nombre: 1
[pruebas] humo real: OpenCode sin encargo (EZ_HUMO_REAL=1)
BIN=C:/Users/JDMoreu/AppData/Local/pi-node/current/node_modules/@opencode/cli/bin/opencode.exe
MODE=interactive
SID=ses_hd0chumos1
OK resolución exe nativo: opencode.exe
OK resolución modo interactivo: interactive
OK sesión del humo: ses_hd0chumos1
[humo] terminal=t14t bin=C:/Users/JDMoreu/AppData/Local/pi-node/current/node_modules/@opencode/cli/bin/opencode.exe sesión=ses_hd0chumos1
[humo] tras 3s: status={'app': 'opencode', 'state': 'idle'}
OK humo idle: {'app': 'opencode', 'state': 'idle'}
OK humo sin error: live
[humo] cerrada t14t
[pruebas] resultado: VERDE
```
- Sin limpieza pendiente: no quedan terminales `hd0t*`, ni terminales con la etiqueta `ez.brief=HD0CHUMO`, ni carpetas `hd0-probar-*`; las terminales de los guiones de prueba (`hd0c-*`) se retiraron con `hc rm`. No se tocó la ventana de Herdr de la persona (sin `EZ_SHOW`).
- No aplica `cargo fmt`/`clippy`/`test`: este brief no toca crates Rust. La puerta equivalente (`bash -n` + `probar.sh` + humo real) está en verde arriba.

## Pendiente o riesgos
- No se lanzó OpenCode con un encargo real (gastaría cuota y `_COMMON-HD.md` lo prohíbe): el humo solo abre la interfaz sin `--prompt`, comprueba `idle` y cierra; no escribe nada ni envía mensajes.
- El fallback `opencode.cmd` y el paso a modo `run` cuando `--version` falla no se ejercitaron con el programa real: en esta máquina el exe nativo responde `opencode v2.3.0`. La lógica queda cubierta por la resolución impresa y por los casos del supervisor con binario falso.
- Durante las ejecuciones intermedias el núcleo de hc se reinició (el escenario real de C0-3): la suite continuó y la ejecución final quedó en verde; `field` en las pruebas ahora tolera esas respuestas fallidas.
- `EZ_RESOLVE_OPENCODE=1` y `EZ_SOLO_HUMO=1` son palancas de diagnóstico para las pruebas; no cambian el lanzamiento normal.

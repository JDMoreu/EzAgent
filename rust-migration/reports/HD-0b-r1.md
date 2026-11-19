## Resultado
- Brief/tarea: HD-0b (correcciones de la revisión de HD-0: C0-1 OpenCode interactivo, C0-2 estado §13 y avisos al padre)
- Estado: completado

## Cambios
- `rust-migration/tools/orquestacion/supervise3.sh`: OpenCode corre en modo interactivo por omisión (`EZ_OPENCODE_MODE=interactive`) con la envoltura sin shell `env.exe OPENCODE_CONFIG_CONTENT={"model":"<modelo>"} <bin> --standalone --auto --session <id> --prompt "<texto>"`; id de sesión estable `<ID>-s1` para que los reintentos la continúen con `--prompt "<continuar>"`. Sin `--model` (va en la variable de entorno). Fin como mcode: informe + worktree confirmado (máx. 15 min) → guardar pantalla y matar la terminal. `EZ_OPENCODE_MODE=run` conserva el modo anterior. Añadido el registro §13: `en_proceso` al lanzar, `terminado` con `summary`/`report`, `fallido` al agotar intentos con `detail`; escritura atómica con Python y `hc send` al padre en cada cambio, con `notified`. Sin `HC_TERMINAL_ID` no hay registro ni avisos y se dice al empezar.
- `rust-migration/tools/orquestacion/pruebas/probar.sh`: ampliado con padre falso propio (nunca la terminal de la persona ni la del agente principal), `EZ_SUBAGENTS_DIR` temporal, comprobación del argv/env de la envoltura, del reintento con la misma sesión, del cierre del TUI de OpenCode, del registro `en_proceso → terminado` y `→ fallido`, de los avisos con formato §13 y del modo heredado `run`.
- `rust-migration/tools/orquestacion/pruebas/fake-worker.cmd`: registra `argv`/entorno en `.fake-argv.log`, añade modo `retry` (primer intento sin informe, segundo con informe) y confirma todos los cambios con `git add -A`.
- `rust-migration/tools/orquestacion/pruebas/fake-parent.sh` (nuevo): guarda en un archivo cada línea que recibe por `hc send`.
- `rust-migration/reports/HD-0b-r1.md` (nuevo): este informe.
- `cleanup_worker.ps1`: sin cambios (no hizo falta).

## Decisiones
- `// DECISION` en el código: (1) `EZ_MODEL` explícito manda; si no, el modelo por omisión del tipo. (2) id estable `<ID>-s1` para la sesión de OpenCode. (3) un solo escritor en este flujo: `temp+rename` atómico basta; el candado §13 pertenece al CLI `ez` con varios escritores concurrentes.
- El brief pedía comprobar el formato de `--session` abriendo `opencode --standalone --session <id>` sin `--prompt`. Lo comprobé con `opencode --help` (sin lanzar un agente real ni gastar cuota, como exige `_COMMON-HD.md`): dice literalmente «Session ID to continue, or to create if it does not exist». Acepta ids propios, así que no hace falta buscar la sesión por título. No hay discrepancia con el diseño.
- §13 no define aviso para `en_proceso`, pero el brief pide avisar en cada cambio: se usa `[EzAgent · aviso] <nombre> (<tipo> · <modelo>) en proceso.` Los de `terminado` y `fallido` siguen el formato de §13, incluido «Para validarlo: ez validate …».
- El campo `label` del registro lleva el nombre legible del tipo (`OpenCode`, `mcode`); `effort` queda vacío porque el supervisor no fija esfuerzo. `summary` se limpia de la viñeta `- ` y se corta a 300 caracteres en una línea.
- `hc send` se intenta antes de escribir `notified`: si falla, se imprime el aviso en la salida y `notified` conserva su valor anterior. Sin `HC_TERMINAL_ID`, `record` no hace nada.
- En las pruebas MSYS convierte las rutas `/tmp/...` de forma distinta al pasar por argv (`hc send`) que por variables de entorno (`python`); por eso el informe del aviso aparece en forma POSIX y el del registro en forma Windows. Es un artefacto del entorno de prueba; ambos apuntan al mismo archivo.
- El contrato (`docs/designs/subagentes-herdr.md`) manda sobre el brief; aquí no hay discrepancia: HD-0b es la herramienta bash de orquestación, no el Rust de `ez-herdr`.

## Verificación
- `bash -n rust-migration/tools/orquestacion/supervise3.sh` → sin errores (`supervise3.sh: bash -n OK`).
- `bash -n rust-migration/tools/orquestacion/pruebas/probar.sh` → sin errores (`probar.sh: bash -n OK`).
- `bash -n rust-migration/tools/orquestacion/pruebas/fake-parent.sh` → sin errores.
- Ejecución real, sin cuota y sin agentes reales, dentro de una terminal de hc (`hc run --bg --json --name hd0b-pruebas -- "C:/Program Files/Git/bin/bash.exe" -o igncr rust-migration/tools/orquestacion/pruebas/probar.sh hd0b-pruebas`; sin `EZ_SHOW`). Estado final: `finished`; no quedaron terminales `hd0b*`/`hd0t*` ni carpetas `hd0-probar-*`. Salida de `hc read t126 --lines 700` (pegada literal):

```
[pruebas] terminal padre falso: t127
[pruebas] caso 1: opencode interactivo falso, argv/env, informe, registro y aviso
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T1] intento 1 en t128 (padre t127, agente opencode)
[HD0T1] informe completado en el intento 1
[HD0T1] código de supervise3.sh: 0
OK padre: t127
OK nombre: hd0t1
OK ez.agent: opencode
OK ez.model: opencode/hy4-flash-free
OK cambios sin confirmar: 0
OK argv env.exe opencode: 1
OK OPENCODE_CONFIG_CONTENT: 1
OK registro estado: terminado
OK registro historial: en_proceso,terminado
OK registro by: supervisor
OK registro notified: terminado
OK registro summary: Estado: completado
OK registro report: C:/Users/JDMoreu/AppData/Local/Temp/hd0-probar-11576/opencode/rust-migration/reports/HD0T1-r1.md
OK aviso terminado opencode
OK aviso resumen del subagente
OK aviso informe en aviso
OK aviso ruta del informe en aviso
OK aviso validación en aviso
[pruebas] línea «Informe» recibida: [EzAgent · aviso] hd0t1 (OpenCode · opencode/hy4-flash-free) terminó. Resumen del subagente: «Estado: completado». Informe: /tmp/hd0-probar-11576/opencode/rust-migration/reports/HD0T1-r1.md. Para validarlo: ez validate hd0t1 --pass|--fail --file <evidencia>
[pruebas] caso 2: mcode falso en modo tui, informe, commit y cierre
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T2] intento 1 en t129 (padre t127, agente mcode)
[HD0T2] informe completado en el intento 1
[HD0T2] código de supervise3.sh: 0
OK padre: t127
OK nombre: hd0t2
OK ez.agent: mcode
OK ez.model: minimax/MiniMax-M3.1-Flash
OK estado cerrado: finished
OK cambios sin confirmar: 0
OK aviso terminado mcode
[pruebas] caso 3: sin informe (EZ_WORKER_CMD) reintenta, falla y avisa
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T3] intento 1 en t12b (padre t127, agente opencode)
[HD0T3] intento 1 terminó sin informe; reintento
[HD0T3] intento 2 en t12c (padre t127, agente opencode)
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
[HD0T4] intento 1 en t12d (padre t127, agente opencode)
[HD0T4] intento 1 terminó sin informe; reintento
[HD0T4] intento 2 en t12f (padre t127, agente opencode)
[HD0T4] informe completado en el intento 2
[HD0T4] código de supervise3.sh: 0
OK reintento misma sesión: 2
OK reintento conserva entorno: 2
[pruebas] caso 5: opencode interactivo con TUI se cierra tras informe y commit
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T5] intento 1 en t12g (padre t127, agente opencode)
[HD0T5] informe completado en el intento 1
[HD0T5] código de supervise3.sh: 0
OK opencode tui cerrado: finished
OK opencode tui sin cambios: 0
[pruebas] caso 6: EZ_OPENCODE_MODE=run conserva el modo anterior
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T6] intento 1 en t12h (padre t127, agente opencode)
[HD0T6] informe completado en el intento 1
[HD0T6] código de supervise3.sh: 0
OK modo run sin envoltura: 1
OK modo run sin OPENCODE_CONFIG_CONTENT: 1
[pruebas] resultado: VERDE
```
- No aplica `cargo fmt`/`clippy`/`test`: este brief no toca crates Rust (es la herramienta bash de orquestación). La puerta equivalente (`bash -n` + `probar.sh`) está en verde arriba.

## Pendiente o riesgos
- No se lanzó `opencode` real (gastaría cuota y `_COMMON-HD.md` lo prohíbe): el falso cubre la forma del argv, la variable `OPENCODE_CONFIG_CONTENT`, el id de sesión, el reintento, el commit y el cierre del TUI.
- Durante una ejecución intermedia el núcleo de hc se reinició («no holder: the core is stopping or restarting») y falló un intento de forma transitoria; se repitió la suite completa con hc estable y quedó en verde (salida pegada). El supervisor ya reintenta un lanzamiento fallido de hc, pero con `EZ_MAX=2` un fallo transitorio consume los dos intentos.
- `EZ_OPENCODE_MODE=run` se conserva y se prueba con el falso; no se ejerció con `opencode` real.
- No se envió ningún aviso a la terminal de la persona ni a la del agente principal: todos los avisos de las pruebas fueron al padre falso (`t127`).

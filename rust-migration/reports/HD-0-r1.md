## Resultado
- Brief/tarea: HD-0 (paso 1: el supervisor lanza a cada trabajador como subagente en Herdr)
- Estado: completado

## Cambios
- `rust-migration/tools/orquestacion/supervise3.sh`: `EZ_AGENT=opencode|mcode` (por omisión `opencode`) elige el tipo; `opencode` mantiene `run --model/--title/--auto` y reintentos con `--session`, `mcode` usa `-m <modelo> "<prompt>"` y reintentos con `-c "<continuación>"`. Modelo por omisión según el tipo (`opencode/hy4-flash-free`, `minimax/MiniMax-M3.1-Flash`). Etiquetas `ez.agent` y `ez.model` dinámicas. `EZ_AGENT_BIN` sustituye al binario (solo pruebas). Con informe completado se espera a que `git -C <worktree> status --porcelain` quede vacío (máx. 15 min), se guarda la pantalla y se cierra la terminal (los TUI de mcode no salen solos). `EZ_WORKER_CMD` ya no termina como «hecho» sin informe: cuenta como intento fallido. `EZ_MAX` hace configurable el número de intentos. Se detecta un intérprete Python usable (`python3` o `python`) porque el entorno de hc y el de Git Bash no coinciden. En pruebas no se bajan prioridades de procesos reales.
- `rust-migration/tools/orquestacion/pruebas/fake-worker.cmd` (nuevo): agente falso que ignora los argv del agente real, lee `.fake-id`/`.fake-mode` de la carpeta de trabajo, espera unos segundos, escribe un informe con «Estado: completado», hace commit y, en modo `tui`, no sale; en modo `fail` no escribe informe.
- `rust-migration/tools/orquestacion/pruebas/probar.sh` (nuevo): guion de pruebas sobre carpetas git temporales bajo `%TEMP%`. Cubre opencode con binario falso (código 0, `parent`, nombre y etiquetas), mcode en modo tui (detecta informe, espera el commit, cierra la terminal, código 0) y `EZ_WORKER_CMD` sin informe (reintenta y sale 2 tras `EZ_MAX=2`). Limpia sus terminales con `hc kill`/`hc rm` y borra sus carpetas. No usa `EZ_SHOW`.
- `rust-migration/reports/HD-0-r1.md` (nuevo): este informe.
- `cleanup_worker.ps1`: sin cambios (no hizo falta).

## Decisiones
- `// DECISION` en el código: un `EZ_MODEL` explícito manda; si no, se usa el modelo por omisión del tipo. Así quien delega fija modelo y tipo juntos sin que un valor heredado del otro tipo se cuele.
- Tipo de agente desconocido: se avisa en español neutro y se cae a `opencode` (opción más simple y segura; no se inventa un tipo nuevo).
- Las etiquetas que exige el brief son `ez.agent` y `ez.model`; la etiqueta `agent=<tipo>` que menciona `docs/designs/subagentes-herdr.md` §3 es la que ya pone `ez-herdr::agent_launch`, no existía en este script bash y no se añadió para no salir del alcance.
- El contrato manda sobre los briefs; aquí no hay discrepancia: este HD-0 es la herramienta bash de orquestación, no el Rust de `ez-herdr`.
- Adaptaciones del entorno de prueba (no cambian el comportamiento de producción): este bash (Git Bash 5.3.9) no tiene la opción `igncr`, así que `probar.sh` copia el supervisor con `tr -d '\r'` antes de ejecutarlo y el informe se verifica con `bash -n` sobre la copia en LF; git almacena estos `.sh` en LF de todos modos. `hc` no inyecta `HC_TERMINAL_ID` en un comando directo, así que `probar.sh` lo deriva de su propio nombre con `hc get` (primer argumento). La terminal por omisión de hc es WSL bash sin `hc`/`python` en PATH, por eso `probar.sh` se lanzó con `C:/Program Files/Git/bin/bash.exe`, donde sí están `hc`, `python` y los coreutils. No se tocó la configuración de git ni la ventana de Herdr de la persona.

## Verificación
- `bash -n rust-migration/tools/orquestacion/supervise3.sh` → sin errores (`supervise3.sh: bash -n OK`).
- `bash -n rust-migration/tools/orquestacion/pruebas/probar.sh` → sin errores (`probar.sh: bash -n OK`).
- Ejecución real, sin cuota y sin agentes reales, dentro de una terminal de hc (`hc run --name hd0-pruebas -- "C:/Program Files/Git/bin/bash.exe" rust-migration/tools/orquestacion/pruebas/probar.sh hd0-pruebas`; sin `EZ_SHOW`). Estado final de la terminal: `state finished exit 0`; no quedaron terminales `hd0t*` ni carpetas `hd0-probar-*`.
- Salida de `hc read t10g --lines 400` (pegada literal):

```
[pruebas] terminal padre: t10g
[pruebas] caso 1: opencode con binario falso, informe y commit
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T1] intento 1 en t10h (padre t10g, agente opencode)
[HD0T1] informe completado en el intento 1
[HD0T1] código de supervise3.sh: 0
OK padre: t10g
OK nombre: hd0t1
OK ez.agent: opencode
OK ez.model: opencode/hy4-flash-free
OK cambios sin confirmar: 0
[pruebas] caso 2: mcode falso en modo tui, informe, commit y cierre
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T2] intento 1 en t10j (padre t10g, agente mcode)
[HD0T2] informe completado en el intento 1
[HD0T2] código de supervise3.sh: 0
OK padre: t10g
OK nombre: hd0t2
OK ez.agent: mcode
OK ez.model: minimax/MiniMax-M3.1-Flash
OK estado cerrado: finished
OK cambios sin confirmar: 0
[pruebas] caso 3: sin informe (EZ_WORKER_CMD) reintenta y falla
warning: in the working copy of '.fake-id', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of '.fake-mode', LF will be replaced by CRLF the next time Git touches it
[HD0T3] intento 1 en t10k (padre t10g, agente opencode)
[HD0T3] intento 1 terminó sin informe; reintento
[HD0T3] intento 2 en t10m (padre t10g, agente opencode)
[HD0T3] intento 2 terminó sin informe; reintento
[HD0T3] sin informe completado tras 2 intentos
[HD0T3] código de supervise3.sh: 2
[pruebas] resultado: VERDE
```
- No aplica `cargo fmt`/`clippy`/`test`: este brief no toca crates Rust (es la herramienta bash de orquestación). La puerta equivalente (`bash -n` + `probar.sh`) está en verde arriba.

## Pendiente o riesgos
- No se probó el lanzamiento con `opencode`/`mcode` reales (gastaría cuota y el brief lo prohíbe); el falso cubre la forma de los argv, las etiquetas, el padre, el commit y el cierre del TUI.
- `hc wait` devolvió un aviso de «core restarted» durante la ejecución, pero la terminal siguió y terminó con `exit 0` con la salida completa; no afectó al resultado.
- `EZ_SHOW` quedó sin cambios y no se ejerció en las pruebas, para no tocar la ventana de Herdr de la persona.
- Los avisos de git sobre LF/CRLF en `.fake-id`/`.fake-mode` son de las carpetas temporales de la prueba y no afectan al resultado.

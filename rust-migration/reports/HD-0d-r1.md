## Resultado
- Brief/tarea: HD-0d — corrección urgente del supervisor (mataba trabajadores sanos por un `\r`)
- Estado: completado

## Cambios
- `rust-migration/tools/orquestacion/supervise3.sh`: v4. Un único ayudante `strip_cr` (`tr -d '\r'`) para todo lo que el script compara o usa como dato (`field`, `terminal_state`, `launch`, `named_terminals`/`free_name`, `session_id`, `report_summary`, `hc read` del log, de la detección de colgado y del aviso de límite, `where.exe`, el prompt y la comprobación del informe). Una terminal viva ya no se mata nunca por un estado «desconocido»: a los 2 min se sigue esperando y se avisa cada 5 min (`EZ_UNKNOWN_GRACE`, `EZ_UNKNOWN_EVERY`); `free_name` solo retira terminales **terminadas** y, si hay una terminal **viva** con el mismo nombre, el supervisor la sigue en vez de lanzar otro intento. C0-6: detecta en pantalla los avisos de límite de uso («free limit reached», «usage limit», «rate limit», «quota»), registra `fallido` con ese motivo, avisa al padre y no reintenta; si `EZ_MODEL` cambia respecto de la sesión guardada, abre una sesión nueva (`ses_…s2`) en vez de continuar la vieja. `EZ_PYTHON` permite apuntar a otro intérprete (las pruebas).
- `rust-migration/tools/orquestacion/pruebas/probar.sh`: la suite ya no exige estar dentro de hc (fuera de hc crea el padre falso con `--no-parent` y no toca ninguna terminal de la persona); casos nuevos: 9 (python que devuelve `\r\n`: el trabajador vivo termina su informe, el supervisor no lo mata y no ve «desconocido»), 10 (aviso de límite de uso: `fallido`, aviso al padre, sin reintento) y 11 (cambio de `EZ_MODEL` → sesión nueva); el caso 8 pasó a comprobar que una terminal viva con el mismo nombre se sigue en vez de matarse.
- `rust-migration/tools/orquestacion/pruebas/fake-worker.cmd`: modos nuevos `slow` (tarda 25 s antes de escribir el informe, para que un «desconocido» mataría al trabajador sano) y `limit` (la pantalla muestra «Free limit reached»); la ruta del informe se fija después de la etiqueta `:escribir` para que el salto de etiqueta no se la salte.

## Decisiones
- `# DECISION: EZ_PYTHON manda; si no, python3 o python, el primero que funcione.` — hace falta un intérprete que se pueda sustituir para reproducir el `\r` de Windows en las pruebas.
- `# DECISION: el supervisor nunca mata una terminal que hc reporte viva por un estado «desconocido»: tras 2 min desconocido, sigue esperando (y lo dice cada 5 min).` — es el encargo (C0-3 + HD-0d). Como consecuencia, si el núcleo de hc muere del todo, el supervisor espera indefinidamente: se puede interrumpir; los topes son configurables con `EZ_UNKNOWN_GRACE`/`EZ_UNKNOWN_EVERY`.
- `# DECISION: si ya hay una terminal VIVA con nuestro nombre, el trabajador sigue: la seguimos. No se lanza otro intento y no se mata ninguna terminal.` — una terminal viva con el mismo nombre puede ser de otro intento anterior; matarla era justo el bug. Ahora el supervisor se engancha a ella.
- `# DECISION: el worker del aviso está atascado en el límite; se cierra su terminal para no dejar un nombre ocupado que ataría el siguiente intento.` — el aviso de límite de uso es un fallo definitivo: se registra `fallido` (código 2) y se cierra esa terminal; sin eso, el nombre ocupado ataría todos los intentos futuros.
- `# DECISION: la sesión de OpenCode guarda el modelo con que se creó; si EZ_MODEL cambia, se abre una sesión nueva en vez de continuar la vieja (que ignora el modelo nuevo).` — el estado de la sesión va en `<subagents>/sessions/<nombre>.state` (sid, modelo y contador), fuera del worktree para no ensuciar `git status`.
- `# DECISION: código de salida 2 para el límite de uso` — el mismo que «sin informe completado tras N intentos»; el motivo queda en el registro §13 y en el aviso al padre.
- Se mantiene el aviso de «colgado al arrancar» (pantalla vacía 3 min) solo para workers recién lanzados: una terminal a la que el supervisor se engancha no se mata.

## Verificación
- `bash -n supervise3.sh` (Git Bash, `bash -o igncr -n`): `bash -n supervise3.sh: 0`
- `bash -n probar.sh` (Git Bash, `bash -o igncr -n`): `bash -n probar.sh: 0`
- `probar.sh` **fuera de hc** (Git Bash, el entorno donde falló; 11 casos, salida completa de 132 líneas):
  ```
  [HD0T9] informe completado en el intento 1
  [HD0T9] código de supervise3.sh: 0
  OK caso 9: el trabajador vivo terminó su informe
  OK caso 9 una sola terminal: 1
  OK caso 9: terminó en el primer intento
  [HD0T10] límite de uso en t19r: Free limit reached. Try again later.
  [HD0T10] código de supervise3.sh: 2
  OK registro fallido por límite: fallido
  OK registro detalle del límite: límite de uso: Free limit reached. Try again later.
  OK aviso aviso de límite de uso
  OK caso 10 sin reintento: 1
  [HD0T11] aviso: el modelo cambió (opencode/m1 -> opencode/m2); abro una sesión nueva (ses_hd0t11s2)
  OK sesión nueva con el modelo nuevo: 1
  OK la sesión vieja no se repite: 1
  [pruebas] resultado: VERDE
  codigo de salida: 0
  ```
- `probar.sh` **dentro de hc** (`HC_TERMINAL_ID=t18r`, la terminal de este subagente; 11 casos, salida completa de 132 líneas):
  ```
  [HD0T9] informe completado en el intento 1
  [HD0T9] código de supervise3.sh: 0
  OK caso 9: el trabajador vivo terminó su informe
  OK caso 9 una sola terminal: 1
  OK caso 9: terminó en el primer intento
  [HD0T10] límite de uso en t1b9: Free limit reached. Try again later.
  [HD0T10] código de supervise3.sh: 2
  OK registro fallido por límite: fallido
  OK registro detalle del límite: límite de uso: Free limit reached. Try again later.
  OK aviso aviso de límite de uso
  OK caso 10 sin reintento: 1
  [HD0T11] aviso: el modelo cambió (opencode/m1 -> opencode/m2); abro una sesión nueva (ses_hd0t11s2)
  OK sesión nueva con el modelo nuevo: 1
  OK la sesión vieja no se repite: 1
  [pruebas] resultado: VERDE
  codigo de salida: 0
  ```
- pytest / ruff / cargo: no aplican (encargo de shell; no hay código Python ni Rust en el alcance).
- Limpieza: al terminar no queda ninguna terminal de la prueba (`hc ls --all` → 0 terminales con nombre `hd0*`); las terminales de los otros subagentes (ir-1, nv-1b, dx-1c) no se tocaron.

## Pendiente o riesgos
- La detección de límite de uso busca «quota» en la pantalla del worker: un worker que muestre esa palabra por otro motivo (un volcado de configuración, por ejemplo) sería marcado `fallido`. Es lo que pide el encargo; si molesta, conviene acotar el patrón.
- Con la regla nueva, un estado «desconocido» permanente (núcleo de hc caído) deja al supervisor esperando indefinidamente, a propósito; se puede interrumpir y los tiempos de aviso son configurables.
- Si el nombre del trabajador está ocupado por una terminal viva que no es su worker (por ejemplo, de otro intento anterior), el supervisor se engancha a ella y espera a que termine antes de lanzar otro intento: es el comportamiento pedido, pero alarga el primer intento.
- La sesión nueva por cambio de modelo solo aplica a las llamadas con `--session`/`--title` de OpenCode; con `EZ_OPENCODE_MODE=run` el cambio de modelo también evita continuar la sesión vieja (el título nuevo no la encuentra).
- El cambio de modelo se guarda en `<subagents>/sessions/<nombre>.state`; si alguien borra esa carpeta, el supervisor vuelve a la sesión `s1` con el modelo nuevo (OpenCode conservaría el modelo viejo). Conviene no borrar esa carpeta al cambiar de modelo.

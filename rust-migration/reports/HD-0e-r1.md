## Resultado
- Brief/tarea: HD-0e — corrección de la revisión de HD-0d (falsos positivos del límite de uso)
- Estado: completado

## Cambios
- `rust-migration/tools/orquestacion/supervise3.sh`: v5. La detección de C0-6 ya no da falsos positivos: un aviso de límite de uso solo cuenta si (a) hc **no** reporta al agente en estado `working`, (b) la pantalla no ha cambiado entre dos lecturas seguidas separadas al menos 15 s, y (c) la frase está en las **últimas 15 líneas** con texto de la pantalla. Solo cuentan las seis frases completas de §3 («Free limit reached», «5 hour usage limit reached», «You've hit your usage limit», «Usage limit reached», «rate limit reached», «quota exceeded», sin distinguir mayúsculas); las palabras sueltas («quota», «rate limit», «usage limit») quedan fuera. Nuevos ayudantes: `agent_state` (estado que reporta el agente), `screen_tail` (las últimas 15 líneas) y `limit_hit` (deja la frase en `LIMIT_HIT`).
- `rust-migration/tools/orquestacion/pruebas/probar.sh`: casos 10 (actualizado), 12, 13, 14 y 15. Además: `PYTHONIOENCODING=utf-8` para leer de vuelta el registro en UTF-8 (el Python de Windows imprime en cp1252 cuando la salida es una tubería y el detalle acentuado «límite de uso» no coincidía) y `notice` espera hasta 10 s a que el padre falso escriba la línea (`hc send` vuelve antes; el caso 3 fallaba de forma intermitente). El caso 12 ahora dura más que la ventana de pantalla quieta, así que el patrón sí se evalúa con las palabras sueltas en pantalla.
- `rust-migration/tools/orquestacion/pruebas/fake-hc.sh`: `EZ_FAKE_STATUS=idle|working|blocked` — el estado que «reporta» el agente en `hc get --json`.
- `rust-migration/tools/orquestacion/pruebas/fake-worker.cmd`: modos nuevos `chatter`, `limit-top` y `limit-work`; `chatter` imprime las palabras sueltas y vive ~44 s para que se cumpla la ventana de pantalla quieta.

## Decisiones
- `# DECISION: las 6 frases completas de §3 son el patrón; las palabras sueltas («quota», «rate limit», «usage limit») quedan fuera a propósito.` — es el criterio de §3 del diseño (el mismo que corrigió NV-1b).
- `# DECISION (HD-0e): la función NO se llama en `$(...)`: en un subshell se perdería el estado de «pantalla quieta» (PREV_*).` — el primer intento fallaba por esto: la detección nunca llegaba a evaluarse.
- `# DECISION (HD-0e): la terminal del trabajador NO se cierra cuando se detecta el límite.` — el padre decide («esperar» o «ez retry», §2/§4 del diseño); el aviso le dice que lea la pantalla.
- `# DECISION (HD-0e): `PYTHONIOENCODING=utf-8` en `probar.sh`.` — el registro es UTF-8 y la comparación del detalle con acentos fallaba fuera de hc; sin esto, el caso 10 no puede comparar el detalle real.
- `# DECISION (HD-0e): `notice` reintenta hasta 10 s (40 × 0.25 s).` — `hc send` vuelve antes de que el padre falso lea y escriba la línea; el caso 3 fallaba de forma intermitente. La espera no cambia lo que se comprueba.
- `# DECISION (HD-0e): el modo `chatter` vive más que la ventana de 15 s.` — con los ~9 s anteriores el trabajador terminaba antes de la segunda lectura y la comprobación de palabras sueltas nunca llegaba a ejecutarse.

## Verificación
- Entorno: Windows, Git Bash 5.3.15 (Cygwin). Los `.sh` tienen CRLF; todo se ejecuta con `bash -o igncr`.
- `bash -n`, **fuera de hc** (`env -u HC_TERMINAL_ID -u HC_SCOPE -u HC_TERMINAL_SOCKET`):
  ```
  OK rust-migration/tools/orquestacion/supervise3.sh
  OK rust-migration/tools/orquestacion/pruebas/probar.sh
  OK rust-migration/tools/orquestacion/pruebas/fake-hc.sh
  OK rust-migration/tools/orquestacion/pruebas/fake-parent.sh
  bash -n exit=0
  ```
- `bash -n`, **dentro de hc** (terminal `hd0e-bashn2`, lanzada con `hc run`):
  ```
  OK rust-migration/tools/orquestacion/supervise3.sh
  OK rust-migration/tools/orquestacion/pruebas/probar.sh
  OK rust-migration/tools/orquestacion/pruebas/fake-hc.sh
  OK rust-migration/tools/orquestacion/pruebas/fake-parent.sh
  bash -n exit=0
  ```
- `probar.sh` **fuera de hc** (Git Bash; `env -u HC_TERMINAL_ID -u HC_SCOPE -u HC_TERMINAL_SOCKET bash -o igncr rust-migration/tools/orquestacion/pruebas/probar.sh`): 168 líneas, 0 «FALLO», código de salida 0.
  ```
  [pruebas] sin HC_TERMINAL_ID: la prueba corre fuera de hc (Git Bash); el padre falso se crea sin padre
  [pruebas] terminal padre falso: t1kd
  [HD0T9] intento 1 en t1kt (padre t1kd, agente opencode)
  [HD0T9] informe completado en el intento 1
  [HD0T9] código de supervise3.sh: 0
  OK caso 9: el trabajador vivo terminó su informe
  OK caso 9 una sola terminal: 1
  OK caso 9: terminó en el primer intento
  [pruebas] caso 10: HD-0d/HD-0e C0-6 un aviso de límite de uso deja el intento «fallido» sin reintento
  [pruebas]   (HD-0e: solo cuenta con el agente parado y la pantalla quieta)
  [HD0T10] intento 1 en t1kv (padre t1kd, agente opencode)
  [HD0T10] límite de uso en t1kv: Free limit reached. Try again later.
  [HD0T10] código de supervise3.sh: 2
  OK registro fallido por límite: fallido
  OK registro detalle del límite: límite de uso: Free limit reached. Try again later.
  OK aviso aviso de límite de uso
  OK caso 10 sin reintento: 1
  [pruebas] caso 11: HD-0d C0-6 si EZ_MODEL cambia, se abre una sesión nueva
  [HD0T11] intento 1 en t1kw (padre t1kd, agente opencode)
  [HD0T11] informe completado en el intento 1
  [HD0T11] código de supervise3.sh: 0
  [HD0T11] aviso: el modelo cambió (opencode/m1 -> opencode/m2); abro una sesión nueva (ses_hd0t11s2)
  [HD0T11] intento 1 en t1kx (padre t1kd, agente opencode)
  [HD0T11] informe completado en el intento 1
  [HD0T11] código de supervise3.sh: 0
  OK sesión nueva con el modelo nuevo: 1
  OK la sesión vieja no se repite: 1
  [pruebas] caso 12: HD-0e palabras sueltas («quota», «rate limit») con el agente parado no son un límite
  [HD0T12] intento 1 en t1ky (padre t1kd, agente opencode)
  [HD0T12] informe completado en el intento 1
  [HD0T12] código de supervise3.sh: 0
  OK caso HD0T12: el trabajador terminó su informe (no se lo tocó)
  OK caso 12: las palabras sueltas estaban en la pantalla
  OK registro caso 12: terminado
  [pruebas] caso 13: HD-0e palabras sueltas con el agente «working» tampoco son un límite
  [HD0T13] intento 1 en t1kz (padre t1kd, agente opencode)
  [HD0T13] informe completado en el intento 1
  [HD0T13] código de supervise3.sh: 0
  OK caso HD0T13: el trabajador terminó su informe (no se lo tocó)
  OK registro caso 13: terminado
  [pruebas] caso 14: HD-0e la frase de límite arriba de una pantalla larga no cuenta
  [HD0T14] intento 1 en t1m0 (padre t1kd, agente opencode)
  [HD0T14] informe completado en el intento 1
  [HD0T14] código de supervise3.sh: 0
  OK caso HD0T14: el trabajador terminó su informe (no se lo tocó)
  OK caso 14: la frase estaba en la pantalla (pero no en las últimas 15 líneas)
  OK registro caso 14: terminado
  [pruebas] caso 15: HD-0e la frase completa con el agente «working» no cuenta
  [HD0T15] intento 1 en t1m1 (padre t1kd, agente opencode)
  [HD0T15] informe completado en el intento 1
  [HD0T15] código de supervise3.sh: 0
  OK caso HD0T15: el trabajador terminó su informe (no se lo tocó)
  OK caso 15: la frase estaba en la pantalla (pero el agente estaba trabajando)
  OK registro caso 15: terminado
  [pruebas] resultado: VERDE
  ```
  (Se quitaron las líneas `warning: in the working copy of …` que imprime git al preparar los repos de prueba; el resto es literal.)
- `probar.sh` **dentro de hc** (terminal `hd0e-suite`; `HC_TERMINAL_ID` derivado con `hc get hd0e-suite`): 167 líneas, 0 «FALLO», código de salida 0.
  ```
  [pruebas] terminal padre falso: t1jr
  [HD0T9] intento 1 en t1k4 (padre t1jr, agente opencode)
  [HD0T9] informe completado en el intento 1
  [HD0T9] código de supervise3.sh: 0
  OK caso 9: el trabajador vivo terminó su informe
  OK caso 9 una sola terminal: 1
  OK caso 9: terminó en el primer intento
  [pruebas] caso 10: HD-0d/HD-0e C0-6 un aviso de límite de uso deja el intento «fallido» sin reintento
  [pruebas]   (HD-0e: solo cuenta con el agente parado y la pantalla quieta)
  [HD0T10] intento 1 en t1k5 (padre t1jr, agente opencode)
  [HD0T10] límite de uso en t1k5: Free limit reached. Try again later.
  [HD0T10] código de supervise3.sh: 2
  OK registro fallido por límite: fallido
  OK registro detalle del límite: límite de uso: Free limit reached. Try again later.
  OK aviso aviso de límite de uso
  OK caso 10 sin reintento: 1
  [pruebas] caso 11: HD-0d C0-6 si EZ_MODEL cambia, se abre una sesión nueva
  [HD0T11] intento 1 en t1k6 (padre t1jr, agente opencode)
  [HD0T11] informe completado en el intento 1
  [HD0T11] código de supervise3.sh: 0
  [HD0T11] aviso: el modelo cambió (opencode/m1 -> opencode/m2); abro una sesión nueva (ses_hd0t11s2)
  [HD0T11] intento 1 en t1k7 (padre t1jr, agente opencode)
  [HD0T11] informe completado en el intento 1
  [HD0T11] código de supervise3.sh: 0
  OK sesión nueva con el modelo nuevo: 1
  OK la sesión vieja no se repite: 1
  [pruebas] caso 12: HD-0e palabras sueltas («quota», «rate limit») con el agente parado no son un límite
  [HD0T12] intento 1 en t1k8 (padre t1jr, agente opencode)
  [HD0T12] informe completado en el intento 1
  [HD0T12] código de supervise3.sh: 0
  OK caso HD0T12: el trabajador terminó su informe (no se lo tocó)
  OK caso 12: las palabras sueltas estaban en la pantalla
  OK registro caso 12: terminado
  [pruebas] caso 13: HD-0e palabras sueltas con el agente «working» tampoco son un límite
  [HD0T13] intento 1 en t1k9 (padre t1jr, agente opencode)
  [HD0T13] informe completado en el intento 1
  [HD0T13] código de supervise3.sh: 0
  OK caso HD0T13: el trabajador terminó su informe (no se lo tocó)
  OK registro caso 13: terminado
  [pruebas] caso 14: HD-0e la frase de límite arriba de una pantalla larga no cuenta
  [HD0T14] intento 1 en t1kb (padre t1jr, agente opencode)
  [HD0T14] informe completado en el intento 1
  [HD0T14] código de supervise3.sh: 0
  OK caso HD0T14: el trabajador terminó su informe (no se lo tocó)
  OK caso 14: la frase estaba en la pantalla (pero no en las últimas 15 líneas)
  OK registro caso 14: terminado
  [pruebas] caso 15: HD-0e la frase completa con el agente «working» no cuenta
  [HD0T15] intento 1 en t1kc (padre t1jr, agente opencode)
  [HD0T15] informe completado en el intento 1
  [HD0T15] código de supervise3.sh: 0
  OK caso HD0T15: el trabajador terminó su informe (no se lo tocó)
  OK caso 15: la frase estaba en la pantalla (pero el agente estaba trabajando)
  OK registro caso 15: terminado
  [pruebas] resultado: VERDE
  ```
  (Mismo filtro de las líneas `warning: in the working copy of …`.)
- Las dos salidas completas son idénticas salvo las direcciones de terminal y la línea «sin HC_TERMINAL_ID» del entorno de fuera (`diff` sin diferencias tras normalizar direcciones y rutas temporales).
- Caso 12: el trabajador falso imprime «quota», «rate limit» y «usage limit», está `idle` y vive ~44 s; el supervisor evalúa el patrón con la pantalla quieta (dos veces) y no lo marca `fallido`.
- pytest / ruff / cargo: no aplican (el alcance es shell; no hay código Python ni Rust).
- Limpieza: no queda ninguna terminal `hd0*`; `ir-1` (t18q), `nv-1b` (t196), `dx-1d` (t1f6) y esta (`hd-0e`, t1f7) siguen vivas. No se lanzó ningún agente real ni se usó `EZ_SHOW` ni `herdr-app`.

## Pendiente o riesgos
- El supervisor sigue leyendo el resumen del informe con Python y la codificación por omisión de Windows cuando la salida es una tubería (`report_summary`): si esa línea trae acentos, el registro puede quedarse sin escribir. No afecta a la detección del límite ni a las pruebas (el resumen del trabajador falso es ASCII); se arregla con la misma línea que en `probar.sh` (`PYTHONIOENCODING=utf-8`) si aparece en uso real.
- Un agente bloqueado por cuota que siga repintando la pantalla (un spinner) no se detectará: es el criterio de §3 (pantalla quieta). El aviso de «sin avance» de NV-1 cubre ese caso.
- La detección mira las últimas 15 líneas **con texto**: si el aviso de límite queda por encima de 15 líneas de registros posteriores, no cuenta (es lo pedido; el padre siempre puede leer la pantalla).

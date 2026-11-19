# HD-0b — Correcciones de la revisión de HD-0 (pedidas por la persona)

Revisión del agente principal sobre `work/HD-0`: el encargo original quedó bien
(`probar.sh` en verde con OpenCode y mcode falsos, padre, nombre, etiquetas, reintentos).
Vuelve con dos pedidos de la persona. Mismo alcance de archivos que HD-0.

## C0-1. OpenCode con su interfaz, no como texto plano
Hoy OpenCode corre con `opencode run …` (modo de un solo disparo): en Herdr se ve como
texto plano, sin interfaz ni entrada, y no reporta estado. Lánzalo en su **modo
interactivo**, como mcode:
- `opencode --standalone --auto --session <id> --prompt "<encargo>"`. La interfaz no tiene
  `--model`: el modelo va en `OPENCODE_CONFIG_CONTENT={"model":"<modelo>"}`. Como `hc run`
  no pasa entorno, usa como envoltura **sin shell** el `env.exe` de Git
  (`C:/Program Files/Git/usr/bin/env.exe OPENCODE_CONFIG_CONTENT=… opencode …`): fija la
  variable y ejecuta el programa con sus argumentos en lista, sin problemas de comillas.
- `--session` crea la sesión si no existe: usa un id estable por trabajador para que los
  reintentos la continúen (`--session <id> --prompt "<continuar>"`). Comprueba qué formato
  de id acepta abriendo `opencode --standalone --session <id>` **sin** `--prompt` (no gasta
  nada) y ciérralo de inmediato; si no acepta ids propios, busca la sesión por título como
  hoy y explícalo.
- Fin del trabajo como mcode: informe completado + worktree confirmado (o 15 min) → guardar
  pantalla y matar la terminal. `EZ_OPENCODE_MODE=run` deja el modo anterior por si hace
  falta.

## C0-2. Estado nativo de cada trabajador y avisos al padre (§13 del diseño)
El supervisor escribe el registro de §13 para cada trabajador y avisa al padre sin que
se lo pidan:
- Archivo `<raíz>/<padre>/<nombre>.json` con `<raíz>` = `EZ_SUBAGENTS_DIR` o
  `%LOCALAPPDATA%\EzAgent\subagentes`, `<padre>` = `HC_TERMINAL_ID` con `/` → `_`, campos
  y nombres de §13, escritura atómica (temporal + renombrar) con el intérprete de Python
  disponible.
- `en_proceso` al lanzar el primer intento; `terminado` con informe completado y worktree
  confirmado (`summary` = la línea «Estado: …» o la primera viñeta de «## Resultado»,
  `report` = ruta del informe); `fallido` al agotar los intentos (`detail` = «sin informe
  completado tras N intentos»). `by` = «supervisor».
- En cada uno de esos cambios: `hc send "$HC_TERMINAL_ID" "<aviso>"` con los formatos de §13
  (`[EzAgent · aviso] <nombre> (<tipo> · <modelo>) terminó. Resumen del subagente: «…».
  Informe: <ruta>…`, `… falló: <motivo>. Revisa su pantalla: hc read <t>`) y `notified` =
  ese estado. Si `hc send` falla, se registra en la salida y se sigue.
- Sin `HC_TERMINAL_ID`: no hay registro ni avisos (y se dice al empezar).

## Pruebas (amplía `pruebas/probar.sh`; sin cuota)
- C0-1: con `EZ_AGENT_BIN` falso, el argv lanzado es la envoltura `env.exe … opencode
  --standalone --auto --session … --prompt …` y lleva `OPENCODE_CONFIG_CONTENT`; reintento
  con la misma sesión; cierre tras informe + commit.
- C0-2: con `EZ_SUBAGENTS_DIR` temporal y un **padre falso** (una terminal de hc propia de
  la prueba que guarde lo que recibe, p. ej. `fake-ws-agent --echo` o un guion que lea
  stdin), el registro pasa por `en_proceso` → `terminado` (y otro caso a `fallido`) y el
  padre falso **recibe** los avisos con el formato de §13. Nunca envíes avisos a la
  terminal de la persona ni a la del agente principal durante las pruebas.

## Entrega
`bash -n` y `probar.sh` en verde con la salida pegada. Informe en
`rust-migration/reports/HD-0b-r1.md`.

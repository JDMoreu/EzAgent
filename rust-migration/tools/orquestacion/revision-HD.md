# Revisión de la ola HD (registro del elefante)

## Correcciones pendientes por encargo

### HD-0 (supervisor)
- C0-1 (pedida por la persona, 19-nov): los trabajadores de OpenCode salen como texto
  plano porque se lanzan con `opencode run` (modo de un solo disparo, sin interfaz ni
  entrada, no reporta estado a Herdr). Lanzar OpenCode en su **modo interactivo**:
  `opencode --standalone --auto --prompt "<encargo>"` con el modelo en
  `OPENCODE_CONFIG_CONTENT={"model":"<modelo>"}` (la interfaz no tiene `--model`; `hc run`
  no pasa entorno, así que hace falta la envoltura de entorno); reintentos con
  `opencode --standalone --session <id> --auto --prompt "<continuar>"`. Fin del trabajo
  como con mcode: informe completado + `git status` limpio (o 15 min), y luego matar la
  terminal.
- C0-2 (pedida por la persona, 19-nov): estado nativo de cada trabajador (§13 del
  diseño). El supervisor escribe el registro de §13 (`EZ_SUBAGENTS_DIR` o
  `%LOCALAPPDATA%\EzAgent\subagentes\<padre>\<nombre>.json`, escritura atómica):
  `en_proceso` al lanzar, `terminado` con informe completado (resumen = primera línea de
  «## Resultado» o «Estado»), `fallido` al agotar los intentos; y avisa al padre sin que
  lo pida con `hc send $HC_TERMINAL_ID "<aviso con el formato de §13>"` en cada cambio.
  Pruebas con `EZ_SUBAGENTS_DIR` temporal y un padre falso (no la terminal de la persona).

### HD-3 (validación contra el catálogo) — revisión 1: devuelto
- Pruebas: 191 de ez-herdr en verde; mutaciones detectadas (aceptar cualquier modelo →
  `modelos_fuera_del_catalogo_se_rechazan` falla; Qwen con niveles →
  `qwen_no_tiene_niveles_de_esfuerzo` falla). Alcance respetado.
- Devuelto con HD-3b: R3-1 no duplicar `match_mcode_model`/`resolve_codex` (la DECISION
  decía que escriben configuración: falso, son puras); R3-2 esfuerzo sin modelo en
  OpenCode se aceptaba (`--effort ultra`); R3-3 `check_program` debe usar
  `agent_command` como el lanzamiento.

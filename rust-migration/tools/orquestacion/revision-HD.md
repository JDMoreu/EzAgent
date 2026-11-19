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

### HD-2 (lanzar como hijo y ubicar, mcode) — validado y fusionado
- Revisado `child.rs`: `hc run --bg --parent --name --cwd [--no-notify] [--outlive-parent]
  --label agent=… --label …(orden) -- <envoltura>`, ubicación tree/tab/right/down con
  `pane.focus` del padre y reintento ante «no running terminal» (0,5 s, tope 15 s),
  `name_taken` marcado, padre real releído con `hc get`. Corrección del `--name` inválido
  del lanzamiento en segundo plano (`hc_slug`) con el fixture de paridad ajustado (7
  expectativas `--name`; aceptado: el oráculo Python está retirado).
- ez-herdr 198 + paridad en verde. Mutaciones: sin `--parent` → falla
  `con_padre_el_lanzamiento_va_por_hc_run_con_parent`; sin reintento → falla
  `terminal_keep_reintenta…`; panel sin enfocar al padre → fallan dos pruebas. Bien.
- ez-app: 348/349; `autonomous::tests::team::a_dirty_round_creates_one_fix_task_per_blocking_finding`
  falló una vez bajo carga (279 s) y pasa 3/3 aislada: inestable, no causada por HD-2.
- Menor, no bloquea (R2-1, va en HD-4): el resultado pone `visible: false` aunque quede
  en pestaña o panel; debe ser `placed != tree`.

### HD-0 (supervisor) — revisión 1: cumplido el original, devuelto con C0-1 y C0-2 (HD-0b)
- `probar.sh` en verde (OpenCode y mcode falsos con padre, nombre, etiquetas; reintento sin
  informe → código 2). Sin comandos `hc server`: el «core restarted» de su informe no lo
  causó el supervisor.
- Devuelto con HD-0b: C0-1 (OpenCode interactivo vía `env.exe` + `--session`) y C0-2
  (registro §13 + avisos al padre).

### DX-2 (respuestas cortas de voz) — diagnóstico validado, implementación DX-2b
- Referencias comprobadas (fast.rs COURTESY_WORDS/strip_courtesy, note_done como
  `role: user` con 12 000 caracteres, done_question). Medición real: turno corto ~4,6 s
  hasta el sonido y ~11 s en total; el atajo local, 0–16 ms.
- Hueco detectado por el elefante: `done_question` termina con «¿Quieres que te resuma…?»;
  tras una pregunta, «ok/dale» = «sí». DX-2b: avisos de «terminó» como afirmación
  («Cuando quieras te cuento qué hizo.»), sin atajo tras una pregunta, y respuestas más
  naturales.

### HD-3b — validado; HD-3 fusionado
- R3-1 (reusa `match_mcode_model`/`resolve_codex`/`agent_command` como `pub(crate)`),
  R3-2 (esfuerzo sin modelo contra el modelo por omisión o la unión) y R3-3 hechas.
  ez-herdr 193 + paridad en verde. Mutación: quitar la validación sin modelo → falla
  `esfuerzo_sin_modelo_usa_los_niveles_del_modelo_por_omision`. Bien.
- C0-3 (nuevo, para el supervisor): durante un reinicio del núcleo de hc, `hc get` falló un
  momento y `field` devolvió vacío: el bucle dio el intento por terminado con la terminal
  viva y lanzó otro (falló por `name_taken`; no hubo dos trabajadores a la vez). Debe tratar
  un `hc get` fallido como «desconocido» (reintentar) y no lanzar otro intento mientras una
  terminal con su nombre siga viva.

### HD-7 (registro de estados) — revisión 1: devuelto (HD-7b)
- Bien: transiciones, candado con carpeta del padre, escritura atómica, textos de §13,
  `pending_notices` (mutación «avisar cambios del padre» → falla su prueba). 201 en verde.
- Hueco: permitir `validado → terminado` no lo detecta ninguna prueba. HD-7b: tabla
  completa de 16 transiciones y estado inicial de `create`.
- Aceptado: aviso corto propio para `en_proceso` (lo usa el supervisor cuando lanza).

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

### HD-7b — validado; HD-7 fusionado
- Tabla de 16 transiciones y `create` con estados finales. Mutaciones `validado→terminado`
  y `fallido→en_proceso` → fallan `transition_table_all_sixteen_pairs` (y otra). 203 en verde.

### Incidente de memoria (19-nov, ~11:20)
- La medición de DX-1 `diag_chat_history_cost` (ejemplo propio del subagente) llegó a
  11,4 GB de RAM; la memoria virtual libre bajó a 0,6 GB y Claude Code detuvo los cinco
  procesos de seguimiento del agente principal (supervisores de HD-5, HD-0b y DX-2b y
  vigilantes de HD-4 y DX-1). Los subagentes siguieron vivos en sus terminales de hc.
- El agente principal detuvo esa medición (RAM libre 4,2 → 15,1 GB) y le pidió a DX-1
  repetir con 10/100/1000 entradas, una a la vez y con tope de 1 GB. El dato es evidencia
  para el diagnóstico del congelamiento.
- Lección para los briefs: toda medición con tope de memoria y tamaño; no más de 3
  subagentes compilando Rust a la vez.

### HD-4 (ez delegate --agent, mcode) — validado y fusionado
- Revisado: validación antes de crear nada, nombre §6 con reintento por `name_taken` solo
  si es automático, etiquetas §3, `notify=false` por omisión (`--hc-notes`), primer
  mensaje §7, modo local vs. árbitro en `ez-ctl`, agente falso `--echo`, R2-1 (`visible`).
  ez-herdr 230, ez-ctl 35, ez-fakes 62 en verde. Mutaciones: ignorar el rechazo del
  catálogo → falla `un_rechazo_del_catalogo_no_crea_nada`; reintentar un nombre pedido →
  falla `un_nombre_pedido_que_está_ocupado_es_un_rechazo`. Bien.
- **Incidente**: el subagente ejecutó un `opencode` real con `ez delegate` (su terminal
  tenía `HC_TERMINAL_ID`). Según su informe, OpenCode salió de inmediato con código 1 sin
  recibir el encargo: sin llamada a modelos ni cuota. Pendiente investigar ese código 1
  (puede ser un fallo real del lanzamiento de OpenCode como hijo).
- Menor (R4-1, va en HD-8): `valid_name` de `delegate/names.rs` duplica `hc_valid_name` de
  `child.rs`; unificar.

### HD-5 (enjambre) — revisión 1: devuelto (HD-5b)
- Bien: `LaunchRequest` con padre/etiquetas/`show=tab`/`notify=false`/`outlive_parent`, paso
  a `RunAgent`. ez-swarm 134 en verde; quitar el padre → fallan dos pruebas.
- Mal: la raíz = «primer miembro con dirección que no sea el que se lanza» invierte el árbol
  al reabrir la raíz (la cuelga del segundo). Mutación «la raíz puede ser el propio miembro»
  no la detecta nadie. HD-5b: raíz = primer miembro siempre.

### DX-1 (congelamiento del chat, mcode) — diagnóstico validado; arreglo DX-1b (fase A)
- Comprobado en el código: `prepare_view` maqueta todo el historial en el hilo que abre
  (`lifecycle.rs:105-114`); `ChatPanel::render` pinta con `PanelState` tomado
  (`window.rs:332`); caché topeada en 400 que expulsa el id más pequeño
  (`entries.rs:100-106`); la firma mezcla parpadeo y tic del orbe (`compose.rs:91-105`).
- Mediciones del subagente: abrir el chat ~89 ms por mensaje (33 mensajes ≈ 3 s de app
  congelada); >401 mensajes, cada cuadro remaqueta todo (×5 950); con el chat abierto, ~1
  núcleo entre los dos hilos de ventana. Sin fuga de GDI/USER ni de memoria.
- Nota menor: las horas de las lecturas en vivo del informe no cuadran entre sí (CPU 511 s
  «a las 09:56»); no cambia las conclusiones, que salen del código y de la medición.
- DX-1b (mcode): pasos 1, 3, 4, 5, 6, 7 y 8; la virtualización (paso 2) queda para después.

### DX-2b (acuses cortos) — revisión 1: devuelto (DX-2c, en cola por el límite de 3)
- Bien: avisos de «terminó» como afirmación, sin atajo tras pregunta, respuestas A3,
  `is_pure_acknowledgement`, kind `ack` para no rearmar, regla de voz. 678 pruebas en
  verde según el informe.
- Mal: las pruebas escriben en el `latency.jsonl` real de la persona (9 líneas
  `ack_fast` comprobadas). DX-2c: carpeta de registros temporal para las pruebas.

### HD-0b (supervisor) — revisión 1: bien con falsos; devuelto (HD-0c) por fallos reales
- Probado por el elefante con el OpenCode real **sin encargo** (sin llamadas a modelos):
  `env.exe opencode` ejecuta el guion de bash de npm → 127; con la ruta nativa
  (`node_modules/@opencode/cli/bin/opencode.exe`) arranca; `--session hd-probe-s1` → «Expected
  a string starting with "ses"» (código 1); `--session ses_hdprobe01` y sin sesión → abre la
  interfaz y **reporta `idle` a hc** (OpenCode interactivo sí informa estado a Herdr).
- HD-0c: C0-3, C0-4 (resolver el ejecutable nativo), C0-5 (ids `ses_…`) y un humo real opt-in
  sin encargo.

### HD-8 (estado y avisos, mcode) — validado y fusionado
- `ez delegate` crea el registro (`en_proceso`/`fallido`) y arranca el vigilante oculto;
  `ez report --state`, `ez validate` (padre o hermano), `ez tasks`, `ez watch`; R4-1 hecha.
  ez-herdr 262, ez-ctl y ez-fakes 69 en verde.
- Mutaciones: el vigilante nunca marca `fallido` → falla
  `una_terminal_terminada_sin_informe_queda_fallida_y_se_avisa`; cualquiera puede validar →
  falla `el_validate_desde_un_desconocido_es_codigo_1`. Bien.
- Aceptado: el alta lleva `by = "ez delegate"` para que el padre reciba también el aviso
  «en proceso» (la persona pidió los cuatro estados).

### HD-5b — validado; HD-5 fusionado
- Raíz = primer miembro siempre; la raíz reabierta no cuelga de nadie; con la raíz sin
  dirección el tercero no cuelga del segundo. ez-swarm 136 en verde.
- La mutación «la raíz se toma a sí misma» no la detecta ninguna prueba porque es
  equivalente con el flujo actual (al reabrir, `members.rs:162`/la recuperación borran la
  dirección antes de relanzar); la guarda queda como defensa. Aceptado.

### HD-0c — validado; HD-0 fusionado (paso 1 completo)
- C0-3 (`hc get` fallido = desconocido; `free_name` evita `name_taken`), C0-4 (ejecutable
  nativo de OpenCode), C0-5 (sesión `ses_…s1`). `probar.sh` en verde con 8 casos falsos y
  el humo real sin encargo (OpenCode abre, hc lo ve `idle`, se cierra).
- Comprobado por el elefante: `EZ_RESOLVE_OPENCODE=1` resuelve `…/@opencode/cli/bin/opencode.exe`,
  modo interactivo, `SID=ses_hd0crevs1`, sin lanzar nada.
- Observación: el núcleo de hc se reinició varias veces durante el día (lo vieron HD-0, HD-0b
  y HD-0c); el supervisor ya lo tolera.

### DX-2c — validado (DX-2 completo, sin fusionar: espera la decisión de la persona)
- Las pruebas escriben latencia/auditoría en un temporal (`EZAGENT_LOGS_DIR`, fijado por el
  soporte de pruebas). Comprobado por el elefante: 75 pruebas de `fast::`, `app::events::` y
  `app::watch::` en verde y el `latency.jsonl` real sigue con 469 líneas (9 `ack_fast` de
  antes de la corrección).
- No se fusiona a `eco/main` hasta que la persona confirme el cambio de los avisos (sin
  pregunta al final) o pida conservarlos con pregunta.

### HD-6 (punta a punta, mcode) — validado y fusionado; fallos del producto → HD-9
- El elefante ejecutó `e2e_delegate.exe`: **126/126**, código 0, foco restaurado y Herdr
  limpio. Seis hijos (claude, opencode, mcode, qwen, codex, pi) con padre, nombre, etiquetas,
  primer mensaje con la marca; tree/tab/right sin perder el padre; rechazos sin terminal;
  estados terminado→validado, fallido por `hc kill`, avisos al padre falso; vigilante único.
- Hallazgos reales del producto (no corregidos por HD-6, como pedía): B1 errores de `hc run`
  por stderr (aviso espurio del vigilante, sin reintento n+1), B2 PowerShell sin `&` con
  ruta citada, B3 el vigilante lee el voice.json real, B4 `--show tab` abre en el espacio a
  la vista. HD-9 los corrige y quita los atajos del ejemplo.

### Cambio de modelo (19-nov ~12:40, pedido de la persona)
- El modelo gratuito agotó su límite diario («Free limit reached»): IR-1 se trabó al arrancar.
  Los subagentes de OpenCode pasan a `opencode-go/deepseek-v4.1-flash`.
- Hallazgo para el supervisor (C0-6): al retomar una sesión (`--session ses_…`), OpenCode
  conserva el modelo con que se creó e ignora `OPENCODE_CONFIG_CONTENT`. Para cambiar de
  modelo hubo que borrar la sesión (`opencode session delete ses_ir1s1 --standalone`). El
  supervisor debería detectar el aviso «Free limit reached» en pantalla y, si el modelo
  pedido cambió, abrir una sesión nueva.

### Pedido nuevo (19-nov): EzAgent lidia con las novedades
- Diseño `docs/designs/novedades-subagentes.md`: estado `bloqueado`, detector (límite de uso
  por pantalla con hora de reinicio, diálogo, sin avance), `ez retry` con sesión nueva, y la
  app de voz como padre que anuncia y responde. Encargos NV-1 (tras HD-9), NV-2 (tras NV-1 e
  IR-3), NV-3 (tras NV-2 y DX-2).

### IR-0 (investigación, mcode) — validado y fusionado
- Revisado: sin claves, tokens, correos ni rutas de proyectos (solo tipos de plan). Hallazgo
  clave: Codex guarda `used_percent`, `window_minutes` y `resets_at`; Claude Code, OpenCode,
  mcode y Pi solo tokens y costo; Qwen Code no está instalado. IR-2 implementa sus lectores.

### HD-9 (fallos de la punta a punta, mcode) — validado y fusionado
- B1 (JSON de error por stderr), B2 (`&` de PowerShell con programa citado), B3 (el vigilante
  no lee voice.json con `--hc`), B4 (pestaña del hijo por la etiqueta `herdr.space` del
  padre, sin mover el foco). Atajos del ejemplo quitados (verificado: sin `space.select` ni
  rutas con barras normales).
- El elefante corrió el ejemplo: **131/131**, código 0, foco intacto. 362 pruebas de
  ez-herdr y ez-ctl en verde. Mutaciones: sin `&` → fallan 3 pruebas de `text`; sin leer
  stderr → falla `un_nombre_ocupado_por_stderr_tambien_se_reconoce`. Bien.
- Cosmético (no bloquea): en «está bajo su padre» el detalle dice «no está en sub_agents»
  aunque la comprobación pasa; es el texto del ejemplo.

### Incidente: el supervisor nuevo mataba trabajadores sanos (19-nov ~13:30)
- IR-1 «falló tras 8 intentos»: cada intento se cortaba a los 2-3 min. Causa comprobada por el
  elefante: `python` en Windows imprime `\r\n` → `terminal_state` = `"live\r"` → «desconocido»
  → a los 2 min siguiente intento y `free_name` mata la terminal viva. Afectó a IR-1 (DeepSeek
  gastado en 8 arranques cortados) y habría afectado a NV-1 (su supervisor se detuvo a tiempo).
- Medidas: supervisor nuevo fuera de uso; IR-1 (último intento, vivo) y NV-1 sigue con un
  vigilante simple que no mata nada; HD-0d (mcode) lo corrige.
- Segundo hallazgo del mismo incidente: **TaskStop detiene la tarea de fondo de Claude Code
  pero no el `bash supervise3.sh` que lanzó** (en Windows sobrevive el árbol). Quedaban vivos
  un supervisor de IR-1 (12:38) y el de NV-1 (12:59), que seguían matando y relanzando
  terminales con el mismo nombre (uno con el modelo gratuito agotado: de ahí el aviso
  «ir-1 (OpenCode · opencode/hy4-flash-free) falló»). Detenidos por proceso (4 procesos).
  Desde ahora: detener un supervisor = TaskStop **y** `Stop-Process` de todo proceso con
  `supervise3.sh` en su línea de comandos.

### NV-1 (estado bloqueado y novedades, OpenCode) — revisión 1: devuelto (NV-1b)
- Bien: estado y transiciones, `uso.json`, diálogo solo fuera de `working`, aviso por episodio.
- Mal: el límite se detecta con el agente `working` y con palabras sueltas («quota», «rate
  limit») → falsos `bloqueado` y agentes anotados como agotados. El diseño listaba esos
  patrones genéricos (error del elefante, corregido en §3). NV-1b: solo con el agente parado,
  pantalla estable, últimas 15 líneas y frases completas.

### DX-1b (arreglo del congelamiento del chat, mcode) — validado y fusionado (sin recompilar la app)
- Medido por el subagente (antes→después): cuadro con 1 000 mensajes 53 s → 7 ms; RSS con
  100 mensajes 288 → 35 MB; clic durante un cuadro de 400 ms bloqueado → < 100 ms; repintado
  en reposo 2/s → 0; registro a archivo con rotación de 7 días.
- Comprobado por el elefante: `chat_frame_cost` 3/3, `hud_chat_parity` en verde, 162 pruebas
  `chat` de ez-hud, `log_rotation` 2/2. Mutación: volver a maquetar al abrir → falla
  `opening_the_chat_does_not_lay_out_the_history`. Bien.
- Pendiente (fase B, paso 2): el primer render sigue costando ~74 ms por mensaje (ya en el
  hilo de pintura, no congela la app): con 33 mensajes el contenido tarda ~2,4 s en aparecer.
  La virtualización lo resuelve.
- La app de la persona no cambia hasta recompilar: se le pregunta antes.

### HD-0d (supervisor) — revisión 1: \r corregido; devuelto (HD-0e) por falsos positivos
- Bien: `strip_cr` en todo dato, no mata terminales vivas por «desconocido», se engancha a
  una terminal viva con su nombre, sesión nueva al cambiar de modelo; `probar.sh` 11 casos en
  verde fuera de hc (el entorno donde falló).
- Mal: C0-6 detecta límites con palabras sueltas y con el agente trabajando, y cierra la
  terminal: IR-2 (que leerá IR-0) quedaría `fallido`. HD-0e: mismo criterio que NV-1b.
  El supervisor sigue fuera de uso hasta HD-0e.

### Decisiones de la persona (19-nov, tarde): «sí a todo»
- DX-2 integrado en `eco/main` (avisos de «terminó» sin pregunta + acuse local + regla de voz).
- `latency.jsonl` real: copia en `latency.jsonl.bak-2026-11-19` (470 líneas) y retiradas las 9
  líneas `ack_fast` que escribieron las pruebas antes de DX-2c (quedan 461).
- Recompilar la app de voz (release) con DX-1b + DX-2 y reiniciarla.

### Incidente: DX-1b hace caer la app al abrir el chat (19-nov 13:52) — revertido
- Tras recompilar con DX-1b + DX-2, la persona abrió el chat y la app se cerró: Windows
  `Application Error` 1000, `0xc0000409`, desplazamiento `0xbe3f2e` (pánico en un callback de
  Windows → aborto). Las pruebas de DX-1b (y la validación del elefante) nunca abrieron el
  chat en la app real: **fallo de validación del elefante**.
- Medidas: `git revert` de la fusión de DX-1b en `eco/main` (82b25b2); app recompilada con DX-2
  y sin DX-1b, reabierta (PID 22384) y viva. DX-1c en pausa (está sobre DX-1b). DX-1d (mcode)
  reproduce, corrige, añade `panic hook` y una prueba de humo de la app real.
- Regla nueva: todo cambio de interfaz necesita una prueba de humo de la app real antes de
  validarlo.

### Incidente de memoria 2 (19-nov ~14:00): whisper-server huérfanos
- Memoria virtual libre 3,8 GB. Causa: 7 `whisper-server` (12,5 GB), uno por cada arranque
  de la app de hoy; los de instancias muertas quedaron vivos. El elefante cerró 5 (+ sus
  envolturas de PowerShell); el de las 07:55 da «Acceso denegado» (probablemente elevado).
  Virtual libre → 15,4 GB. Bug del producto → DX-3 (Job Object, reutilizar el sano).

### Cambio de proveedor (19-nov ~14:10): créditos de mcode agotados
- `hd-0e` mostró «Code: 50110 … switch provider». DX-1d y HD-0e pasan a OpenCode + DeepSeek
  V4.1 Flash con notas de relevo (`DX-1d-relevo.md`, `prompt-HD-0e-relevo.txt`). DX-1c
  sigue en pausa. Desde aquí, todos los subagentes con OpenCode + DeepSeek.

### NV-1b — revisión 1: devuelto (NV-1c)
- Bien: frases completas de §3, `working` excluido, el recorte vale también para reanudar,
  pruebas de palabras sueltas, frase alta y frase al final con pantalla estable (300/300).
- Mal 1: `last_lines` cuenta líneas en blanco. Captura real de OpenCode (`hc read --lines 0`,
  121 líneas): las 15 últimas son 7 vacías + la caja de entrada + `[hc] incomplete…`; ni una
  línea de la conversación. El caso real de IR-1 nunca se habría detectado. Las pantallas
  falsas de las pruebas no tienen ese relleno.
- Mal 2: «la primera lectura cuenta como estable» contradice §3: al reiniciar el vigilante,
  un hijo `idle` que cite una frase queda `bloqueado` en la primera vuelta.
- NV-1c: líneas con texto (alfanuméricas), dos vueltas iguales, fixture desde la captura
  real (`briefs/NV-1c-pantalla-opencode.txt`).

### IR-1 — parado sin motivo (19-nov ~14:40)
- Dejó `cargo` en segundo plano y cerró su turno («en cuanto termine sigo…»): OpenCode
  interactivo no vuelve solo. La compilación ya había acabado; se le escribió que siga y que
  compile en primer plano. Es el caso «sin avance» de §3 (NV-1).

### NV-1c — validado y fusionado (6c5bd30)
- Líneas «con texto» (al menos un alfanumérico), primera lectura no estable, fixture con
  la captura real. Corrida del elefante: fmt, clippy, 304 + paridades en verde.
- Mutaciones (todas detectadas): contar líneas vacías → 2 fallos (incl. la captura real);
  primera lectura estable → 5; ignorar `working` → 1; pantalla entera → 4.
- Tras fusionar en `eco/main`: `cargo test -p ez-herdr -p ez-ctl` en verde. Worktree NV-1
  retirado; terminal `nv-1b` cerrada.
- Riesgo anotado: un agente `idle` cuyo último mensaje cite una frase completa de límite
  queda `bloqueado` tras dos vueltas (inherente a §3; se resuelve al cambiar la pantalla).

### IR-1 — revisión 1: devuelto (IR-1b)
- Bien: crate `ez-scan` con `Probe`, `entorno.json` atómico, inventario real sin abrir
  agentes (lista Claude Code, OpenCode, mcode, Codex, Pi; Qwen no instalado), GPU por
  `sysinfo` sin `unsafe`, `ez init`/`resources`/`inventory`.
- Mal 1 (medido a la vez): `ez resources` → commit libre 27 179 MB; Windows
  `FreeVirtualMemory` → 14 162 MB. RAM libre + swap libre ≠ commit libre: en el incidente
  de memoria la guarda habría dejado lanzar. IR-1b: `GetPerformanceInfo` /
  `GlobalMemoryStatusEx` con un único `unsafe` autorizado (excepción a `_COMMON-ECO`).
- Mal 2: cuenta como «pesados» a todos los hijos vivos (§4 dice compilando o midiendo):
  el agente principal no podría delegar con 4 hijos vivos aunque estén parados. IR-1b:
  pesado = árbol con `cargo`/`rustc`/`link`… o ≥ `heavy_mb`.

### Registro de estados: un archivo dañado deja ciego al vigilante → HD-10
- `ez tasks` falla entero con `registro inválido …ir-1.json` (escrito por el supervisor
  viejo: `validation` con campos `null`). Peor: `Watcher` usa `list(..).unwrap_or_default()`
  → con un archivo dañado no ve a **ningún** hijo y deja de avisar sin decir nada.
- HD-10 (OpenCode + DeepSeek, `hd-10` t1nb): lista tolerante, `validation` toda en `null` =
  `None`, `ez tasks` nombra los ilegibles, el vigilante avisa una vez por archivo.
- Además, el supervisor de HD-0d/0e escribe en `…\EzAgent\subagents` (inglés) y el producto
  lee `…\EzAgent\subagentes`: los registros del supervisor no se verían. Va a HD-0f.

### IR-1b — validado y fusionado (c25aa5d)
- `GetPerformanceInfo` en `ez-scan/src/real/commit.rs` (único `unsafe`, con `// SAFETY:`);
  pesado = árbol con `cargo`/`rustc`/`link`/`lld-link`/`cl` (editable) o ≥ 1500 MB.
- Corrida del elefante: fmt, clippy y pruebas de `ez-scan` + `ez-ctl` en verde; commit libre
  medido 3 veces contra `Win32_OperatingSystem.FreeVirtualMemory`: 0,72 %, 0,08 %, 1,62 %.
- Mutaciones: contar a todos como pesados → 2 fallos; quitar la regla de memoria → 2.
- Tras fusionar: clippy y 20 suites de `ez-scan`, `ez-ctl`, `ez-herdr` en verde. Worktree
  IR-1 retirado; terminal `ir-1` cerrada. IR-2 lanzado (`ir-2` t1p2, OpenCode + DeepSeek).

### HD-10 + HD-10b — validado y fusionado (9b183a2)
- HD-10: `list_lenient`, `validation` toda en `null` = `None`, aviso único por ilegible,
  también `refresh_activity` tolerante. Devuelto una vez (HD-10b): `ez tasks --json` cambiaba
  de array a objeto cuando había ilegibles (fallo de mi brief); ahora siempre array y los
  ilegibles a stderr.
- Corrida del elefante: fmt, clippy, 16 suites de `ez-herdr` + `ez-ctl` en verde; mutación de
  compatibilidad (`if false`) → falla `a_validation_object_with_every_field_null_is_no_validation`;
  `ez tasks --parent t6h` real (solo lectura) lista `ir-1` y `nv-1` con código 0.
- Tras fusionar: clippy y 20 suites de `ez-herdr`, `ez-ctl`, `ez-scan` en verde. Worktree
  retirado; terminal `hd-10` cerrada.

### HD-0e (supervisor) — revisión 1: devuelto (HD-0f)
- Corrida del elefante sobre una copia (3d22f61): `probar.sh` 15 casos en verde.
  Mutaciones: ignorar `working` → caso 15 falla; pantalla entera → caso 14; volver a contar
  «quota» → caso 12; **quitar la ventana de pantalla quieta → nada falla** (falta prueba).
- Fallos nuevos: escribe en `…\EzAgent\subagents` (el producto lee `subagentes`);
  `validation` con campos `null` (forma no canónica); Python en cp1252 por tubería (un
  resumen con «→» sale vacío, comprobado); «línea con texto» distinta de NV-1c.
- HD-0f: carpeta y forma canónicas, UTF-8, mismo criterio de líneas, casos 16 (pantalla que
  cambia), 17 (captura real), 18 (acentos) y `ez tasks` del producto leyendo los registros
  del supervisor. Sin compilar Rust (usa el `ez.exe` ya compilado).

### DX-3 lanzado (`dx-3` t1pw, OpenCode + DeepSeek)
- Adenda antes de lanzar: archivos (`ez-stt`, `scripts/stop-voice.ps1`), excepción de
  `unsafe` solo en `server/job.rs` para el Job Object, humo con `fake-whisper` y procesos de
  prueba por PID (el `whisper-server` de la persona no se toca).

### DX-1d — validado y fusionado (con DX-1b de nuevo)
- Del mcode al relevo de OpenCode + DeepSeek. Entrega: hook de pánicos síncrono al registro,
  `catch_panic` en WndProc de las ventanas layered, gancho de rueda, bandeja y DirectSound;
  la vista del chat se recupera de un pánico de maquetación; `place_caret` ya no indexa una
  lista vacía (clic en el campo antes del primer cuadro: candidato fuerte al disparador);
  `composite_caret` recorta contra la capa de contenido.
- Símbolos del exe caído (recompilado de 7ce0bbb): `0xbe3f2e` = `panic_with_hook` → pánico
  que cruzó un `extern "system"`. **El disparador exacto no se reprodujo** (35 mensajes
  sintéticos, tema y tamaño de la persona, secuencia panel → chat con llamada).
- Corrida del elefante (release): 29 suites de `ez-core` + `ez-hud` en verde;
  `panic_boundary` → «VIVO»; humo `chat` y `real` → `chat_open=true alive_secs=10`.
  Mutaciones: quitar la guarda de `place_caret` → falla su prueba; quitar `catch_unwind` en
  `paint_content` → falla `a_panicking_layout_keeps_the_view…`; quitar `catch_panic` en
  WndProc → `panic in a function that cannot unwind … aborting`.
- Integración: `git revert 82b25b2` (vuelve DX-1b) + fusión de `work/DX-1d`.
- El humo usa la `VoiceApp` real con ventanas nativas y micrófono silencioso (no el exe con
  micrófono, para no tocar el audio de la persona ni arrancar otro whisper-server). Falta la
  prueba en la app de la persona: recompilar y reiniciar **solo con su permiso**.

### App de la persona recompilada (19-nov 17:26) — con su permiso
- La app de las 13:54 ya no corría: la cerró la persona (lo confirmó); sin eventos de caída
  después de las 13:52. Su `whisper-server` 18960 quedó huérfano: cerrado (+ su powershell).
- `eco/main` e15eb05 (DX-1b + DX-1d + DX-2 + NV-1 + IR-1 + HD-10): `cargo build --release`
  3 min 14 s; abierta con `Start-Process` (PID 19576), viva a los 12 s, whisper nuevo 1556.
- Verificación de `eco/main` fusionado: 47 suites de `ez-core`, `ez-hud`, `ez-app`, `ez-chat`
  en verde (saltando la inestable conocida `a_dirty_round…`, que pasó 3/3 aislada); clippy
  sin avisos propios (solo `vendor/freetype-sys`, ya existentes).
- Pendiente: que la persona abra el chat (panel → chat) y confirme.

### DX-3 — revisión 1: devuelto (DX-3b)
- Bien: Job Object con `KILL_ON_JOB_CLOSE`, `unsafe` solo en `server/job.rs`, estado propio
  con nombre de imagen comprobado antes de matar, reutiliza/reemplaza, humo release con
  `fake-whisper` y los procesos de la persona intactos.
- Falta: el guion del humo no se comprometió (no es repetible) y el lanzamiento directo
  duplica `start-whisper.ps1` sin comprobar que da el mismo servidor que hoy (exe, modelo
  large-v3-turbo-q8_0, `-l es -t 6 --vad --vad-model silero`). DX-3b: guion en
  `scripts/tests/humo-whisper.ps1`, ejemplo `whisper_plan` de solo lectura y prueba de
  correspondencia.

### IR-2b — validado y fusionado (1064c92)
- OpenCode v2.3.0 guarda en `session_message` (la tabla `message` es del 24-oct): ahora lee
  la más reciente en solo lectura viendo el WAL (`mode=ro` por URI), con `json_extract` de
  campos numéricos y de modelo (nunca el texto). Avisos por plan (Free → `*-free`; Go →
  `opencode-go/*`); el agente solo `agotado` si todos sus modelos usables lo están.
- Corrida del elefante: 14 suites de `ez-scan` + `ez-ctl`; `ez usage --json` real →
  OpenCode Go `opencode-go/deepseek-v4.1-flash` 202 M tokens hoy, no agotado. Mutación: el
  filtro Free que cubre todos los modelos → falla `un_aviso_free_no_agota_el_plan_go`.
- Pendiente detectado: mcode sale `desconocido` aunque sus créditos se agotaron (no deja
  porcentaje en disco). IR-3 añade la frase «Check /usage or switch provider» al vigilante.
- IR-3 lanzado (`ir-3` t1sk): recomendación, `ez delegate` sin `--agent`, guarda de
  recursos y avisos de memoria.

### DX-3b — pruebas y plan validados; humo pendiente hasta que la persona termine su prueba
- `whisper_plan -- 53584` coincide **exactamente** con la línea de comandos del servidor
  real de la persona (PID 1556). 12 suites de `ez-stt` + `ez-fakes` en verde.
- Incumplimiento anotado: el subagente hizo `grep -nE "WHISPER_(URL|ENGINE)" .env` (regla:
  nunca leer `.env`). Comprobado: en su worktree **no existe** `.env` (ignorado por git), así
  que no leyó nada. Se le recordó la regla.
- Falta: correr `scripts/tests/humo-whisper.ps1` y la mutación sin `KILL_ON_JOB_CLOSE`. Se
  aplaza porque abre otra instancia de la app de voz con micrófono y la persona está
  haciendo una prueba con EzAgent. Terminal `dx-3` cerrada; el worktree se conserva.

### IR-3 + IR-3b + IR-3c — validado y fusionado (c76b3f2)
- IR-3: `modelos.toml` de fábrica editable, `ez recommend` con razones, `ez delegate` sin
  `--agent`, guarda con código 4 y `--force` (si la medición falla, no bloquea y avisa),
  aviso de memoria por episodio con `MemoryProbe` (sin dependencia circular), frase de
  créditos de mcode. Devuelto dos veces por agotamientos: IR-3b (por modelo: si no, el
  gratuito agotaría a todo OpenCode) e IR-3c (sin hora también vencen: si no, mcode quedaría
  agotado para siempre).
- Corrida del elefante: 21 suites de `ez-scan` + `ez-ctl` + `ez-herdr`; `ez recommend` real:
  developer → OpenCode · deepseek-v4.1-flash · high; qa_validator y planner →
  deepseek-v4-pro · high. Mutaciones: sin regla de familia → 2 fallos; guarda que no mide →
  2; agotamiento sin modelo → 5.
- Nota: el «último developer» del padre t6h es `ir-1` (modelo gratuito, familia desconocida)
  porque los subagentes lanzados con `hc run` a mano no entran en el registro; por eso
  qa_validator no cambió de familia. Se arregla al delegar con `ez delegate`.
- NV-2 lanzado (`nv-2` t1x6).

### DX-1c — revisión 1 (relevo de mcode a OpenCode)
- Entrega: alto medido sin pintar, solo se maqueta el viewport ± una pantalla; humo `big`
  (320 mensajes, rueda arriba/abajo) además de `chat` y `real`, sin `PÁNICO`, pico 147 MB.
  Primer render: 10 → 402 ms (antes 711), 100 → 912 ms (antes 5 974), 1 000 → 6 928 ms
  (antes 63 983). **No cumple** el criterio «mismo orden de magnitud con 10 y 1 000»: medir
  sigue siendo lineal (~6,9 ms por mensaje rico). Se acepta como mejora y queda **DX-1e**
  (alto estimado fuera de pantalla → primer cuadro constante).
- Falsa alarma investigada: 4 pruebas de medidas de texto de `ez-hud` fallaban solo en el
  worktree DX-1c (también con el código de `eco/main`): caché vieja de `freetype-sys` en su
  `target`. `cargo clean --release -p freetype-sys` → pasan. No es un fallo del código.
- Mutación: maquetar todo el historial → fallan las 3 de `chat_virtualization`.
- Humo visual aplazado hasta que la persona termine su prueba técnica (abre ventanas).

### HD-0f — validado; supervisor fusionado (HD-0d + HD-0e + HD-0f)
- Corrida del elefante sobre una copia (1416351): `probar.sh` en verde, con los casos 16
  (pantalla que cambia), 17 (captura real), 18 (acentos y «→») y `ez tasks` del producto
  leyendo los registros del supervisor (código 0, nombres, sin «registro ilegible»).
- Mutación que antes sobrevivía (sin ventana de pantalla quieta) → falla el caso 16.
- El supervisor queda **utilizable** de nuevo. Worktree HD-0d retirado; terminal cerrada.

# EzAgent: espacio de trabajo con elefante y peces dorados (diseño v3)

Estado: **aprobado por el usuario el 2026-11-18** («sí, hagamos eso… crea el entorno
de terminales y hub integrado… delega sub agentes en paralelo con opencode»).

Sustituye la tabla «Adaptación a EzAgent» (sección 2) de
`modo-autonomo-elefante-pez-dorado.md`: **el elefante ya no es EzAgent**. El resto de ese
diseño (metodología, hallazgos, reglas de parada, lista de denegación) sigue vigente
salvo donde este documento lo cambie.

## 1. Qué es

EzAgent pasa a ser un **espacio de trabajo para el ciclo de desarrollo con agentes**:
una sola ventana con espacios. En cada espacio, un **elefante** (el agente de código que
elige el usuario: Claude Code, OpenCode, Codex…) recibe los pedidos y delega en **peces
dorados** (agentes en sesiones nuevas) que se ven trabajar en subterminales.

EzAgent es el **árbitro**: hace cumplir la metodología en código, porque un elefante
puede saltársela si solo se le pide por instrucciones («las skills son instrucciones; los
guardrails son reglas que no se pueden saltar»). La voz es una forma de entrada (nota de
voz y llamada), no el producto.

Principios: **rápido** (núcleo en Rust; la vista es HTML/JS sin paso de compilación),
**sencillo** (una ventana, pocas piezas), **ágil** (cada pieza se prueba sola con dobles).

## 2. Papeles

| Papel | id | Qué hace | Contexto |
|---|---|---|---|
| Persona | — | Aprueba encargo, criterios y equipo; responde las pausas | — |
| Elefante | `elephant` | Conversa con la persona, escribe artefactos, delega con `ez`, decide sobre los hallazgos. **No escribe código del proyecto ni califica trabajo** | Sesión larga |
| Planificador | `planner` | Escribe `plan.md` desde el encargo y los criterios | Pez nuevo |
| Revisor del plan | `plan_reviewer` | Revisa el plan (comprensión, regla de cero decisiones). Opcional | Pez nuevo en cada ronda |
| Desarrollador | `developer` | Implementa | Pez; puede recibir correcciones (`ez send`) dentro de la misma tarea |
| QA | `qa` | Escribe y corre pruebas contra los criterios | Pez nuevo en cada ronda |
| Validador de QA | `qa_validator` | Comprueba que las pruebas del QA sean exigentes: mete fallas (mutaciones) en una copia y verifica que las pruebas las detecten | Pez nuevo en cada ronda |

Los papeles verificadores (`plan_reviewer`, `qa`, `qa_validator`) nunca reciben `ez send`
y entregan un solo informe.

## 3. Arquitectura

```
ezagent-hub.exe  (Tauri 2; un solo proceso)
 ├─ vista (WebView2): HTML + CSS + JS sin compilar, xterm.js 6 incluido en el repo
 ├─ ez-hub      ventana, espacios, vistas de terminal (PTY que ejecuta `hc attach <t>`)
 ├─ ez-arbiter  tareas, rondas, reglas, verificación, git, estado en disco
 ├─ ez-ctl      servidor HTTP 127.0.0.1 con token + binario `ez` (elefante y peces)
 ├─ ez-team     perfiles de agentes, suscripciones, inventario, equipo, textos de los peces
 └─ ez-term     cliente JSON-RPC de herdr-core (`hc connect`)
herdr-core (hc.exe, Apache-2.0): terminales persistentes y estado de cada agente
```

Flujo: la persona escribe o dicta en la barra del espacio → el texto llega a la terminal
del elefante → el elefante usa `ez` (HTTP local) → el árbitro valida y lanza peces con
`ez-term` → la vista muestra cada pez en vivo (`hc attach`) → los peces informan con
`ez report` → el árbitro verifica con comandos propios y decide si la ronda es limpia →
la vista muestra la traza y las tarjetas de aprobación.

`hc` se usa como motor porque resuelve lo que más costó en el modo enjambre: terminales
que sobreviven, estado del agente (`idle`, `working`, `done`, `blocked`, `error`) y
entrega fiable de mensajes. Protocolo observado en `docs/herdr/hc-protocolo.md`; API
completa en `docs/herdr/hc-schema-0.1.21.json`.

## 4. Archivos

Datos de la app, `%APPDATA%\EzAgent\hub\` (los agentes no escriben aquí):
- `espacios.json`: espacios (id, nombre, carpeta, elefante).
- `agentes.toml`: perfiles de agentes (se crea con valores por omisión).
- `suscripciones.toml`: suscripciones que declara la persona.
- `tareas\<tarea>\estado.json`: estado del árbitro (escritura atómica: temporal + renombrar).
- `tareas\<tarea>\bitacora.jsonl`: un evento por línea.
- `tareas\<tarea>\criterios.congelados.toml`: copia de los criterios aprobados.

En el proyecto, `<carpeta>\.ez\tareas\<tarea>\` (lo leen y escriben los agentes):
- `encargo.md`, `criterios.toml`, `plan.md`, `equipo.toml` (los escribe el elefante;
  `plan.md` el planificador).
- `ronda-<n>\<pez>.encargo.md` (encargo del elefante para ese pez).
- `ronda-<n>\<pez>.instrucciones.md` (instrucciones del papel; las escribe el árbitro).
- `ronda-<n>\<pez>.informe.md` (informe del pez).
- `ronda-<n>\verificacion.json` (resultado de los comandos de los criterios).

## 5. Criterios (`criterios.toml`)

```toml
[validacion]               # opcional; valores por omisión entre paréntesis
mutaciones_minimas = 3     # (3)
rondas_maximas = 5         # (5) punto de control: al llegar, pausa
horas_maximas = 2.0        # (2.0)

[[criterio]]
id = "C1"
descripcion = "Transcribe la muestra en español"
comando = ["python", "transcribir.py", "muestras/es.wav"]
carpeta = "."              # opcional, relativa a la carpeta del espacio (".")
codigo = 0                 # opcional, código de salida esperado (0)
contiene = "hola"          # opcional, texto que debe aparecer en la salida
segundos = 300             # opcional, tope de tiempo (300)
```

Reglas: al menos un criterio; `id` único; `comando` no vacío y en lista (sin shell);
`carpeta` dentro de la carpeta del espacio; ningún comando en la lista de denegación
(`ez_herdr::permission::denied` sobre los argumentos unidos con espacios).

## 6. Agentes, suscripciones, inventario y equipo (`ez-team`)

### `agentes.toml` (valores por omisión que escribe EzAgent si no existe)

```toml
[[agente]]
id = "opencode"
nombre = "OpenCode"
argv = ["opencode", "--model", "{modelo}"]
modelos_cmd = ["opencode", "models"]      # salida: un modelo por línea

[[agente]]
id = "claude"
nombre = "Claude Code"
argv = ["claude", "--model", "{modelo}"]
modelos = ["opus", "sonnet", "haiku"]
familia = "anthropic"

[[agente]]
id = "codex"
nombre = "Codex"
argv = ["codex", "--model", "{modelo}"]
esfuerzo_args = ["-c", "model_reasoning_effort={esfuerzo}"]
modelos = []                              # la persona los completa
familia = "openai"
```

- `{modelo}` y `{esfuerzo}` se sustituyen; si el equipo no da esfuerzo, `esfuerzo_args`
  no se añade. Los argumentos de cada CLI son configurables porque cambian entre versiones.
- **Familia** de un modelo: el campo `familia` del agente; si no lo tiene, se deriva del
  modelo: la parte tras la última `/`, hasta el primer `-` o `.`, sin dígitos finales
  (`opencode-go/deepseek-v4.1-flash` → `deepseek`, `opencode-go/qwen4-max` → `qwen`,
  `opencode-go/gpt-6-luna` → `gpt`). El sufijo de variante `#…` se ignora.

### `suscripciones.toml`

```toml
[[suscripcion]]
nombre = "OpenCode Go"
agentes = ["opencode"]
```

### Inventario
Un agente está **disponible** si `argv[0]` existe (ruta absoluta o en `PATH` con `PATHEXT`)
y alguna suscripción lo nombra. Si no hay suscripciones declaradas, cuentan todos los
detectados. Modelos: salida de `modelos_cmd` (tope 20 s, se guarda en memoria del proceso)
o la lista `modelos`.

### `equipo.toml` (lo escribe el elefante; se propone con `ez team propose`)

```toml
[[papel]]
papel = "developer"
agente = "opencode"
modelo = "opencode-go/deepseek-v4.1-flash"
esfuerzo = "max"     # opcional
cantidad = 1         # peces simultáneos; solo developer admite más de 1
```

Validación (errores bloquean; avisos se muestran en la tarjeta):
- Error si falta `planner`, `developer`, `qa` o `qa_validator` (`plan_reviewer` es opcional).
- Error si el agente no está disponible, o si el modelo no está en su lista (cuando la
  lista se conoce y no está vacía).
- Error si `qa` o `qa_validator` usan el mismo modelo que `developer`.
- Aviso si `qa` o `qa_validator` son de la misma familia que `developer` y el inventario
  tiene otra familia disponible.
- Aviso si `qa_validator` usa el mismo modelo que `qa`.

La tarjeta la arma el código: «Equipo propuesto para «{tarea}»:» y una línea por papel
`- {Papel}: {agente} · {modelo}[ · esfuerzo {esfuerzo}][ · ×{cantidad}]`, luego los avisos.

### Textos de los peces
Lección del enjambre: un texto largo pegado al abrir un agente se pierde o queda sin
enviar. Por eso el **primer mensaje es una sola línea**:
`Eres un pez dorado de EzAgent con el papel {papel}. Lee {instrucciones} y síguelo al pie de la letra.`
Las instrucciones completas van al archivo `<pez>.instrucciones.md`, generado desde las
plantillas de `rust/crates/ez-team/src/roles/<papel>.md` (escritas por el elefante).

## 7. Comando `ez` y protocolo (`ez-ctl`)

Variables de entorno que pone EzAgent en cada terminal: `EZ_URL`
(`http://127.0.0.1:<puerto>`), `EZ_TOKEN`, `EZ_ROLE` (`elephant` o `fish`) y, en los
peces, `EZ_FISH`. `PATH` empieza por la carpeta de `ez.exe`.

| Comando | Quién | `command` en el protocolo |
|---|---|---|
| `ez --skill` | cualquiera (local, sin servidor) | — imprime `skill.md` |
| `ez status` | ambos | `status` |
| `ez inventory [--json]` | elefante | `inventory` |
| `ez task new --title <t> --brief <md> --criteria <toml>` | elefante | `task.new` |
| `ez team propose --file <toml>` | elefante | `team.propose` |
| `ez round start` | elefante | `round.start` |
| `ez fish <papel> --brief <md>` | elefante | `fish.new` |
| `ez send <pez> --file <md>` | elefante | `fish.send` |
| `ez verify` | elefante | `verify` |
| `ez round close` | elefante | `round.close` |
| `ez close` | elefante | `task.close` |
| `ez pause --reason <texto>` | elefante | `pause` |
| `ez wait [--timeout <s>]` | elefante | `wait` (por omisión 170 s) |
| `ez report --verdict pass\|fail --file <md> [--blocking N] [--major N] [--minor N] [--mutations-tried N] [--mutations-caught N]` | pez | `report` |

- HTTP: `POST {EZ_URL}/v1/command`, cabecera `Authorization: Bearer <token>`, cuerpo
  `{"command": "...", "args": {...}}`. Respuesta `{"ok": bool, "message": "...", "data": {...}}`.
  `message` es el texto que imprime `ez` (lo genera el árbitro, en español).
- Las rutas de archivo se convierten a absolutas en `ez` antes de enviarlas.
- Códigos de salida de `ez`: 0 bien; 1 rechazado por una regla; 2 uso incorrecto;
  3 no se pudo conectar.
- `ez wait` es una espera larga: el servidor retiene la petición hasta que hay un evento
  para el elefante o vence el tope; entonces responde `ok: true` y `data.event: null`
  con el mensaje «Sin novedades; vuelve a ejecutar `ez wait`.».
- El servidor solo escucha en `127.0.0.1`, puerto elegido por el sistema, y rechaza tokens
  desconocidos (401). El anfitrión registra cada token con su `Caller`.

## 8. Árbitro (`ez-arbiter`)

### Estados de la tarea
`pending_approval` → (persona aprueba) `approved` | (persona rechaza) `rejected`.
`approved`/`between_rounds` → `round.start` → `in_round` → `round.close` → `between_rounds`.
Final: `done` (con `task.close`), `stopped` (persona). `paused` guarda el estado previo y
la persona lo reanuda. El equipo tiene su propia aprobación (`team_approved`).
Una tarea activa por espacio.

### Reglas (cada rechazo lleva un texto generado por el código)

- **R1 Quién puede qué.** Los comandos del elefante solo con un token de elefante del
  mismo espacio; `report` solo con un token de pez, y solo para sí mismo.
- **R2 Tarea nueva.** El encargo y los criterios existen y están dentro de la carpeta del
  espacio; los criterios cumplen la sección 5; la carpeta es un repositorio git
  (`git rev-parse --is-inside-work-tree`), si no: «La carpeta del espacio no es un
  repositorio git: ejecuta `git init` primero.». Copia ambos archivos a
  `.ez/tareas/<tarea>/`. Queda en `pending_approval` y se emite la tarjeta.
- **R3 Criterios congelados.** Al aprobar, se guarda la copia y su sha256. En
  `round.start`, `verify` y `round.close` se compara: si cambió, la tarea pasa a `paused`
  con motivo `criterios_modificados`.
- **R4 Empezar ronda.** Exige tarea aprobada, equipo aprobado, sin ronda abierta y sin
  pausa. Guarda una foto de git (HEAD y `git status --porcelain`).
- **R5 Pez nuevo.** El papel está en el equipo. `qa` solo cuando todos los
  desarrolladores con trabajo en esta ronda ya informaron; `qa_validator` solo cuando
  informó el `qa` de esta ronda. Como máximo un `qa`, un `qa_validator` y un
  `plan_reviewer` por ronda. Desarrolladores vivos ≤ `cantidad`. El árbitro escribe
  `<pez>.instrucciones.md`, genera el token y pide el lanzamiento al `FishLauncher`.
- **R6 Corrección.** `ez send` solo a `planner` o `developer` de la misma tarea, con la
  terminal viva y con una ronda abierta; marca trabajo pendiente del pez en esta ronda.
- **R7 Informe.** El pez tiene trabajo pendiente en esta ronda (los verificadores, una
  sola vez). `qa_validator` debe dar `--mutations-tried` y `--mutations-caught`. El
  archivo del informe está dentro de la carpeta del espacio. Al recibirlo se toma la foto
  de git y se atribuyen al papel los archivos que cambiaron desde que empezó su trabajo.
- **R8 Verificar.** Cada criterio se ejecuta con `std::process::Command` (sin shell),
  con su carpeta y tope; aprueba si el código coincide y, si hay `contiene`, el texto
  aparece en la salida. Se guardan código, duración y los últimos 4000 caracteres de
  salida en la ronda y en `verificacion.json`.
- **R9 Cerrar ronda.** Exige los informes de `qa` y `qa_validator` de la ronda (si no,
  el rechazo dice cuáles faltan). Ejecuta R8 y R3. **Ronda limpia** = todos los criterios
  pasan ∧ `qa` dice `pass` con 0 bloqueantes y 0 mayores ∧ `qa_validator` dice `pass` con
  `mutaciones probadas ≥ mutaciones_minimas` y `detectadas = probadas` ∧ ningún archivo
  protegido tocado por `developer` en esta ronda. **Protegidos**: archivos atribuidos a
  `qa` en rondas anteriores. Limpia → `rondas_limpias_seguidas += 1`; si no → 0.
  Después: si `ronda ≥ rondas_maximas` → `paused` (`tope_rondas`); si los criterios que
  fallan y sus salidas son iguales a los de la ronda anterior → `paused` (`mismo_error`);
  si pasaron más de `horas_maximas` desde la aprobación → `paused` (`tope_tiempo`).
- **R10 Cerrar tarea.** Exige `rondas_limpias_seguidas ≥ 2`; si no: «Todavía no: hacen
  falta dos rondas limpias seguidas (llevas {n}).».
- **R11 Elefante fuera de ronda.** En `round.start` se compara git con la foto del último
  `round.close`: si cambiaron archivos fuera de `.ez/`, la tarea pasa a `paused`
  (`cambios_fuera_de_ronda`). La persona decide al reanudar.
- **R12 Traza.** Toda acción y todo rechazo se agregan a `bitacora.jsonl` y se emiten a
  los suscriptores (la vista) como `ArbiterEvent`.

Modelo de amenaza: las reglas evitan atajos y errores de agentes cooperativos; no
pretenden contener a un agente hostil con acceso a la máquina.

### Eventos para el elefante (`ez wait`)
`task_approved`, `task_rejected`, `team_approved`, `team_rejected`, `fish_reported`,
`fish_blocked` (con `kind` de hc), `fish_exited`, `paused`, `resumed`.

## 9. Cliente de herdr-core (`ez-term`)

- Lanza `hc connect` y habla JSON-RPC 2.0, un mensaje por línea. Primer mensaje:
  `hello {"generation": 1}`.
- **Una conexión para peticiones y otra exclusiva para eventos**: en una conexión con
  `events.subscribe` activo, hc deja de responder otras peticiones (observado).
- Métodos que se envuelven: `terminal.run` (con `argv`, `cwd`, `name`, `labels`, `env`,
  `no_parent: true`, `no_notify: true`), `terminal.list`, `terminal.get`,
  `terminal.read`, `terminal.send`, `terminal.keys`, `terminal.interrupt`,
  `terminal.kill`, `terminal.rm`, `terminal.wait`, `events.subscribe`.
- Etiquetas que pone EzAgent: `ez=1`, `ez_space`, `ez_task`, `ez_role`, `ez_fish`.
- Nombre en hc de un pez: `ez-{últimos 4 de la tarea}-{pez}` (≤ 32, `a-z 0-9 - _ .`).
- Ids de pez: `plan-r{n}`, `revplan-r{n}`, `dev-r{n}-{k}`, `qa-r{n}`, `valqa-r{n}`.

## 10. Ventana (`ez-hub`)

```
┌ Espacios ┐┌──────────── Elefante · Claude Code ────────────┐┌ Peces │ Traza ┐
│ EzAgent  ││ terminal en vivo del elefante (xterm.js)       ││ dev-r1-1 ▸ working │
│ whisper  ││                                                ││ [terminal]         │
│ + Nuevo  ││                                                ││ qa-r1   ▸ done     │
│          │├────────────────────────────────────────────────┤│ [terminal]         │
│          ││ [Escribe o dicta…]      [nota] [llamar] [Enviar]││                    │
└──────────┘└────────────────────────────────────────────────┘└────────────────────┘
```

- Tauri `=2.12.5`, `tauri-build =2.7.4`, `portable-pty =0.9.0`. Vista en
  `rust/crates/ez-hub/ui/` (HTML, CSS, módulos JS) con `@xterm/xterm 6.0.0` y
  `@xterm/addon-fit 0.11.0` copiados en `ui/vendor/` con su licencia. Sin npm ni empaquetador.
- Cada terminal visible es un PTY de `portable-pty` que ejecuta `hc attach <t>` (o, en
  modo demostración, cualquier programa) y envía sus bytes a xterm.js.
- Barra de entrada: Enter envía, Mayús+Enter salta de línea. El texto va al elefante
  (`terminal.send`). Nota de voz: graba, transcribe con whisper y deja el texto en la barra
  para editarlo. Llamada: conversación por voz con la esfera (fase 3).
- Tarjetas de aprobación (encargo y criterios, equipo, pausa) en la columna de traza con
  «Aprobar» y «Rechazar» (rechazar pide un comentario que se entrega al elefante).
- Tema oscuro por omisión con tokens CSS; textos de la vista en `ui/js/texts.js`, español
  neutro.
- Seguridad: CSP solo con `'self'`, sin contenido remoto, capacidades mínimas.

## 11. Fases

- **Ola 1 (en paralelo):** ECO-1 `ez-term`, ECO-2 `ez-arbiter`, ECO-3 `ez-ctl`,
  ECO-4 `ez-team`, ECO-5 `ez-hub` (ventana con modo demostración).
- **Ola 2:** ECO-6 integración (crear espacio lanza al elefante por hc; los peces
  aparecen solos; tarjetas; traza; `ez wait`), ECO-7 nota de voz, ECO-8 prueba de punta
  a punta con agentes falsos.
- **Ola 3:** llamada con la esfera, aprobación automática de permisos
  (`ez_herdr::permission`), empaquetado con `hc.exe`, retiro del HUD flotante (con
  confirmación de la persona).

La app de voz actual (`ezagent-voice`) no cambia durante las olas 1 y 2.

## 12. Criterios de aceptación de la ola 1 (los verifica el elefante)

1. Desde `rust/`: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`
   y `cargo test --workspace` en verde.
2. `ez-term`: el ejemplo `hc_smoke` contra el hc real crea, lista, lee y borra una
   terminal y recibe sus eventos.
3. `ez-arbiter`: pruebas de cada regla R1–R12 con lanzador y reloj falsos y git real en
   carpetas temporales.
4. `ez-ctl`: ida y vuelta `ez` ↔ servidor con un `Handler` falso; token inválido → 401 y
   salida 3 o 1 según el caso; `ez --skill` imprime el manual.
5. `ez-team`: inventario con `PATH` falso; validación del equipo (incluye una sola
   suscripción); instrucciones de cada papel renderizadas.
6. `ez-hub`: compila; `ezagent-hub --demo` abre la ventana con dos terminales
   interactivas, traza y una tarjeta; captura revisada por el elefante.

## 13. Decisiones

- Motor de terminales: herdr-core por su API, detrás de `ez-term` (cambiable).
- Vista: Tauri 2 + xterm.js sin empaquetador (lo más documentado para los peces y lo más
  rápido de construir).
- Estado del árbitro en archivos JSON (como el modo autónomo v2), no en SQLite.
- `ez-ctl` no depende de `ez-arbiter`: define el `Handler`; la integración lo une (ola 2).
- Mutaciones: el árbitro exige los números; las ejecuta el `qa_validator`
  (verificarlas en código queda para después).
- Codex sin `--yolo` por omisión (configurable en `agentes.toml`).

## 14. Revisión 1 (pez ECO-REV0): veredictos y cambios

Informe: `rust-migration/reports/ECO-REV0-r1.md` (10 Bloqueantes, 16 Mayores, 6 Menores).
**Donde difiera, manda esta sección.** Se aplica en la ronda de correcciones de la ola 1
(ECO-1b…5b) y en la ola 2.

### 14.1 Identidad y credenciales (B02, B03, M09)
- Id de tarea: `t` + 12 hex de un uuid v4 (`t3f9a0c1d2e4b`); `task.new` lo devuelve en `data.task`.
- Pez: `FishKey { space, task, fish, attempt }` (`attempt` empieza en 1). Es el `Caller::Fish`
  de `ez-ctl` y de `ez-arbiter`, y lo reciben `fish_blocked`, `fish_exited` y `fish_failed`
  (la ventana lo saca de las etiquetas hc `ez_space`, `ez_task`, `ez_fish` y `ez_attempt`).
- Nombre hc: `ez-{8 hex de la tarea}-{pez}`, más `-a{n}` si `attempt > 1`.
- Token: lo genera el árbitro y va en `LaunchRequest.token`; **el lanzador lo registra en
  `Tokens` antes de lanzar el proceso**. El árbitro llama a `FishLauncher::revoke(token)`
  cuando un verificador informa, cuando el pez sale o falla y cuando la tarea termina o se
  detiene. Ningún evento, vista, bitácora ni log lleva tokens.

### 14.2 Lanzamiento (B01, M08, M11, M16, m04)
- Perfil: se añaden `env` (mapa; marcadores `{modelo}`, `{esfuerzo}` y `{variante}` =
  `#<esfuerzo>` o vacío) y `previo` (opcional; hoy solo `"opencode_state"`, el estado
  aislado de `ez_herdr::prelaunch`). `launch::command(profile, seat) -> LaunchCommand { argv, env, previo }`.
- OpenCode por omisión (su TUI rechaza `--model`): `argv = ["opencode", "--standalone"]`,
  `env = { OPENCODE_CONFIG_CONTENT = '{"model":"{modelo}{variante}"}' }`, `previo = "opencode_state"`.
- Claude: `esfuerzo_args = ["--effort", "{esfuerzo}"]`.
- Validación: `agente` y `modelo` no vacíos; si hay `esfuerzo`, no vacío y el perfil debe
  poder aplicarlo (`esfuerzo_args` o `{esfuerzo}`/`{variante}` en `env`); si no, error.
- Transacción del lanzador (ventana): registrar token → `terminal.run` → `terminal.wait`
  hasta que no esté `working` (tope 60 s) → `terminal.send` de la primera línea → si
  `started == Some(false)`, leer la pantalla, esperar 5 s y reenviar una vez. Resultado
  `Delivered`, `Unconfirmed` (`started == None`) o `Failed(motivo)`; con `Failed` el trabajo
  queda `failed` (14.5) y no hay trabajo pendiente.
- Elefante: el espacio guarda `{agente, modelo, esfuerzo}` y se lanza con el mismo
  `launch::command`, `EZ_ROLE=elephant` y la primera línea: «Eres el elefante de este
  espacio de EzAgent. Ejecuta `ez --skill` y sigue ese manual; luego espera el pedido de la persona.»

### 14.3 Fases de la ronda e informes (B04, B05)
- Trabajo: `WorkItem { key, round, kind: launch | correction(k), report_path, state: pending | reported | failed }`.
  Informe en `ronda-<n>/<pez>.informe.md` (lanzamiento) o `ronda-<n>/<pez>.informe-c<k>.md`
  (corrección; la línea de corrección nombra esa ruta). `report` exige exactamente la ruta
  del trabajo pendiente.
- Ronda 1: planner (uno) `pass` → plan_reviewer `pass` (si está en el equipo) → al menos un
  developer → qa → qa_validator. Rondas siguientes: developers (lanzamiento o corrección) →
  qa → qa_validator. Lanzar o corregir un developer con un `qa` ya lanzado en la ronda → rechazo.
- **Ronda solo de verificación** (sin developer): permitida solo si la huella del proyecto
  es igual a la del cierre de la ronda anterior y esa ronda fue limpia.
- `--blocking`, `--major` y `--minor` son obligatorios; `pass` con bloqueantes o mayores > 0
  → rechazo: «El veredicto pass no puede llevar hallazgos bloqueantes o mayores.». El
  informe se copia al almacén privado con su sha256.
- **Ronda limpia (sustituye la de R9):** criterios pasan ∧ **todos** los informes de la ronda
  `pass` con 0 bloqueantes y 0 mayores ∧ mutaciones del validador ≥ mínimo y todas
  detectadas ∧ sin violaciones (14.4) ∧ artefactos congelados intactos (14.6).

### 14.4 Huellas y atribución (B06, B07)
- Huella = HEAD + sha256 del contenido de cada archivo de
  `git ls-files -co --exclude-standard -z` fuera de `.ez/`.
- Se toma al aprobar la tarea, al lanzar o corregir un pez, al recibir cada informe, al
  abrir y cerrar ronda y en `task.close`.
- Los cambios entre la huella al empezar un trabajo y la de su informe se atribuyen a ese
  pez. Los cambios cuando no hay trabajo pendiente se atribuyen al elefante → `paused`
  (`cambios_fuera_de_ronda`); al reanudar, `rondas_limpias_seguidas` vuelve a 0.
- Violaciones (la ronda no es limpia): `planner`, `plan_reviewer` o `qa_validator` cambiaron
  archivos del proyecto; `developer` cambió un archivo atribuido a `qa` (en esta ronda o
  antes). Con developers en paralelo, sus cambios se atribuyen al papel.
- `task.close` repite R3 y R8 y compara la huella con la del cierre de la última ronda
  limpia: si difieren → rechazo.

### 14.5 Concurrencia, recuperación y topes (B08, M01, M02, M03)
- Un ejecutor secuencial por espacio (cola + hilo `ez-arbiter-<espacio>`): cada comando del
  elefante, de un pez o de la persona se procesa entero antes del siguiente. `wait` y
  `snapshot` no pasan por la cola.
- Al arrancar, las tareas no finales pasan a `paused` (`reinicio`), sus trabajos
  pendientes a `failed` y sus tokens dejan de valer.
- Pez que sale, se pierde o queda en `error` antes de informar → trabajo `failed`, token
  revocado y evento `fish_failed` al elefante. Se puede relanzar ese papel en la misma
  ronda con `attempt + 1` (hasta 3 intentos por papel y ronda).
- Tarjeta de pausa: «Reanudar», «Dar más margen» (+2 rondas y +1 h, guardado fuera de los
  criterios) y «Detener». El tiempo cuenta solo fuera de pausa. `task.close` solo desde
  `between_rounds`.

### 14.6 Artefactos congelados y rangos (B10, M12)
- Al aprobar se congelan encargo, criterios y equipo (copia privada + sha256). Al lanzar,
  informar, verificar y cerrar se comparan las copias de `.ez/tareas/<tarea>/`: si
  difieren → `paused` (`artefactos_modificados`); al reanudar se restauran.
- Rangos de `criterios.toml`: `mutaciones_minimas` 1–20; `rondas_maximas` 2–20;
  `horas_maximas` finito, > 0 y ≤ 24; `segundos` 1–3600; `descripcion` no vacía. `contiene`
  se busca en stdout seguido de stderr. La tarjeta muestra `[validacion]`.

### 14.7 Procesos y rutas (M06, M14)
- Al vencer el tope de un criterio: `taskkill /T /F /PID <pid>` (argumentos en lista, sin
  shell); los lectores de salida se abandonan 2 s después.
- Rutas: `canonicalize` de la raíz y del destino (del padre si el archivo no existe) y
  `Path::starts_with` por componentes. Pruebas con `..`, prefijo compartido
  (`proyecto` frente a `proyecto-otro`) y enlace o junction.

### 14.8 Servidor, protocolo y vista (M05, M10, m01, m02, m06)
- `ez-ctl`: hasta 8 hilos para comandos y hasta 32 esperas simultáneas en hilos aparte
  (más → 503 «Demasiadas esperas.»). `stop` despierta las esperas.
- `ArbiterEvent` tipado con `seq` monotónico; `snapshot` incluye `last_seq`. Tarjeta:
  `Card { id, space, task, kind: task | team | pause, title, body }`; acciones `approve` y
  `reject` (task, team), `resume`, `more_time` y `stop` (pause).
- `ez --json` (global) imprime el `Reply` entero; `inventory` devuelve el inventario en `data`.
- `HcClient::send`: tope 75 s.
- La barra envía texto crudo con `terminal.send`; el pegado entre corchetes queda solo
  para PTY directos (modo demostración).

### 14.9 Eventos de hc (M13)
`TermEvent` lleva `seq`; tras `missed` se descartan los eventos con `seq` ≤ el de la foto.
Si la foto no se puede leer: `Missed { snapshot: None }` y el consumidor resincroniza con
`terminal.list`.

### 14.10 Diferido o aclarado
- M04 (churn): diferido a la ola 3 (necesita hallazgos estructurados); hasta entonces solo `mismo_error`.
- M07: ECO-8 incluye una prueba real por CLI (primera línea, modelo, `ez report`, estado).
  Si el sandbox de Codex no deja ejecutar `ez.exe`, decide la persona.
- B09: excepción explícita para `--demo`: `powershell.exe -NoLogo` interactivo, sin `-Command`.
- M15: los peces usan `cargo fmt -p <crate>`; `cargo fmt --all --check` queda para la puerta del elefante.
- m03: ejemplos de familia: los tres de §6 más `hy4-flash-free` → `hy`.
- m05: la copia del validador va en `%TEMP%\ez-valqa-<uuid>`, sin `.git`, `target`,
  `node_modules` ni `.ez`, y se borra al final.

## 15. Revisión 2 (2026-11-18): árbol de agentes y papeles abiertos

Pedido de la persona: «algo como Herdr, con espacios y agentes, y de esos agentes se
despliegan los demás como subcarpetas; se puede abrir cada terminal; no hay una cantidad
específica de agentes ni solo desarrollo y QA: pueden ser infinidad de combinaciones de
peces dorados». Decisiones de la persona: **varios agentes principales por espacio**;
**un pez puede delegar si su papel lo permite**; **papeles libres: biblioteca + propuestas
del elefante**. **Donde difiera, manda esta sección** (sobre §2, §6, §8, §10 y §14).

### 15.1 Árbol y aislamiento
- Espacio (carpeta, repositorio git) → **agentes principales** (elefantes; cada uno con su
  tarea y su equipo) → peces → subpeces. Profundidad máxima bajo el elefante:
  `profundidad_maxima` (1–5, por omisión 3) en `[validacion]` de los criterios.
- Identidad: `ElephantKey { space, elephant }` (id `e` + 8 hex). `Caller::Elephant(ElephantKey)`.
  **Una tarea activa por elefante.** `FishKey` gana `elephant` y `parent: Option<String>`
  (id del pez que lo delegó; `None` si lo delegó el elefante).
- **Cada agente principal trabaja en su propio worktree** (rama `ez/<elefante>`, carpeta
  `<carpeta>.ez-wt\<elefante>`), creado al crear el agente principal. Sus peces y subpeces
  trabajan en ese worktree. Así las huellas (§14.4) de un elefante no ven los cambios de
  otro. Al cerrar su tarea, la tarjeta final ofrece «Integrar en {rama base}» (merge sin
  fast-forward, hecho por el código; si hay conflicto, se avisa y no se toca nada) o
  «Dejar en su rama».
- Etiquetas hc nuevas: `ez_elephant`, `ez_parent`. El árbol de la vista sale de esas
  etiquetas (y de `parent` de hc para terminales abiertas a mano).

### 15.2 Papeles abiertos
- Un papel es un archivo Markdown con cabecera TOML entre líneas `+++`:
  ```
  +++
  nombre = "Auditor de seguridad"
  tipo = "verificador"          # "ejecutor" o "verificador"
  descripcion = "Busca vulnerabilidades en el código cambiado"
  puede_delegar = false         # solo ejecutores
  escribe_pruebas = false       # sus archivos quedan protegidos frente a los ejecutores
  mutaciones = false            # exige --mutations-tried/--mutations-caught
  solo_primera_ronda = false
  +++
  (instrucciones del papel, con los marcadores de §6)
  ```
- Id = nombre del archivo sin `.md`: `[a-z][a-z0-9_]{1,23}`.
- Orígenes, de mayor a menor prioridad: **de la tarea** (`.ez/tareas/<tarea>/papeles/`,
  propuestos por el elefante, congelados al aprobar el equipo), **biblioteca de la persona**
  (`%APPDATA%\EzAgent\hub\papeles\`) e **incluidos** (los cinco de hoy, en
  `rust/crates/ez-team/src/roles/`, con su cabecera).
- Incluidos: `planner` (ejecutor, `solo_primera_ronda`), `plan_reviewer` (verificador,
  `solo_primera_ronda`), `developer` (ejecutor), `qa` (verificador, `escribe_pruebas`),
  `qa_validator` (verificador, `mutaciones`).
- Comando nuevo `ez roles`: lista id, nombre, tipo, descripción y origen.
- La plantilla común `_comun.md` se antepone siempre; si el papel `puede_delegar`, se le
  añade además `_delegar.md` (cómo usar `ez fish`, `ez send` y `ez wait` con sus hijos).

### 15.3 Equipo
```toml
[[papel]]
papel = "developer"       # id de un papel disponible
agente = "opencode"
modelo = "opencode-go/deepseek-v4.1-flash"
esfuerzo = "max"          # opcional
cantidad = 3              # peces simultáneos de este papel (≥ 1)
etapa = 2                 # orden dentro de la ronda (≥ 1)
delega_en = []            # papeles que puede lanzar (solo si el papel puede delegar)
```
- `etapa` por omisión: 1 para ejecutores, 2 para verificadores.
- Validación: al menos un ejecutor y un verificador; cada papel existe; un verificador no
  puede usar el modelo de ningún ejecutor (error) y se avisa si comparte familia habiendo
  otra disponible; `delega_en` solo en papeles que pueden delegar y con papeles del equipo.
- La tarjeta del equipo muestra, además, los **papeles nuevos de la tarea** con su tipo y
  descripción, para que la persona los apruebe junto con el equipo.

### 15.4 Rondas (sustituye el orden fijo de §14.3)
- Un trabajo de etapa *k* solo empieza cuando todos los trabajos de etapas menores de esa
  ronda informaron. Los papeles `solo_primera_ronda` solo corren en la ronda 1.
- Correcciones (`ez send`) solo a ejecutores, y solo si aún no empezó una etapa posterior
  a la suya en esa ronda.
- **Ronda solo de verificación** (sin trabajos de ejecutores): permitida si la huella es
  igual a la del cierre de la última ronda limpia.
- **Ronda limpia:** criterios pasan ∧ todos los informes de la ronda `pass` con 0
  bloqueantes y 0 mayores ∧ al menos un verificador informó ∧ cada papel con `mutaciones`
  cumple el mínimo con todas detectadas ∧ sin violaciones ∧ congelado intacto.
- **Violaciones:** un verificador sin `escribe_pruebas` cambió archivos del proyecto; un
  ejecutor cambió archivos atribuidos a un papel con `escribe_pruebas`.

### 15.5 Delegación
- Un pez cuyo papel `puede_delegar` usa `ez fish <papel> --brief`, `ez send` y `ez wait`
  sobre **sus propios hijos**, solo con papeles de su `delega_en`, dentro de la `cantidad`
  del equipo y sin pasar `profundidad_maxima`. Sus hijos pertenecen a su misma ronda y etapa.
- Un pez que tiene hijos solo puede informar cuando todos ellos informaron.
- Los verificadores no delegan. `ez wait` de un pez devuelve las novedades de sus hijos.

### 15.6 Ventana (sustituye §10)
```
┌ Espacios ──────────────┐┌ [elefante A] [dev-r1-1] [qa-r1] ─────────┐┌ Traza │ Tarjetas ┐
│▾ whisper-app           ││ terminal de la pestaña activa (hc attach) ││ Ronda 1           │
│  ▾ ● Elefante A · Opus ││                                           ││ dev-r1-1 en marcha│
│      ● investigador-r1 ││                                           ││ …                 │
│    ▾ ● arquitecto-r1   ││                                           ││ [Equipo propuesto]│
│        ● dev-api       │├───────────────────────────────────────────┤│                   │
│      ○ qa-r1           ││ [Escribe o dicta…]  [nota] [llamar] [Enviar]│                   │
│  ▸ ● Elefante B · Codex│└───────────────────────────────────────────┘└───────────────────┘
│ + Agente principal     │
│ + Espacio              │
└────────────────────────┘
```
- **Izquierda:** árbol plegable. Punto de estado por nodo (`working`, `idle`, `done`,
  `blocked`, `error`, falló). «+ Agente principal» pide agente, modelo y esfuerzo.
- **Centro:** pestañas de terminales; clic en un nodo abre o enfoca su terminal
  (`hc attach`); se puede dividir en dos.
- **Barra:** escribe al agente de la pestaña activa (principal o pez); nota de voz igual.
- **Derecha (plegable):** traza y tarjetas de la tarea del agente principal seleccionado.

### 15.7 Contrato nuevo de `ez-team` (lo aplica ECO-R1)
```rust
pub struct RoleId(pub String);                 // validado: [a-z][a-z0-9_]{1,23}
pub enum RoleKind { Executor, Verifier }
pub enum RoleOrigin { Task, Library, Builtin }
pub struct RoleDef {
    pub id: RoleId, pub name: String, pub kind: RoleKind, pub description: String,
    pub can_delegate: bool, pub writes_tests: bool, pub mutations: bool,
    pub first_round_only: bool, pub body: String, pub origin: RoleOrigin,
}
pub struct Seat {
    pub role: RoleId, pub agent: String, pub model: String, pub effort: Option<String>,
    pub count: u32, pub stage: u32, pub delegates_to: Vec<RoleId>,
}
// InstructionContext: `role: RoleId` y `role_name: String` en lugar de `role: Role`;
// gana `parent: Option<String>` y `children_roles: Vec<RoleId>`.
```
El enum `Role` desaparece; los cinco papeles incluidos quedan como constantes de id.

### 15.8 Plan
1. Termina lo que corre (ECO-2c, ECO-6a), sin cambios.
2. **ECO-R1:** migración mecánica al contrato 15.7 en `ez-team`, `ez-arbiter` y `ez-hub`
   (mismo comportamiento: los cinco incluidos, sus etapas por omisión).
3. En paralelo: **ECO-4c** (papeles abiertos en `ez-team`: cabeceras, orígenes,
   validación 15.3, `_delegar.md`, texto de `ez roles`) y **ECO-2d** (árbitro: elefantes por
   espacio, worktrees, etapas 15.4, delegación 15.5, tarjeta de integración).
4. **ECO-6c:** runtime de la ventana con varios elefantes, worktrees, `ez roles` y
   delegación; **ECO-6b** (rehecho): la ventana de 15.6.
5. **ECO-8:** punta a punta con agentes falsos y hc real.

### 15.9 Nombres (pedido de la persona)
«Elefante» y «pez dorado» son la **referencia** de la metodología, no nombres del producto.
- Todo lo que lee una persona o un agente usa **«agente principal»** y **«subagente»**:
  la vista («Agente principal · {agente}», pestaña «Subagentes», «+ Agente principal»),
  tarjetas, traza, mensajes de `ez`, el manual (`ez --skill`: «Manual del agente
  principal»), las plantillas de papeles («Eres un subagente de EzAgent…») y las primeras
  líneas:
  - subagente: `Eres un subagente de EzAgent con el papel {papel}. Lee {instrucciones} y síguelo al pie de la letra.`
  - principal: `Eres el agente principal de este espacio de EzAgent. Ejecuta ez --skill y sigue ese manual; luego espera el pedido de la persona.`
- Comando `ez fish` → **`ez delegate <papel> --brief <md>`** (protocolo `agent.delegate`);
  `EZ_ROLE` vale `main` o `sub`. Los demás comandos no cambian.
- Los identificadores internos de Rust (`Elephant…`, `Fish…`) y este documento pueden
  seguir usando la referencia; no se muestran.
- Lo aplica ECO-R1 junto con el contrato 15.7 (el elefante entrega el manual y las
  plantillas ya reescritos).

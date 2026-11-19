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

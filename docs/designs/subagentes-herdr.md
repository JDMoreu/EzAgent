# Subagentes en Herdr: hijos de cualquier tipo bajo su agente (2026-11-19)

Contrato del elefante para la ola HD. Rama base: `eco/main`. Manda sobre los briefs
`HD-*` si discrepan.

## 1. Pedido
- Los agentes que lanza un agente aparecen en Herdr **debajo del agente que los lanzó**
  (no como «Shell» de fondo dentro de Claude Code), con su propio nombre, y se pueden
  abrir en pestaña o panel.
- El padre puede tener hijos de **cualquier tipo** del catálogo (OpenCode, mcode/MiniMax,
  Qwen Code, Codex, Claude Code, Pi…) y alternar tipos: el tipo, el modelo y el esfuerzo
  se eligen **en cada delegación** y se validan contra el catálogo de ese tipo.
- El árbol muestra qué hijo es de qué agente y de qué tipo es cada uno.

## 2. Lo que ya existe (no se reimplementa)
- **herdr-core (`hc`)**: cada terminal tiene `parent`, fijado al crearla: por omisión la
  terminal que ejecuta `hc run` (`HC_TERMINAL_ID`), o `--parent <t>`. Los hijos terminan
  con el padre salvo `--outlive-parent`; `--no-notify` evita que hc escriba avisos en el
  padre. `hc run` **no** tiene `--env`: el hijo hereda el entorno del núcleo de hc, no el
  de quien lo lanza (la API `terminal.run` sí acepta `env`).
- **herdr-app 0.1.16**: la lista de agentes dibuja los subagentes en árbol bajo su padre
  (`~/.config/herdr-app/mods/agents/main.js`). Fila = ícono del programa + `name` (si no
  hay nombre, el del programa: por eso «Shell»). `terminal.keep {terminal, place}` da
  pestaña (`tab`) o panel (`right`/`down`, junto al panel **enfocado**) y el hijo sigue
  bajo su padre; `pane.focus {terminal}` enfoca el panel de una terminal;
  `terminal.park` le quita el panel. Al matar la terminal, Herdr cierra su pestaña.
- **Verificado en vivo (19-nov)**: un `hc run --bg` desde Claude Code (t6h) quedó con
  `parent = t6h`, apareció bajo «EzAgent» y `terminal.keep --place tab` le dio pestaña sin
  cambiar el padre. **`terminal.keep` justo después de `hc run` falla con «no running
  terminal "tN"»**: herdr-app tarda en ver la terminal nueva; a los ~2 s funciona. Toda
  ubicación reintenta (0,5 s entre intentos, hasta 15 s).
- **Catálogo**: `ez-core` (`default_agents`, `default_profiles`) + `voice.json` del usuario
  (`agents` reemplaza el comando por tipo; los perfiles se fusionan). `ez-herdr`
  (`AgentDriver::agent_launch`) ya arma argv/entorno por tipo (flags, `OPENCODE_CONFIG_CONTENT`,
  `XDG_STATE_HOME` de OpenCode, esfuerzo de mcode en su `config.yaml`, modelos de Codex y
  mcode leídos de sus archivos) y `finish_launch` espera al programa y pega la tarea.

## 3. Modelo de datos de un hijo
| Dato | Dónde | Valor |
|---|---|---|
| Padre | `parent` de hc | La terminal que delega (`HC_TERMINAL_ID`); nunca cambia |
| Nombre | `name` de hc | `<papel-corto><n>-<tipo>`, p. ej. `dev1-opencode` (regla §6) |
| Tipo | etiqueta `ez.agent` | Clave del catálogo (`opencode`, `qwen`…) |
| Modelo | etiqueta `ez.model` | El elegido (se omite la etiqueta si se usa el de por omisión) |
| Esfuerzo | etiqueta `ez.effort` | El elegido (se omite si no se pidió) |
| Papel | etiqueta `ez.role` | El papel tal como se escribió (`developer`, `qa`…) |
| Padre (copia) | etiqueta `ez.parent` | La misma dirección, para filtrar con `hc ls --label` |
| Enjambre | etiqueta `ez.swarm` | Id del enjambre (solo §8) |

Además se conserva la etiqueta `agent=<tipo>` que ya pone `ez-herdr`.

## 4. Comando
```
ez delegate <papel> --agent <tipo> [--model <modelo>] [--effort <nivel>] --brief <archivo>
                    [--name <nombre>] [--show tree|tab|right|down] [--cwd <carpeta>]
                    [--no-notify] [--outlive-parent] [--json]
```
- **Modo local (Herdr)**: cuando **no** hay `EZ_URL`/`EZ_TOKEN`. Requiere `HC_TERMINAL_ID`
  (si falta: código 3, «Ejecútalo desde la terminal de un agente en Herdr»).
- **Modo árbitro** (hay `EZ_URL` y `EZ_TOKEN`): se comporta como hoy; si llegan `--agent`,
  `--model`, `--effort`, `--name`, `--show`, `--cwd`, `--no-notify` u `--outlive-parent`,
  código 2: «En una tarea del árbitro el tipo sale del equipo aprobado (llega con la
  integración del árbitro).»
- `--brief`: archivo UTF-8 (máx. 200 000 bytes) con el encargo. `--cwd` por omisión: la
  carpeta actual. `--show` por omisión: `tree`. Por omisión hc **sí** avisa al padre
  (`--no-notify` lo apaga) y el hijo **muere** con el padre (`--outlive-parent` lo cambia).
- Salida: una línea legible, p. ej. `Subagente dev1-opencode (t12) lanzado: OpenCode ·
  deepseek-v4.1-flash · high, hijo de t6h, en una pestaña.`; con `--json` el objeto
  `{ok, terminal, name, agent, label, model, effort, role, parent, show, placed, task_sent,
  warnings}` (`placed` = lo que de verdad quedó: `tree`, `tab`, `right`, `down`).
- Códigos: 0 lanzado (con o sin avisos), 1 rechazado (validación, nombre ocupado, fallo de
  hc), 2 uso incorrecto, 3 sin `hc`/sin `HC_TERMINAL_ID`.

## 5. Validación estricta por tipo (antes de crear nada)
1. **Tipo**: se resuelve con los alias del catálogo. Desconocido → «No conozco el agente
   «X». Agentes del catálogo: <claves ordenadas>.» Programa no instalado → ««X» está en el
   catálogo pero no encuentro su programa (<argv[0]>).»
2. **Modelo** (si se pidió):
   - el perfil lista `models` → debe estar (sin distinguir mayúsculas; se acepta con o
     sin el `model_prefix` del perfil, y `proveedor/nombre` si `nombre` está). Si no →
     «<Etiqueta> no tiene el modelo «M». Modelos: <lista>.»
   - perfil con `discover` (mcode, Codex) → se valida contra lo descubierto con la lógica
     que ya existe; si no se pudo descubrir nada → se acepta con aviso «No pude leer los
     modelos de <Etiqueta>; uso «M» sin verificar.»
   - sin lista ni descubrimiento (Pi) → se acepta con aviso «El catálogo de <Etiqueta> no
     lista modelos; uso «M» sin verificar.»
3. **Esfuerzo** (si se pidió): `efforts` del perfil y `model_efforts` del modelo
   (OpenCode) y los niveles de Codex, con los mensajes que ya existen. Tipo sin niveles ni
   forma de fijarlos (Qwen Code) → «<Etiqueta> no tiene niveles de esfuerzo.»
4. Sin modelo → el de por omisión del perfil o del propio programa (sin etiqueta).
Un rechazo **no** crea terminal ni toca configuraciones.

## 6. Nombre
- `--name` válido: `^[a-z0-9][a-z0-9_.-]{0,31}$` y sin forma de dirección
  (`^t[0-9][0-9a-z]*$`, hc los rechaza); si no, código 2. Nombre ocupado: hc responde
  `kind: "name_taken"`.
- Por omisión: `<corto><n>-<tipo>` recortado a 32. `corto`: `planner`→`plan`,
  `plan_reviewer`→`revplan`, `developer`→`dev`, `qa`→`qa`, `qa_validator`→`valqa`; otro
  papel → sus caracteres `[a-z0-9]` en minúsculas, máx. 8. `n` = 1 + número de terminales
  (vivas o terminadas) con `ez.parent=<padre>` y `ez.role=<papel>`. Si hc dice que el
  nombre está ocupado, se prueba `n+1` (hasta 5 veces).

## 7. Primer mensaje (lo escribe el código)
```
Eres un subagente de EzAgent con el papel «{papel}», lanzado por la terminal {padre}.
Trabaja solo en lo que dice tu encargo. No lances otros agentes salvo que tu encargo lo
pida; si lo pide, usa `ez delegate`. Cuando termines o te bloquees, avisa con:
hc send {padre} "<resultado o bloqueo>". Si tu respuesta es larga, escríbela en un archivo
y envía su ruta.

Encargo:
{contenido del archivo --brief}
```
Se entrega con `finish_launch` (espera al programa, fija lo pendiente y pega). Los textos
largos ya se entregan por archivo (`LONG_TEXT_CHARS`).

## 8. Lanzar con padre (ez-herdr) y ubicar
- `RunAgent` gana `parent`, `labels`, `show`, `notify`, `outlive_parent` y `hc_name`. Con
  `parent` el lanzamiento va **siempre** por `hc run --json --bg --parent <p> --name
  <hc_name> --cwd … [--no-notify] [--outlive-parent] --label …  -- <argv>` (con la misma
  envoltura de entorno que el lanzamiento en segundo plano), **nunca** por `tab.new` +
  tecleo (ese camino no puede poner padre). Sin `parent`, todo sigue como hoy.
- Ubicación tras crear la terminal (antes de esperar al programa):
  `tree` nada; `tab` → `terminal.keep --place tab`; `right`/`down` → `pane.focus
  --terminal <padre>` y `terminal.keep --place right|down`; si el padre no tiene panel →
  pestaña con aviso «El agente padre no tiene panel; abrí el subagente en una pestaña.».
  Reintentos según §2. Si Herdr no está abierto o no responde → queda en el árbol con
  aviso. **La ubicación nunca hace fallar la delegación.**
- Tras ubicar se relee `hc get` y el resultado informa el `parent` real.

## 9. Enjambre y equipo autónomo
- Raíz = el primer miembro del enjambre (orden de trabajo) que ya tiene dirección.
- Los demás miembros se lanzan con `parent = raíz`, `outlive_parent = true` (si la raíz se
  cierra, la recuperación del enjambre decide; no se cae todo en cascada), `notify =
  false` (coordina EzAgent; nada de avisos de hc en la entrada de la raíz), `show = tab`
  (siguen visibles como hoy), `hc_name` = slug del nombre del miembro, etiquetas §3 más
  `ez.swarm`.
- Sin raíz con dirección (aún no abre o falló) → como hoy, sin padre (y se registra).
- La reapertura o el reemplazo de un miembro siguen la misma regla. La raíz se lanza como
  hoy.

## 10. ez-term
`RunSpec` gana `parent: Option<String>` y `outlive_parent: bool` (por omisión `None` y
`false`; `no_parent`/`no_notify` siguen por omisión en `true`). Con `parent = Some(p)` se
envía `"parent": p` y `"no_parent": false`. Sin cambios de comportamiento para quien no
los use.

## 11. Aceptación de punta a punta (sin cuota)
Con hc y herdr-app reales y un **agente falso** por tipo (`fake-ws-agent --echo`, que
reporta su estado con OSC 7501 y guarda argv, entorno y lo que recibe):
1. Un padre falso en su propio espacio de Herdr delega en los seis tipos (claude,
   opencode, mcode, qwen, codex, pi) con modelos y esfuerzos válidos del catálogo y
   `--show` `tree`, `tab` y `right` repartidos.
2. Cada hijo: `parent` = padre, nombre de §6, etiquetas de §3, primer mensaje recibido
   con la marca única del encargo, y el argv/entorno que corresponde a su tipo (flags de
   Claude Code, Codex, Qwen Code y Pi; `OPENCODE_CONFIG_CONTENT` + `--standalone` de
   OpenCode; `--model` de mcode y su esfuerzo escrito en el `config.yaml` **temporal**).
3. `herdr-app ls`: el padre lista a los seis en `sub_agents`; `tab` tiene pestaña propia,
   `right` está en la pestaña del padre; el padre no cambió tras ubicar.
4. Rechazos sin crear terminal: tipo desconocido, modelo fuera del catálogo (Claude Code,
   OpenCode, Codex, mcode), esfuerzo inválido (Claude Code, OpenCode por modelo, Codex),
   esfuerzo en Qwen Code; Pi con modelo libre se acepta con aviso.
5. Limpieza: no queda ninguna terminal ni pestaña de la prueba y el foco vuelve a donde
   estaba. Nunca se tocan `voice.json`, mcode ni Codex del usuario (todo va a carpetas
   temporales: `APPDATA`, `MINIMAX_DATA_DIR`, `CODEX_HOME`, `OPENCODE_CONFIG_DIR`).

## 12. Fuera de alcance (después)
Integrar el árbitro (rondas, papeles, verificación) en la app de voz con este mismo
mecanismo; columna «tipo · modelo · esfuerzo» en el árbol (copia del mod `agents`) e
íconos de Qwen Code y MiniMax Code en `programs.json`; poner `ez.exe` en el PATH de los
agentes; prueba con CLIs reales (gasta cuota: se pide permiso).

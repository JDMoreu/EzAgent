# EzAgent

Mensajero de voz para Herdr con **notch** y **menú desplegable**. No hay app de
escritorio, dashboard ni panel de llamada: la isla flotante y su menú son toda
la interfaz.

```
Micrófono 16 kHz
   │
   ▼
VAD por energía ──► whisper.cpp (es) ──► LLM compatible con OpenAI
   │                                      (OpenCode, OpenAI, Groq, Claude, Gemini…)
   ▼
Kokoro (ONNX local, español)
   │
   ▼
Altavoz
```

| Capa | Motor | Dónde corre |
|---|---|---|
| VAD / segmentación | `Segmenter` local con pre-roll e histéresis | Local |
| STT | whisper.cpp + `large-v3-turbo` | Local, GPU AMD vía Vulkan |
| LLM | Proveedor compatible con OpenAI, configurable | Nube |
| TTS | Kokoro v1.0, voces españolas | Local, CPU (ONNX) |

**Pipecat no se usa.** El pipeline es propio (VAD, whisper.cpp, Kokoro y el
LLM); la carpeta `~/.cache/pipecat/kokoro-onnx` es una caché antigua que solo
se lee para migrar los modelos Kokoro, que se copian una vez a
`%LOCALAPPDATA%\EzAgent\models\kokoro`.

## Arranque

Requisitos: Windows 11, `uv` y una clave del proveedor LLM elegido. Los modelos
y binarios se descargan una sola vez, con verificación SHA256, a
`%LOCALAPPDATA%\EzAgent` (whisper.cpp, modelo Whisper, Kokoro y Silero VAD):

```powershell
uv run python -m ezagent.core.downloads --list   # qué falta
uv run python -m ezagent.core.downloads --all    # descarga lo necesario
```

La descarga automática instala whisper.cpp en **CPU**, porque el proyecto
oficial no publica binarios de Windows con Vulkan. Si tienes uno propio, copia
su `whisper-server.exe` y DLL a `%LOCALAPPDATA%\EzAgent\whisper` y se usará
antes que el de CPU. Otra carpeta: `EZAGENT_WHISPER_HOME` o `whisper_home` en
`voice.json`.

```powershell
git clone <url-del-repositorio> EzAgent
cd EzAgent
uv sync
# Opcional: dependencias de las skills de Word/PDF/Excel
uv sync --extra office
copy .env.example .env
# edita .env: elige LLM_PROVIDER y pon su clave

# Terminal 1: STT
powershell -ExecutionPolicy Bypass -File scripts\start-whisper.ps1

# Terminal 2: mensajero + notch
powershell -ExecutionPolicy Bypass -File scripts\start-voice.ps1
```

O doble clic en `EzAgent.bat` y elige **Iniciar mensajero de voz**. La app
arranca escuchando; `Ctrl+Alt+Space` cuelga o retoma.

## La gota (interfaz por defecto)

Desde la versión 0.2 la interfaz es **una barra sin texto**: una cápsula de color
entre cuatro botones redondos (ajustes ☰, repetir lo último, silenciar y colgar),
anclada abajo al centro, sobre la barra de tareas. **La forma es fija y de borde
nítido; lo que se mueve es el interior**, como agua o nubes de color: fluye
despacio en calma y corre y se arremolina más cuando hablas o escuchas. El color
cambia con el estado (espera, escucha, piensa, habla, pregunta, error y
silencio). Una confirmación convierte los botones de los lados en ✕ y ✓ (la
pregunta se dice por voz). Los ajustes se abren en una tarjeta encima de la
barra. Se arrastra por la forma y recuerda su posición.

**Círculo o píldora:** haz doble clic sobre la forma de color, o usa «Cambiar
forma» en el menú de la bandeja; la elección se guarda. El silencio del
micrófono solo se activa con el botón, el atajo `Ctrl+Alt+M` o una orden
explícita («silencia el micro»): palabras sueltas como «silencio» ya no lo
silencian.

Se configura en `voice.json`:

| Clave | Valores | Por defecto |
|---|---|---|
| `ui_style` | `drop` (la barra) · `notch` (la isla antigua) | `drop` |
| `ui_layout` | `pill` (cápsula) · `circle` (círculo sobre la fila de botones) | `pill` |
| `ui_palette` | `ezagent` (lima, menta, violeta) · `sky` (azules y lavanda) | `sky` |
| `ui_buttons` | `always` (siempre visibles) · `hover` (aparecen al acercar el puntero) | `always` |

Los iconos se regeneran con `uv run python scripts/make_icons.py`.

## El notch (interfaz anterior)

La isla cerrada mide **169×36** acoplada al borde superior o **151×36**
flotando, y se puede arrastrar por el cuerpo:

- Lejos del borde superior es una **píldora flotante** que se queda donde la
  sueltas; el borde inferior y los laterales no acoplan nada. Puede colocarse
  en cualquier punto del monitor, **incluida la franja de la barra de tareas**,
  y se mantiene por encima de ella.
- A ≤2 px del borde superior se acopla como **muesca** con el cuello hacia
  arriba.
- Al desplegar el menú, la isla se ensancha con él hasta **320×596** y forma
  una sola pieza continua.
- El micrófono refleja el estado real: **normal** cuando la escucha está
  activa, **tachado gris** cuando está apagada y **tachado rojo** cuando está
  silenciado. Pulsar el micrófono tachado reactiva la escucha y quita el mute;
  con la escucha activa, el mismo botón silencia o reactiva.
- La flecha derecha abre y cierra el menú; `Ctrl+Alt+O` hace lo mismo.

## Bandeja del sistema

Mientras EzAgent está abierto, aparece un icono en la bandeja de Windows con el
menú contextual:

- **Configurar:** muestra el notch si estaba oculto y abre Ajustes.
- **Reiniciar EzAgent:** cierra esta instancia y lanza una nueva con la misma
  configuración y argumentos.
- **Cerrar:** cierra la aplicación.

Windows puede colocar el icono nuevo en el desbordamiento de la bandeja (la
flecha `^`) la primera vez; se puede arrastrar a la zona visible. El tooltip
refleja el estado: en espera, escuchando, en conversación o micrófono
silenciado.

## El menú es Ajustes

El menú desplegable contiene todos los ajustes del mensajero y vive en la misma
ventana que el notch. Si hay más filas de las que caben, la rueda desplaza el
contenido entre la cabecera y el pie; el pulgar de scroll queda visible a la
derecha. Los campos de texto se editan en línea: al pulsarlos, el notch pide el
teclado y lo devuelve al confirmar con `Enter`, cancelar con `Esc` o pulsar
fuera. `Ctrl+V` pega desde el portapapeles.

- **AUDIO:** micrófono y **medidor de entrada con nivel en vivo y umbral
  arrastrable**; umbral de ruido y silencio de cierre.
- **MOTOR:** motor de Whisper (`Automático`, `GPU · Vulkan` o `CPU`) y variante
  de Kokoro (`int8`/`fp32`). El puerto local de Whisper se elige libre y se
  configura solo al arrancar. VAD y TTS corren en local.
- **TTS:** voz de Kokoro, velocidad y pausas de frase/cláusula. Las rutas de
  modelo/voces quedan como configuración avanzada.
- **VAD:** antelación de escucha, duración mínima, frase máxima y umbral de
  interrupción.
- **WHISPER · STT:** modelo en una **lista de los modelos instalados**, indicación,
  tiempo máximo y arranque automático. La dirección del servidor ya no se pide:
  EzAgent usa un puerto local libre.
- **LLM · API:** perfil guardado, proveedor, **modelo en lista**, dirección,
  clave, temperatura, máximo de tokens y **nivel de razonamiento**.
- **CONVERSACIÓN:** interrupción activa, confirmación opcional y prueba de voz.
- **AGENTE:** abre `system.md` o `voice.json` con el editor predeterminado para
  cambiar el comportamiento o los perfiles de modelo/esfuerzo de cada agente.
- **ATAJOS:** los cinco atajos globales editables en línea (llamada, colgar,
  repetir, silencio y ajustes). Se validan al guardar, no pueden repetirse y se
  re-registran sin reiniciar; si otra app usa el mismo atajo, queda en el log.
- Guardar, cambios sin guardar, ocultar la isla y cerrar el menú.

Los ajustes de audio, voz, TTS, VAD, Whisper y conversación se guardan en
`%APPDATA%\EzAgent\voice.json`; los perfiles de proveedor, en
`%APPDATA%\EzAgent\llm.json`. No hay una segunda ventana de configuración.
La posición de la isla se guarda como `notch_x`/`notch_y` en `voice.json` y se
restaura al abrir la app.

## Núcleo agéntico

EzAgent no solo delega en Herdr: también puede trabajar con archivos, buscar y
leer la web, cargar skills, recordar preferencias y controlar la escucha. Todo
pasa por una política de permisos y queda en una auditoría.

### Carpetas

```
%APPDATA%\EzAgent\
  voice.json            ajustes del mensajero y posición del notch
  llm.json              perfiles de proveedor
  permissions.toml      política de rutas, niveles y confirmaciones
  system.md             comportamiento y estilo (editable a mano)
  memory\USER.md        quién eres (1.375 caracteres)
  memory\MEMORY.md      hechos y flujo de trabajo (2.200 caracteres)
  memory\history.db     historial buscable (SQLite FTS5, 180 días)
  skills\               skills propias (una carpeta por skill con SKILL.md)
  tools.d\              herramientas declarativas extra (*.toml)
  logs\audit.jsonl      auditoría append-only, rota a 5 MB
  journal\              operaciones deshacibles
~\.agents\skills\       skills globales compartidas (solo lectura)
~\EzAgentWorkspace\     carpeta de trabajo por defecto
```

`permissions.toml` y `system.md` se crean con valores seguros la primera vez.
Se editan con un editor de texto; el agente nunca escribe `permissions.toml`.
`system.md` se recarga al inicio de cada turno, sin reiniciar.

### Herramientas

- **Hora y archivos:** `get_datetime`, `list_dir`, `read_file`, `create_dir`,
  `write_file`, `copy_path`, `move_path`, `delete_path`, `open_path`,
  `undo_last`. Borrar siempre manda a la **Papelera**; nunca hay borrado
  definitivo.
- **Web:** `web_search` y `fetch_page` con bloqueo de `localhost`, IPs privadas
  y redirecciones peligrosas. El contenido web se entrega al modelo como dato
  no confiable y activa `taint`: en ese turno, cualquier escritura pide
  confirmación.
- **Skills:** `load_skill` y `run_skill_script`. Se descubren en
  `%APPDATA%\EzAgent\skills\` y `~\.agents\skills\`; el catálogo se inyecta en
  el prompt. Los scripts están desactivados por defecto
  (`allow_scripts = false`) y, si se activan, piden confirmación y se aprueban
  por hash de la carpeta.
- **Memoria:** `remember` propone y solo guarda tras un sí; `recall` busca en el
  historial; `forget` y `compact_memory` piden confirmación. Para borrar todo
  el historial:
  `python -m ezagent.agent.memory wipe` (pide escribir «borrar»; `--yes` para
  modo no interactivo).
- **Herdr:** `delegate_to_agent` (listar, leer, enviar, lanzar, enfocar,
  vigilar o matar) y `voice_control` (cerrar escucha, silenciar, repetir).

### Confirmaciones

Las operaciones destructivas (borrar, sobrescribir, matar un agente) siempre
piden confirmación. El toggle **CONVERSACIÓN · Confirmar acciones** solo añade
confirmación a las operaciones de nivel 2 no destructivas. La pregunta se lee
por voz y aparece como una fila bajo la isla con **Sí** y **No**; también se
puede responder «sí», «confirmo», «adelante» o «dale». Para operaciones
masivas hay que decir «confirmo» y el número de elementos. Si no hay respuesta
en 15 s, se cancela.

### Skills incluidas

`organizar-carpetas`, `investigar-web`, `documentos-word`, `documentos-pdf`,
`hojas-excel`, `resumir-documento` y `notas-rapidas`. `documentos-word`,
`documentos-pdf` y `hojas-excel` necesitan `uv sync --extra office`. En el
ejecutable portable, los
scripts de skills buscan un Python del sistema; sin él, las skills que solo
leen/escriben archivos siguen funcionando y las que ejecutan scripts muestran
un error claro.

## Tareas pendientes y avisos

Cuando pides algo como «vigila esta tarea y avísame cuando termine», EzAgent
guarda la dirección del agente y **la tarea concreta** en
`%APPDATA%\EzAgent\tasks.json`. Esa tarea:

- se inyecta en el prompt como `<pending_tasks>` para que el asistente sepa qué
  está esperando;
- se conserva si reinicias EzAgent;
- se usa en el aviso de finalización: «Oye, ya terminé «…». ¿Quieres que te
  resuma qué hizo, que te lea la salida o que te la muestre?»;
- se elimina cuando el watcher avisa.

Puedes preguntar «¿qué tareas pendientes tengo?» y el asistente las enumera.

### Modelo y esfuerzo por agente

Cada agente delegado tiene su propio perfil en `voice.json`, independiente del
modelo del mensajero de voz. `delegate_to_agent` acepta `model` y `effort` en
`action=run`; los flags se añaden solo al comando de ese agente:

```json
"agents": {
  "claude": ["claude"],
  "opencode": ["opencode"],
  "mcode": ["mcode"],
  "codex": ["codex"],
  "kimi": ["kimi"]
},
"agent_profiles": {
  "claude": {
    "models": ["sonnet", "opus", "haiku"],
    "efforts": ["low", "medium", "high"],
    "default_model": "",
    "default_effort": "",
    "model_args": ["--model", "{model}"],
    "effort_args": ["--effort", "{effort}"]
  },
  "opencode": {
    "models": ["deepseek-v4.1-flash"],
    "efforts": ["low", "medium", "high"],
    "default_model": "",
    "default_effort": "",
    "model_args": ["--model", "{model}"],
    "effort_args": ["--effort", "{effort}"]
  }
}
```

`default_model` y `default_effort` vacíos significan «solo aplica modelo o
esfuerzo cuando el usuario o el LLM los piden». Si quieres un valor fijo para
un agente, rellénalos. Ajusta `model_args`/`effort_args` a lo que acepte cada
CLI (por ejemplo, Codex usa `-c model_reasoning_effort={effort}`). Las claves
de `agents` son libres: puedes añadir `kimi`, `deepseek`, `minimax` o el agente
que uses, y crear su perfil.

### Encadenar la salida de un agente en otro

Pasa `input_from` al segundo `action=run` con la dirección del primero. Si el
primero sigue trabajando, EzAgent guarda la cadena, espera a que termine y lanza
el segundo con su salida como contexto; si ya terminó, la pasa en el mismo
turno. El destino conserva su propio modelo/esfuerzo y, al acabar, da el aviso
con la tarea concreta.

Ejemplo por voz:

> «Abre Claude Code en esfuerzo low con Sonnet y pásale su salida a OpenCode
> con DeepSeek en esfuerzo alto para que haga el plan. Avísame al terminar.»

## Proveedor y modelo LLM

Se pueden cambiar desde el grupo **LLM · API** del menú o con variables de
entorno y `llm.json`. Los proveedores integrados son OpenCode, OpenAI, Groq,
Claude (Anthropic), Gemini, OpenRouter y cualquier endpoint compatible con
OpenAI. Ejemplo por entorno:

```dotenv
LLM_PROVIDER=groq
GROQ_API_KEY=...
# Opcionales; si faltan, se usan los del proveedor:
# LLM_MODEL=llama-3.3-70b-versatile
# LLM_BASE_URL=https://api.groq.com/openai/v1
```

Proveedores integrados: OpenCode, OpenAI, Groq, Claude (Anthropic), Gemini,
OpenRouter y cualquier endpoint compatible con OpenAI. También puedes usar
`%APPDATA%\EzAgent\llm.json` o `EZ_LLM_CONFIG` con perfiles por proveedor.
Guía completa: [`docs/llm-providers.md`](docs/llm-providers.md).

## Atajos

| Atajo | Acción |
|---|---|
| `Ctrl+Alt+Space` | Iniciar / colgar la llamada |
| `Ctrl+Alt+E` | Colgar (`Ctrl+Alt+C` sigue como alias) |
| `Ctrl+Alt+M` | Silenciar / reactivar el micrófono |
| `Ctrl+Alt+O` | Abrir / cerrar el menú del notch |
| `Ctrl+Alt+R` | Repetir el último resumen |

El micrófono no se cierra entre turnos durante una llamada. Las acciones
explícitas de nivel 0 y 1 se ejecutan directamente y el asistente responde en
una frase corta con qué hace; las destructivas o masivas piden confirmación
aunque el usuario haya dado la orden.

## Pruebas

```powershell
uv run pytest -q
```

Detalle del mensajero, límites y arquitectura de audio:
[`src/ezagent/voice/README.md`](src/ezagent/voice/README.md).

## Comunidad

- Licencia: [Apache-2.0](LICENSE). Atribuciones de terceros en [NOTICE](NOTICE).
- Cómo contribuir: [CONTRIBUTING.md](CONTRIBUTING.md) y el contrato para
  agentes de código en [AGENTS.md](AGENTS.md).
- Seguridad: cómo reportar un problema en [SECURITY.md](SECURITY.md).
- Convivencia: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

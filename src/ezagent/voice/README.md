# EzAgent Voice — mensajero para Herdr

Capa de voz para Herdr. Pulsas un atajo, hablas, y el mensajero:

- transcribe con **whisper.cpp**;
- entiende la petición con el **LLM configurable** (OpenCode Go, OpenAI, Claude, Gemini, Groq, Cerebras, DeepSeek, OpenRouter o uno personalizado compatible con OpenAI o con Anthropic);
- consulta o delega en **Herdr** (`hc`);
- te responde por voz con **Kokoro**;
- te avisa cuando un agente termina o se bloquea.

No hace las tareas de código: solo escucha, consulta, delega y narra.

## Requisitos

- Windows 11.
- Herdr en marcha y `hc` en el PATH.
- `whisper-server` con Vulkan y el modelo `ggml-large-v3-turbo-q5_0.bin` en
  `%LOCALAPPDATA%\EzAgent\whisper` (la carpeta contiene `whisper-server.exe` y
  `models\`). EzAgent lo arranca solo en un puerto local libre; para usar un
  servidor externo, define `WHISPER_URL`.
- La clave del proveedor LLM elegido en `.env` (por ejemplo,
  `OPENCODE_API_KEY`).
- Modelo Kokoro descargado (se descarga solo la primera vez).

## Arrancar

```powershell
# desde la raíz del proyecto
powershell -ExecutionPolicy Bypass -File scripts\start-voice.ps1

# o
uv run ezagent-voice

# detenerlo
powershell -ExecutionPolicy Bypass -File scripts\stop-voice.ps1
```

Para uso diario, doble clic en **`EzAgent.bat`** en la raíz: arranca Herdr si
hace falta, abre el mensajero en una ventana propia y comprueba `.env`, `.venv`
y whisper. El mismo lanzador tiene menú de estado, micrófonos, agentes,
actualización; `EzAgent.bat voice` inicia directamente y
`EzAgent.bat stop` detiene voz y whisper.

La primera vez crea/lee `%APPDATA%\EzAgent\voice.json`. Para generarlo con los
valores actuales:

```powershell
uv run ezagent-voice --write-config
```

## Atajos

| Atajo | Acción |
|---|---|
| **Ctrl+Alt+Space** | Iniciar / colgar la llamada |
| **Ctrl+Alt+E** | Colgar (Ctrl+Alt+C sigue funcionando como alias) |
| **Ctrl+Alt+M** | Silenciar / reactivar el micrófono |
| **Ctrl+Alt+O** | Abrir / cerrar el menú embebido |
| **Ctrl+Alt+R** | Repetir el último resumen (`Ctrl+Alt+Y` si R está ocupado) |

Se cambian en `voice.json` (`hotkey`, `cancel_hotkey`, `repeat_hotkey`).
Los cinco atajos de EzAgent son globales y se re-registran sin reiniciar al
guardar; si otro programa ya usa uno, queda anotado en el log.

Al abrir el mensajero, entra en modo escucha automáticamente a los 600 ms: no hace
falta pulsar nada. `Ctrl+Alt+Space` sirve para colgar o volver a llamar. En
modo llamada el micrófono **no se cierra entre turnos**: hablas, el asistente
responde y vuelve a escuchar solo. La barra queda visible durante toda la
llamada.

El menú se ordena por tarea: **APARIENCIA**, **ESCUCHA**, **MICRÓFONO**
(dispositivo, medidor con umbral arrastrable y ruido), **FIN DE TURNO**
(silencio máximo, cierre inteligente, certeza para cerrar, antelación,
duración mínima y frase máxima), **CONVERSACIÓN** (interrupción y su umbral,
confirmaciones, prueba de voz), **VOZ · KOKORO** (voz, variante, velocidad y
pausas), **TRANSCRIPCIÓN · WHISPER** (motor, modelo instalado, indicación y
tiempo máximo), **LLM · API** (proveedor, perfil, compatibilidad, dirección,
clave, modelo escrito a mano, modelos de la API, temperatura, tokens y
razonamiento), **AGENTE** y **ATAJOS**. El puerto local de Whisper se elige
libre. El contenido que no cabe se desplaza con rueda y un pulgar visible. La
voz, el silencio, el cierre inteligente, el barge-in y los umbrales del VAD se
aplican en vivo; cambiar de micrófono reabre la captura; los cambios de LLM se
aplican al guardar y los de modelo TTS recargan Kokoro. El pie muestra `TODO GUARDADO` o
`● CAMBIOS SIN GUARDAR`; Guardar es ámbar cuando hay cambios y confirma con un
check. El menú solo usa texto para las etiquetas de ajustes y el pie.

El código de la interfaz vive en `notch/`: `tokens.py` (tokens), `anim.py`
(curvas y tween), `bar_motion.py` y `bar_theme.py` (movimiento y tema de la
barra), `clouds.py` (cielo), `drop.py` (ventana y composición), `menu.py`
(menú y componentes), `settings.py` (modelo de ajustes) y `state.py` (máquina de
estados). No hay panel Tk ni segunda ventana de ajustes.

### Latencia

- **Fin de turno con Smart Turn** (Pipecat, BSD-2, modelo ONNX de 8 MB que se
  descarga solo): tras **0,35 s** de silencio el modelo mira si tu frase terminó.
  Si la ve completa, el turno se cierra ya (`VAD end: … (fin de turno
  detectado)`); si no, se espera el silencio de siempre (**1,3 s**, ajustable en
  el menú). No cambia la interrupción (barge-in): esa usa su propio umbral.
- Si el turno se cerró en una pausa y sigues hablando mientras el modelo piensa,
  la frase nueva se transcribe **junto con la anterior** («Sigues hablando: uno
  esta frase con la anterior»), en vez de perder la primera parte.
- El VAD tiene pre-roll e histéresis para no cortar a la primera pausa.
- Kokoro y whisper se **precalientan** al arrancar.
- Mute, desmute, cerrar, repetir, saludar y listar agentes son **atajos
  locales**: no pasan por el LLM.
- El LLM sigue en streaming; el primer trozo que se manda a Kokoro es una
  cláusula corta para que el primer audio llegue antes.
- **Cola de voz**: Kokoro sintetiza la frase siguiente mientras suena la actual,
  así no quedan silencios entre frases.
- **Acuse inmediato**: si hace falta una herramienta o un agente, el modelo
  empieza con una frase corta («Va, lo reviso.») antes de trabajar. Si no lo
  hace, el código dice una de una lista fija. Con varias llamadas al LLM, el log
  añade `LLM por llamada: …` con el tiempo de cada llamada y herramienta.
- La hora va al **final** del system prompt para no invalidar la caché por
  prefijo del proveedor a cada minuto.
- Al transcribir, el log muestra `Tiempos: VAD # ms | STT # ms`. Al terminar
  el turno añade el desglose completo, por ejemplo
  `Tiempos: VAD 650 ms | STT 420 ms | LLM 900 ms (1.er token 380 ms) |
  TTS 210 ms síntesis | Voz 3200 ms | 1.er audio a los 1660 ms | Turno 4800 ms`.
  `VAD` es el silencio que espera antes de cortar tu frase; `LLM` cuenta solo
  la espera de tokens (incluye herramientas, no la voz); `TTS` es la síntesis y
  `Voz`, la reproducción. Los comandos locales muestran `Comando local: # ms`.
  Cada diez turnos añade media y P95 por etapa, y todo queda en
  `%APPDATA%\EzAgent\logs\latency.jsonl`.

### Avisos de tarea, cierre y mute

- **“Ciérrate y avísame cuando termine X”**: el mensajero registra el aviso,
  dice que cierra la escucha, **apaga el STT** y espera en segundo plano.
  Cuando el agente termina, **reabre solo la escucha/notch** y pregunta:
  “He terminado la tarea. ¿Quieres que te diga la salida, te resuma lo que
  hizo, o quieres darme otra instrucción?”.
- Puedes responder “sí”, “dime solo la salida”, “resume”, “haz esto ahora” o
  cualquier otra indicación. Si dices “deja de escuchar” o “cierra”, se cierra.
- Solo se avisa de los agentes que pides explícitamente (`announce_all=false`).
- **Ctrl+Alt+M** silencia el micrófono. El notch muestra un micrófono con una
  barra roja mientras está muteado y el micrófono normal al reactivarlo. El
  ruido de fondo no se transcribe.
- También puedes decir “mute” / “desmute” y el mensajero usa
  `voice_mic_mute`.

### Streaming e interrupciones

- El LLM responde en **streaming** y el TTS habla **frase a frase**, sin esperar
  a que el modelo termine.
- **Tú puedes interrumpir** al asistente: si hablas mientras responde, se corta
  el TTS y se procesa tu nueva frase.
- **El asistente no te interrumpe**: espera a que hagas una pausa antes de
  responder o de dar cualquier aviso.
- Cuando un agente termina, **reabre la escucha/notch** y pregunta si quieres
  la salida, un resumen o dar otra instrucción. No da el resumen hasta que se
  lo pidas.
- Los avisos de agente **se entregan aunque cuelgues**: la cola no se borra al
  colgar; si hay uno sonando, termina de decirse.
- El STT sigue siendo por turnos (VAD + whisper), no palabra a palabra; lo que
  se streamea es la respuesta del asistente.

## Micrófono

El mensajero elige automáticamente un micrófono real y descarta los dispositivos
virtuales de Windows (Controlador primario, Mezcla estéreo, Steam...), que
entregan audio sin bloquear y rompen el VAD.

```powershell
uv run ezagent-voice --list-mics
uv run ezagent-voice --mic 2
```

Hay micrófonos con poca ganancia (los de portátil) y webcams que quedan lejos.
El VAD parte de `min_rms = 0.0006` (antes 0.0015) y solo aprende ruido de los
niveles cercanos al suelo, para que una frase floja no se confunda con ruido ni
suba el umbral del siguiente intento. Si aun así no te detecta, baja
`min_rms` a `0.0003` en el menú del notch o en `voice.json`; si el ruido de
fondo abre turnos falsos, súbelo. No hay AGC de software: si el dispositivo
tiene poca ganancia, conviene elegir otro micrófono en el menú.

## Pruebas sin micrófono

```powershell
uv run ezagent-voice --list-agents
uv run ezagent-voice --ask "¿Qué agentes hay ahora mismo?"
uv run ezagent-voice --say "Hola, soy el mensajero de Herdr."
```

## Qué entiende

- “¿Qué agentes hay?”
- “¿Qué está haciendo el agente de OpenCode?”
- “Lanza Claude Code en EzAgent y que arregle los tests.”
- “Pásale a OpenCode que use el modelo pequeño.”
- “Avísame cuando termine.”
- “Repítemelo.”
- “Silencio diez minutos.”

Las órdenes explícitas de nivel 0 y 1 se ejecutan directamente; las
destructivas (matar, borrar, sobrescribir) o masivas piden confirmación por voz
o con la fila Sí/No del notch. El asistente responde en una frase corta con qué
abre y dónde.

## Núcleo agéntico

Además de Herdr, el mensajero usa el núcleo de `src/ezagent/agent/`: permisos,
archivos, web, skills y memoria. El detalle de carpetas, niveles y
confirmaciones está en el README raíz. Las herramientas de Herdr se agrupan en
`delegate_to_agent` (action list/get/read/send/run/open_terminal/focus/kill/watch)
y `voice_control` (close/close_and_watch/mic_mute/repeat_last/mute) para no
inflar el prompt.

## Visibilidad y CUA

- **Por defecto todo se ve en Herdr.** `delegate_to_agent` con
  `action=open_terminal` abre una terminal visible y con `action=run` abre una
  pestaña visible con el agente y la tarea.
- Solo se usa `background=true` si el usuario dice explícitamente “en segundo
  plano”, “sin mostrarlo” o “no quiero verlo”.
- Para tareas de interfaz gráfica (abrir Brave, YouTube, navegar, hacer clic,
  escribir en apps), el mensajero pone `gui=true`; el agente recibe la
  instrucción de usar la skill **cua-driver** y deja las ventanas a la vista.
- La skill `cua-driver` está enlazada para Claude Code, Codex y OpenCode
  (`cua-driver skills status`). Si usas otro agente:
  `cua-driver skills install`.
- El mensajero puede leer el terminal del agente con `action=read` y resumir el
  progreso; no hace falta que mires la pantalla para saber qué pasa.

Ejemplo:

> “Abre una terminal visible, lanza Claude Code y con CUA driver abre Brave,
> entra en YouTube y reproduce un vídeo.”

El mensajero abre las pestañas visibles y delega; el agente usa `cua-driver`
para manejar Brave/YouTube.

## Proveedor del LLM (menú → LLM · API)

- **Proveedor**: OpenCode Go, OpenAI (GPT, Codex), Claude (Anthropic), Gemini,
  Groq, Cerebras, DeepSeek, OpenRouter o **Personalizado**. Las claves se leen
  de `.env` (`OPENCODE_API_KEY`, `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`,
  `GEMINI_API_KEY`, `GROQ_API_KEY`, `CEREBRAS_API_KEY`, `DEEPSEEK_API_KEY`,
  `OPENROUTER_API_KEY`) o se escriben en el menú.
- **Personalizado**: elige «Personalizado», ponle un **Nombre del perfil**
  (p. ej. `minimax`), elige **Compatibilidad** (OpenAI o Anthropic) y escribe
  dirección, clave y modelo. Al guardar queda como un proveedor más en la lista.
  Para Anthropic vale la raíz que da el servicio (`…/anthropic`) o una que
  termine en `/v1`.
- **Modelo** es texto libre: escribe el id exacto. **Modelos de la API** pide
  la lista real al proveedor (`GET /models`) al abrir el menú, al cambiar de
  proveedor y al guardar; elegir uno rellena el campo.
- Las suscripciones de Claude Code o Codex (inicio de sesión con la cuenta) no
  son claves de API: aquí hace falta una clave de API de Anthropic u OpenAI.
- Los perfiles viven en `%APPDATA%\EzAgent\llm.json`.

## Configuración (`voice.json`)

Los cinco atajos (`hotkey`, `cancel_hotkey`, `repeat_hotkey`, `mute_hotkey`,
`menu_hotkey`) se pueden editar en el grupo **ATAJOS** del menú o a mano aquí.
Al guardar se validan, no pueden repetirse y se re-registran sin reiniciar.
EzAgent no usa Pipecat; el pipeline es propio.

| Campo | Por defecto | Qué hace |
|---|---|---|
| `hotkey` | `ctrl+alt+space` | Hablar / parar |
| `cancel_hotkey` | `ctrl+alt+e` | Colgar |
| `mute_hotkey` | `ctrl+alt+m` | Silenciar / reactivar micrófono |
| `menu_hotkey` | `ctrl+alt+o` | Abrir / cerrar el menú |
| `repeat_hotkey` | `ctrl+alt+r` | Repetir último resumen |
| `whisper_url` | `http://127.0.0.1:8088/inference` | STT |
| `stt_model` | `ggml-large-v3-turbo-q5_0` | Modelo de whisper-server |
| `stt_engine` | `auto` | `auto`, `vulkan` o `cpu` |
| `stt_prompt` | `""` | Contexto opcional para whisper |
| `stt_timeout_secs` | `60` | Timeout del STT |
| `voice` | `em_alex` | Voz Kokoro |
| `speed` | `0.95` | Velocidad |
| `tts_model_variant` | `int8` | Variante del modelo Kokoro |
| `kokoro_model_path` | `""` | Ruta manual del modelo (opcional) |
| `kokoro_voices_path` | `""` | Ruta manual de voces (opcional) |
| `tts_sentence_pause` | `0.30` | Pausa entre frases |
| `tts_clause_pause` | `0.12` | Pausa entre cláusulas |
| `input_device_index` | `null` | Micrófono; `null` = automático |
| `output_device_index` | `null` | Altavoz |
| `silence_secs` | `1.3` | Silencio que cierra el turno (más alto = no te corta) |
| `smart_turn` | `true` | Cierra antes si Smart Turn ve la frase completa |
| `turn_quick_secs` | `0.35` | Silencio tras el que se consulta a Smart Turn |
| `turn_threshold` | `0.5` | Probabilidad mínima de «frase completa» (más alto = más prudente) |
| `min_rms` | `0.0006` | Umbral mínimo de voz (sensibilidad) |
| `pre_roll_secs` | `0.6` | Audio previo que se conserva |
| `min_speech_secs` | `0.2` | Duración mínima para enviar a STT |
| `max_utterance_secs` | `30` | Tope de una intervención |
| `noise_multiplier` | `1.3` | Umbral sobre el ruido de fondo |
| `barge_in` | `true` | Interrumpir el TTS al hablar |
| `barge_in_rms` | `0.008` | Nivel mínimo para barge-in |
| `auto_start_whisper` | `true` | Arrancar whisper-server solo |
| `llm_provider` | `opencode` | Perfil activo: un preset o el nombre de uno personalizado |
| `llm_api_style` | del perfil | `openai` (Chat Completions) o `anthropic` (Messages) |
| `llm_model` | `deepseek-v4.1-flash` | Modelo del mensajero (se escribe tal cual) |
| `llm_base_url` | endpoint del proveedor | URL base de la API |
| `llm_pinned` | `false` | `true` si el proveedor se eligió en el menú |
| `llm_temperature` | `0.3` | Creatividad del LLM |
| `llm_max_tokens` | `320` | Tope de la respuesta |
| `llm_reasoning_effort` | `none` | `none`, `low`, `medium` o `high` |
| `confirm_actions` | `false` | Confirmar también las operaciones de nivel 2 no destructivas; las destructivas siempre confirman |
| `hc_path` | `hc` | CLI de Herdr |
| `herdr_app_path` | `herdr-app` | CLI de la ventana de Herdr (pestañas visibles) |
| `default_cwd` | cwd actual | Carpeta por defecto para lanzar agentes |
| `agents` | claude/opencode/mcode/codex | Comandos para lanzar cada agente |
| `agent_profiles` | claude/opencode/mcode/minimax/codex | Modelos, esfuerzos y flags de cada agente delegado |
| `announce_all` | `false` | Avisar de todos los agentes (no recomendado) |
| `max_read_lines` | `40` | Líneas del buffer que se leen |

## Límites del MVP

- Solo Windows (notch Win32 y `RegisterHotKey`).
- Sin icono de bandeja: se ejecuta en segundo plano; se para con
  `scripts\stop-voice.ps1`.
- Sin wake word: el mensajero arranca en modo escucha (`Ctrl+Alt+Space` cuelga o
  retoma); la barra solo mutea, abre el menú y se arrastra.
- El watcher solo anuncia los agentes pedidos (`announce_all=false`).
- Con altavoces, el TTS puede colarse por el micrófono y disparar el barge-in.
  Usa auriculares o pon `barge_in: false`.
- El STT es por turnos (whisper batch); el streaming es del LLM/TTS. Para STT
  palabra a palabra haría falta un modelo streaming (Nemotron 3.5).
- El notch se basa en la capa layered de **Dicta** (MIT).

## Siguiente iteración

- Icono de bandeja y arranque con Windows.
- Inyectar la respuesta de voz directamente en el agente (`hc send`).
- MCP + skill para Claude Code, OpenCode y MiniMax Code fuera de Herdr.
- Mod de Herdr para ver el mismo estado dentro de la app.

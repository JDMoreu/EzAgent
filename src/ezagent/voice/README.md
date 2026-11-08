# EzAgent Voice — mensajero para Herdr

Capa de voz para Herdr. Pulsas un atajo, hablas, y el mensajero:

- transcribe con **whisper.cpp**;
- entiende la petición con el **LLM configurable** (OpenCode, OpenAI, Groq, Claude, Gemini o endpoint compatible);
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
responde y vuelve a escuchar solo. La isla queda visible durante toda la
llamada.

## Controles de la isla

La isla no es solo un indicador: también es el mando del mensajero.

| Zona | Acción |
|---|---|
| Micrófono | Silenciar / reactivar: tachado y en rojo cuando está muteado |
| Cuerpo de la pestaña | Arrastrar la isla por la pantalla |
| Flecha derecha | Abre o cierra el menú completo; es un chevrón sin fondo |

La isla cerrada mide **169×36** en la muesca superior o **151×36** como píldora
flotante: el espacio justo para los tres controles, sin el vacío que dejaba la
antigua palabra de estado. Queda como **control compacto en reposo**; la llamada
se inicia o termina con `Ctrl+Alt+Space`, no desde la isla. Al arrastrar su
cuerpo se convierte en una **píldora flotante** que se queda donde la sueltas;
solo si la sueltas a menos de 2 px del borde superior se acopla como muesca con
el cuello arriba. El borde inferior y los laterales no acoplan nada. La flecha
derecha despliega el **menú completo dentro de la misma ventana** hacia el lado
con más espacio. La píldora se ensancha junto con el menú hasta 320 px, de modo
que barra y panel son una sola pieza continua, sin costura, sin tooltips y sin
ventanas secundarias. El menú se cierra con la flecha o con
`Ctrl+Alt+O`; el botón `–` oculta la isla hasta la próxima llamada o atajo. El
menú no roba el foco al abrirse ni al desplazarse: solo pide el teclado mientras
se edita una fila de texto y lo devuelve al confirmar con `Enter`, cancelar con
`Esc` o pulsar fuera. `Ctrl+V` pega desde el portapapeles.

El menú tiene siete grupos: **AUDIO**, **MOTOR**, **TTS**, **VAD**,
**WHISPER · STT**, **LLM · API** y **CONVERSACIÓN**. Incluye micrófono, un
medidor de entrada con nivel en vivo y umbral arrastrable, ruido, silencio,
motor de Whisper y variante de Kokoro, voz/velocidad/pausas, parámetros del VAD,
modelo de Whisper instalado, y proveedor/modelo/endpoint/clave/temperatura/
razonamiento del LLM. Los modelos de Whisper y del proveedor LLM se eligen en
listas; el puerto local de Whisper se elige libre. El contenido que no
cabe se desplaza con rueda y un pulgar visible. La voz, el silencio, el barge-in
y los umbrales del VAD se aplican en vivo; cambiar de micrófono reabre la
captura; los cambios de LLM se aplican al guardar y los de modelo TTS recargan
Kokoro. El pie muestra `TODO GUARDADO` o
`● CAMBIOS SIN GUARDAR`; Guardar es ámbar cuando hay cambios y confirma con un
check. El estado de los ojos sigue a la voz: **ESPERA**, **ESCUCHO**,
**PIENSO**, **HABLO**, **AVISO**, **PAUSA** y **ERROR**. **La isla no tiene
texto**: no hay palabra de estado, pistas ni tooltips; los botones son iconos y
el estado se lee en la forma y el color de los ojos. El menú solo usa texto para
las etiquetas de ajustes y el pie.

El menú usa los tokens de `DESIGN.md`: cuerpo grafito `#0E0F13`, grupos
`#1C1C1F`, un solo mint `#5EEAC4`, ámbar `#FFC24A` y rojo `#FF453A`. Los ojos
de dos cápsulas son la única marca; la onda de cinco cápsulas solo existe en
**HABLO**. La tipografía es **Geist / Geist Mono**, empaquetada en
`src/ezagent/voice/assets/fonts`, y los iconos se dibujan como vectores. El tema
sigue a Windows (`auto`) o se fija como `dark`/`light` en `voice.json`.

El código está separado en `notch/tokens.py` (tokens), `anim.py` (curvas y
tween), `eyes.py` (poses), `silhouette.py` (cuerpo con hombros), `menu.py`
(menú y componentes), `settings.py` (modelo de ajustes), `state.py` (máquina de
estados) e `island.py` (ventana y composición). No hay panel Tk ni segunda
ventana de ajustes.

### Latencia

- El silencio de cierre es de **1,3 s** (ajustable en el menú del notch). El VAD tiene
  pre-roll e histéresis para no cortar a la primera pausa.
- Kokoro y whisper se **precalientan** al arrancar.
- Mute, desmute, cerrar, repetir, saludar y listar agentes son **atajos
  locales**: no pasan por el LLM.
- El LLM sigue en streaming; el primer trozo que se manda a Kokoro es una
  cláusula corta para que el primer audio llegue antes.
- El log muestra `STT`, `Primer audio`, `Turno completo` y, por turno,
  `Latencia: vad_wait_ms ... total_ms`; cada diez turnos añade media y P95.
  El detalle y el protocolo de 20 frases están en `docs/voice-latency.md`.

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
| `min_rms` | `0.0006` | Umbral mínimo de voz (sensibilidad) |
| `pre_roll_secs` | `0.6` | Audio previo que se conserva |
| `min_speech_secs` | `0.2` | Duración mínima para enviar a STT |
| `max_utterance_secs` | `30` | Tope de una intervención |
| `noise_multiplier` | `1.3` | Umbral sobre el ruido de fondo |
| `barge_in` | `true` | Interrumpir el TTS al hablar |
| `barge_in_rms` | `0.008` | Nivel mínimo para barge-in |
| `auto_start_whisper` | `true` | Arrancar whisper-server solo |
| `llm_provider` | `opencode` | Proveedor activo (OpenAI-compatible) |
| `llm_model` | `deepseek-v4.1-flash` | Modelo del mensajero |
| `llm_base_url` | endpoint del proveedor | URL base de la API |
| `llm_pinned` | `false` | `true` si el proveedor se eligió en el menú |
| `llm_temperature` | `0.3` | Creatividad del LLM |
| `llm_max_tokens` | `320` | Tope de la respuesta |
| `llm_reasoning_effort` | `none` | `none`, `low`, `medium` o `high` |
| `confirm_actions` | `false` | Confirmar también las operaciones de nivel 2 no destructivas; las destructivas siempre confirman |
| `notch_x` / `notch_y` | `null` | Última posición física de la isla; se guarda al arrastrar |
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
  retoma); la isla solo mutea, abre el menú y se arrastra.
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

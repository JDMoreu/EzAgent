# EzAgent Voice — arquitectura

## Idea en una frase

`ezagent-voice` es un **orquestador de voz externo** que decide qué hacer y
delega en Herdr; **Herdr core** ejecuta y mantiene los terminales/agentes, y
**Herdr app** los muestra y organiza. El mensajero no reimplementa ninguno de
los dos: los usa por sus CLIs (`hc` y `herdr-app`).

## Piezas y responsabilidades

| Pieza | Responsabilidad | No hace |
|---|---|---|
| **Herdr core (`hc`)** | Ejecuta terminales y agentes, conserva procesos, buffers, estados (OSC 7501), labels, turnos y relaciones padre/hijo | No dibuja la interfaz |
| **Herdr app (`herdr-app`)** | Espacios, pestañas, paneles, layouts, mods, foco, notificaciones y teclas | No es el runtime de los terminales |
| **ezagent-voice** | Escucha, transcribe, razona, delega, vigila y narra | No ejecuta agentes por su cuenta |
| **Agentes** (Claude Code, OpenCode, mcode, Codex) | Hacen la tarea real; pueden usar `cua-driver` para GUI | No orquestan a otros |
| **Mod de Herdr** (opcional, futuro) | Panel/barra dentro de Herdr para estado, mute y últimos avisos | No captura audio ni ejecuta el LLM |

## Diagrama

```
┌──────────────────────────────────────────────────────────────┐
│ Usuario                                                      │
│  hotkeys globales · micrófono · altavoz · notch              │
└───────────────────────────┬──────────────────────────────────┘
                            │ audio / eventos
┌───────────────────────────▼──────────────────────────────────┐
│ ezagent-voice (Python, proceso externo)                      │
│                                                              │
│  Hotkeys globales ──► estado IDLE/LISTENING/THINKING/SPEAKING│
│  VAD adaptativo ──► whisper.cpp ──► OpenCode DeepSeek 4.1    │
│                                      │ tool calling          │
│                                      ├─ Herdr adapter         │
│                                      ├─ Watcher (polling)     │
│                                      └─ Kokoro TTS ──► notch  │
│                                                              │
│  Notch Win32 layered (reutilizado de Dicta)                  │
└───────────────┬──────────────────────────────┬───────────────┘
                │ `hc ...`                     │ `herdr-app do ...`
                ▼                              ▼
┌──────────────────────────────┐   ┌───────────────────────────┐
│ Herdr core                   │   │ Herdr app                 │
│ terminales, agentes, estados │◄──│ espacios, tabs, panes,    │
│ buffers, procesos, labels    │   │ layouts, mods, foco       │
└───────────────┬──────────────┘   └───────────────────────────┘
                │ PTY + OSC 7501
                ▼
┌──────────────────────────────────────────────────────────────┐
│ Claude Code · OpenCode · mcode · Codex                       │
│ (trabajadores; `cua-driver` para Brave/YouTube/escritorio)   │
└──────────────────────────────────────────────────────────────┘
```

## Por qué está separado así

- **Core = verdad de ejecución.** Los terminales viven en `herdr-core` y
  sobreviven al cierre de la ventana. El core sabe qué proceso corre, en qué
  carpeta, con qué estado y qué ha escrito.
- **App = verdad de presentación.** La app decide qué se ve, en qué espacio,
  pestaña o panel. No es dueña de los procesos.
- **Mensajero = verdad de intención.** Escucha al usuario, decide si es una
  pregunta, una orden o un aviso, y llama a las herramientas. Es el único que
  habla con el LLM y con el audio.
- **Agentes = trabajadores.** Hacen la tarea. El mensajero no la hace.

Esta separación permite que el mensajero tenga **hotkey global y notch
siempre-encima** aunque Herdr no tenga el foco, y que Herdr siga siendo la
fuente de verdad de los agentes.

## Flujos principales

### 1. Lanzar una tarea visible

```
Voz: "Abre Claude Code en EzAgent y arregla los tests"
  │
  ▼
VAD → whisper.cpp → LLM compatible + tool herdr_run_agent
  │
  ├─ visible (por defecto):
  │    herdr-app do tab.new \
  │       --name "claude · voz" \
  │       --folder C:\...\EzAgent \
  │       --argv '["claude"]' \
  │       --task "arregla los tests"
  │         │
  │         ├─ Herdr app crea una pestaña en el espacio enfocado
  │         └─ Herdr core arranca el terminal del agente (p. ej. t1b)
  │
  └─ background (solo si el usuario lo pide):
       hc run --json --bg --no-parent ... + hc send
  │
  ▼
Resultado: { terminal, tab, space, visible }
  │
  ▼
Kokoro: "Lanzado en la pestaña ... visible en Herdr."
```

### 2. Vigilar y avisar al terminar

```
"Avísame cuando termine" / "Ciérrate y avísame cuando termine"
  │
  ▼
LLM registra la dirección en `watched`
  │  (herdr_watch_agent, voice_close_and_watch o notify_on_done)
  │
  ├─ si pidió cerrar: el STT/notch se apaga al terminar de hablar
  │
  ▼
Watcher: `hc ls --json` cada 1 s
  │
  ├─ working → done / blocked / error
  └─ o el terminal desaparece
  │
  ▼
done_prompt
  │
  ├─ si la llamada estaba cerrada: se reabre sola (STT + notch)
  ├─ se añade la salida reciente al contexto del LLM
  └─ Kokoro pregunta:
       "He terminado la tarea. ¿Quieres que te diga la salida,
        te resuma lo que hizo, o quieres darme otra instrucción?"
  │
  ▼
El usuario responde por voz:
  "dime la salida" / "resume" / "haz esto ahora" / "deja de escuchar"
```

### 3. Consultar estado

```
"¿Qué está haciendo el agente de OpenCode?"
  │
  ▼
LLM tools:
  herdr_list_agents  → hc ls --json
  herdr_get_agent    → hc get t...
  herdr_read_agent   → hc read t... --lines 40
  │
  ▼
El LLM resume el buffer; Kokoro lo dice.
```

### 4. GUI con CUA

```
"Con CUA driver abre Brave, entra en YouTube y reproduce un vídeo"
  │
  ▼
LLM: herdr_run_agent(..., gui=true, background=false)
  │
  ├─ Herdr abre pestaña visible con el agente
  └─ la tarea lleva delante:
       "Trabaja de forma visible... Usa la skill cua-driver..."
  │
  ▼
El agente usa cua-driver; Brave/YouTube se ven en el escritorio.
```

### 5. Segundo plano explícito

Solo cuando el usuario dice “en segundo plano”, “sin mostrarlo” o “no quiero
verlo”:

```
herdr_run_agent(background=true)
  → hc run --json --bg --no-parent ...
  → hc send <terminal> "<tarea>"
```

El terminal existe en el core, pero no se abre pestaña visible en la app.

## El mensajero no es un mod de Herdr

Es deliberado:

- Un mod de Herdr vive **dentro** de la ventana de Herdr y solo recibe teclas
  cuando Herdr tiene el foco.
- El mensajero necesita un **hotkey global** (`Ctrl+Alt+Space`, `Ctrl+Alt+E`,
  `Ctrl+Alt+M`, `Ctrl+Alt+O`, `Ctrl+Alt+R`) y un **notch always-on-top** sobre
  cualquier app.
- Por eso el notch es una ventana Win32 layered (la capa de Dicta) y el audio
  corre en un proceso Python externo.

## La isla como mando

La isla es una ventana Win32 layered **interactiva** (`click_through=False`).
Cerrada mide **169×36** en horizontal o **36×186** acoplada a un lateral; al
abrirse, la misma ventana contiene el menú completo y la pestaña se ensancha
**con** él hasta **320×596** (o **356×560** en lateral, creciendo hacia dentro)
alrededor del ancla, hacia el lado con más espacio. La barra y el panel comparten una sola silueta, sin escalón. No hay
`Toplevel`, tooltips ni ventanas secundarias. El WndProc y los hooks de puntero
traducen clics, arrastre de la isla, arrastre de sliders y rueda; la ventana
no se activa en reposo (`WS_EX_NOACTIVATE` + `MA_NOACTIVATE`) y solo la edición
de texto pide el teclado temporalmente.

| Zona | Acción |
|---|---|
| Micrófono | mute local: micrófono tachado y en rojo (`ERR`) |
| Flecha | abre o cierra el menú en la misma ventana |
| Cuerpo de la pestaña | arrastra la isla: píldora flotante, acople arriba/abajo/laterales a ≤12 px del borde |
| Menú | AUDIO, MOTOR, TTS, VAD, WHISPER · STT, LLM · API y CONVERSACIÓN + pie Guardar |
| «–» del menú | oculta la isla hasta la próxima llamada o atajo |

La isla ya no tiene botón de llamada: el mensajero entra en escucha solo a los 600 ms
de arrancar y `Ctrl+Alt+Space` sirve para colgar o retomar. Los ojos de dos
cápsulas son la única marca y quedan centradas en la isla (cx=86,5); la
flecha del menú es un chevrón desnudo a la derecha. **La isla no tiene texto**: el estado se lee por la forma,
el tamaño y el color de los ojos; la palabra (ESPERA, ESCUCHO, PIENSO, HABLO,
AVISO, PAUSA, ERROR) vive en el menú. La onda de cinco cápsulas solo existe en
HABLO. Los iconos son vectores dibujados por el proyecto; la tipografía del
menú es **Geist / Geist Mono** y los colores salen de `DESIGN.md` (grafito
`#0E0F13`, grupos `#1C1C1F`, mint `#5EEAC4`, ámbar `#FFC24A`, rojo
`#FF453A`). El tema sigue a Windows o se fija en `voice.json`.

Al arrastrar el cuerpo, la pestaña se convierte en píldora; al soltarla dentro
de los 12 px del borde superior o del inferior útil, se acopla con el cuello
hacia ese borde (`render_surface` con `attachment="top"|"bottom"|"float"`).
`clamp_position` la mantiene dentro del monitor y por encima de la barra de
tareas. El menú elige arriba o abajo con `menu_geometry`; si no cabe, limita el
cuerpo y activa el scroll.

El menú guarda en `voice.json` con escritura atómica y merge de claves. Los
umbrales del VAD se leen en cada bloque de audio, así que se aplican en vivo;
cambiar de micrófono reabre el stream. El VAD arranca en `min_rms=0.0006` y
solo actualiza su suelo de ruido con niveles cercanos al suelo; una frase floja
que aún no cruzó el umbral no se aprende como ruido. El arranque exige tres
bloques de 30 ms y conserva 0,6 s de pre-roll; `min_speech_secs=0.2` deja pasar
órdenes cortas. Tras el TTS hay una ventana de eco de 0,25 s, pero una frase
que empieza dentro y sigue después no se descarta. No hay panel Tk ni segunda ventana de ajustes.

## Latencia

El camino crítico es:

```
fin de voz → 1,3 s de silencio → whisper → primer token LLM → primera frase Kokoro
```

- `silence_secs=1.3` por defecto en el mensajero, ajustable en el menú.
- Kokoro se precalienta al arrancar.
- Mute, desmute, cerrar y repetir se resuelven con atajos locales, sin LLM.
- El LLM va en streaming y el TTS habla por frases.
- El log mide `STT`, `Primer audio`, `Turno completo` y la línea `Latencia` por turno; el protocolo de 20 frases está en `docs/voice-latency.md`.

## Mod de Herdr (futuro)

Un mod de Herdr **sí encajaría** como panel de control dentro de la app:

```
mods/ezagent-voice/
  main.js     # barra/panel: estado, mute, último aviso, botones
  mod.json    # place: right | top | none
```

Ese mod podría usar `agents()`, `notify()`, `run()` y `fetch` para hablar con
el mensajero o con `hc`; sería una **tercera vista** del mismo core, no el
motor de voz. El motor de voz sigue fuera para conservar hotkeys globales,
audio y LLM.

## Modelo de concurrencia

El proceso del mensajero tiene varios hilos:

| Hilo | Función |
|---|---|
| Tk main loop | Máquina de estados, eventos de UI, cola de avisos |
| Hotkeys globales | `RegisterHotKey` → cola de eventos |
| Captura de micrófono | Stream continuo, VAD, segmentación, barge-in |
| STT | whisper.cpp HTTP |
| LLM | OpenCode streaming + tool calling |
| TTS | Kokoro + PyAudio, frase a frase |
| Watcher | `hc ls --json` cada 1 s |
| Notch | Render nativo Win32 a 60 fps |

Los hilos de fondo no tocan Tk: ponen eventos en una cola; el hilo principal
los procesa. El notch lee variables atómicas (estado, RMS, mute) y dibuja.

### Generaciones de turno

Cada intervención incrementa `_turn_gen`. Los eventos de STT/LLM/TTS llevan su
generación; si el usuario interrumpe o se cierra la escucha, los eventos viejos
se descartan. Eso evita que una respuesta antigua hable encima de una nueva.

### Eco y mute

- El micrófono puede oír el TTS del propio asistente (TA82).
- Se usa un umbral de barge-in por encima del eco, se resetea el VAD al acabar
  cada TTS y se ignoran 0,5 s de cola.
- `Ctrl+Alt+M` mutea el micrófono; el notch muestra un micrófono con barra roja
  y el audio de fondo no se transcribe.

## Seguridad y consistencia

- El mensajero solo llama a `hc` y `herdr-app`; no ejecuta shell arbitrario.
- Lanzar, enviar o matar se ejecuta directamente ante una orden explícita; la respuesta es una frase corta con qué se abre y dónde.
- `background=true` solo si el usuario lo pide.
- El watcher solo vigila agentes pedidos (`announce_all=false`).
- El contexto del aviso incluye la salida reciente; no se resume hasta que el
  usuario lo pide.

## Límites conocidos

- El STT es por turnos (VAD + whisper); el streaming es del LLM/TTS.
- El watcher es polling de 1 s, no un bus de eventos push.
- El notch y los hotkeys son Windows-only.
- Un mod de barra de Herdr para control visual es una iteración futura.

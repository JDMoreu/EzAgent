# Reproducción de punta a punta (`parity/replay/`)

Arranca el `VoiceApp` **real** de Python (`src/ezagent/voice/app.py`) sin ventana
ni hardware y con los servicios externos sustituidos por los falsos de
`rust/crates/ez-fakes`. Cada escenario termina con una traza normalizada
determinista (`trace.jsonl`) y unas aserciones de alto nivel.

El oráculo es el Python de esta rama: el arnés no importa ni ejecuta nada de
Rust salvo los binarios falsos. `src/` y `tests/` no se modifican.

## Requisitos

```
cd rust && cargo build -p ez-fakes --bins
```

- Python del proyecto: `C:/Users/JDMoreu/Documents/EzAgent/.venv/Scripts/python.exe`
  (tiene `pyyaml`, `kokoro_onnx`, `numpy` y el resto de dependencias).
- Modelo de Kokoro instalado solo para **regenerar** los WAV de entrada; el
  arnés no sintetiza voz (el altavoz es un doble).
- `parity/replay/audio/*.wav` ya está versionado: PCM16 mono 16 kHz, generado
  una vez con Kokoro (`em_alex`) y normalizado a un pico fijo. `ruido.wav` es
  ruido blanco con semilla fija y RMS 0,0002; `eco_te_cuento.wav` es una frase
  de Kokoro atenuada a RMS 0,0030 (por encima del VAD y por debajo del umbral
  de barge-in).

## Órdenes

```powershell
# corre todos los escenarios y reescribe parity/replay/golden/*.trace.jsonl
python parity/replay/run_py.py --all

# uno solo (la traza va a la ruta indicada)
python parity/replay/run_py.py --scenario 02 --trace-out C:/tmp/02.jsonl

# lista los escenarios
python parity/replay/run_py.py --list

# determinismo: dos corridas por escenario, en procesos distintos, byte a byte
python parity/replay/run_py.py --verify
```

Cada escenario se ejecuta en su **propio proceso** (también con `--all`):
los parches son de módulo y los falsos abren puertos, así que no se pueden
mezclar entre corridas. `--verify` compara los bytes de las dos trazas; hoy
los seis escenarios salen idénticos.

Si `fake-llm`/`fake-whisper` no existen, el arnés falla con la orden de
compilación. Los puertos se piden al sistema (nunca fijos) y los procesos se
cierran siempre en `finally`.

## Costuras que se sustituyen (sin tocar `src/`)

| Pieza real | Sustituto | Qué se registra |
|---|---|---|
| `tkinter.Tk` | `FakeRoot` | bombea `after()` en el hilo principal; el guion del YAML corre dentro de `mainloop()` |
| `hud.drop.create_ui` | `RecordingHud` | `hud.show` (cambio de estado), `hud.in_call`, `hud.muted`, `hud.notice`, confirmaciones, cierre |
| `audio.MicCapture` | `ReplayMic` | cola de segmentos WAV/silencio/ruido a 30 ms por bloque; usa el `Segmenter` y el `BargeInDetector` reales |
| `tts.Speaker` | `ReplaySpeaker` | frases dichas y corte; duración simulada `max(min_secs, 0.045·len(texto))`, con `min_secs` configurable por escenario |
| `stt.transcribe` | envoltorio sobre el real | texto devuelto por `fake-whisper` |
| `herdr.GlobalHotkey` | `NullHotkey` | los atajos globales se inyectan como eventos (`hotkey:`) |
| `VoiceApp._create_swarm` | `None` | el enjambre no arranca su servidor |
| `VoiceApp._create_tray` | `None` | sin icono de bandeja |
| `VoiceApp._warm_stt` | no-op | evita consumir una transcripción del guion antes del primer turno |

Además se envuelven, sobre la instancia ya creada: `ChatModel` (`chat.user`,
`chat.assistant`, `chat.cancel`, `chat.fail`, `chat.note`, confirmaciones y
escrituras de `chat.jsonl`), `AgentRuntime.execute` (`tool.call`/`tool.result`)
y `VoiceConfig._write_json` (`file.write`).

El LLM se apunta con variables de entorno y `voice.json` a `fake-llm`
(`LLM_PROVIDER=custom`, `LLM_API_STYLE=openai`, `LLM_BASE_URL=…/v1`,
`LLM_MODEL=fake-model`); whisper con `WHISPER_URL=…/inference`; y `hc_path` /
`herdr_app_path` al `fake-hc.exe` absoluto. `auto_start_whisper` queda en
`false` y el micrófono real nunca se abre.

## Formato del escenario (YAML)

```yaml
id: "07"                     # obligatorio; también se acepta el nombre del archivo
name: "Delegar a OpenCode"   # descripción corta
notes: >                     # opcional, para humanos
  Qué mide el escenario.
timeout: 60                  # segundos de reloj; por defecto 60

settings:                    # se mezcla sobre los ajustes base del arnés
  silence_secs: 1.0
  barge_in_rms: 0.008
  speech_secs_min: 1.8       # mínimo de reproducción simulada por frase
  min_rms: 0.0006
  wake_phrase: "EzAgent, escúchame"

llm:                         # guion exacto de fake-llm (JSON inline)
  model: fake-model
  chunk_chars: 8
  turns:
    - content: "Voy a abrirlo."
    - status: 429
      retry_after: 2
  fallback: {content: "Fin del guion."}

whisper:                     # guion exacto de fake-whisper
  transcripts: ["Abre el proyecto."]
  by_hash: {}
  repeat_last: false

hc:                          # estado inicial de fake-hc (opcional)
  terminals: []

agent:                       # guion de fake-agent para F1-12b/c (opcional)
  scenes: [working, done]

steps:                       # guion del arnés, en orden
  - wait_state: listening
  - audio: hola_como_estas.wav
  - wait_state: speaking
  - audio: espera_para.wav
  - wait_quiet: 30

expect:                      # aserciones de alto nivel
  states: [thinking, speaking, listening]   # subsecuencia de hud.show
  user: ["Hola, ¿cómo estás?"]             # mensajes del chat, en orden
  assistant: ["Claro, aquí estoy."]
  speech_contains: ["Claro, aquí estoy."]   # cada texto, presente
  speech_count: {"Claro, aquí estoy.": 1}   # repeticiones exactas
  tool_calls: ["get_datetime {}"]           # "nombre argumentos_json"
  llm_requests: 1                           # peticiones de completación
  never: ["ESTO ES UN ECO."]                # no debe aparecer en voz ni chat
  final: {state: listening, in_call: true, muted: false}
```

### Pasos

| Paso | Efecto |
|---|---|
| `audio: archivo.wav` | encola el WAV en el micrófono; `gain:` opcional multiplica la muestra |
| `silence: 1.5` | encola silencio explícito (sin cola también se entrega silencio) |
| `noise: 1.0` | encola `ruido.wav` repetido hasta cubrir los segundos |
| `hotkey: toggle` | publica un evento de atajo global (`toggle`, `cancel`, `repeat`, `mute`, `menu`, `chat`) |
| `wait_state: speaking` | espera al estado del HUD |
| `wait_requests: 1` | espera a que `requests.jsonl` de `fake-llm` tenga N peticiones |
| `wait_quiet: 30` | espera a `listening`, chat sin turno, micrófono sin audio ni VAD y sin hilos de turno durante 250 ms; el número es el plazo |
| `sleep: 0.5` | espera de reloj |

Con la cola del micrófono vacía se sigue inyectando silencio a 30 ms por
bloque, así que una frase se cierra por el `silence_secs` normal del VAD.

### Aserciones

Se evalúan al terminar el guion; el proceso sale con código 1 si alguna falla.
`states` es una subsecuencia (no exige que no haya otros estados entre medias).
`speech_count` cuenta frases **reproducidas** (una frase partida por la
segmentación del TTS cuenta como dos). `never` busca en mensajes, voz y
transcripciones.

## Traza (`golden/*.trace.jsonl`)

Una línea JSON por evento:

```json
{"thread": "ezagent-answer#0", "seq": 2, "event": "tool.call", "name": "get_datetime", "args": "{}"}
```

- `thread` es `<nombre>#<ordinal>`; `seq` es el orden **dentro de ese hilo**.
  El archivo agrupa por hilo en este orden: `main`, `ezagent-call`,
  `ezagent-stt`, `ezagent-answer`, `ezagent-speech-synth`, `ezagent-speech`,
  `ezagent-tts`, `ezagent-busy`, `ezagent-wake`; los desconocidos, al final.
  Así la comparación es el orden causal de cada hilo, no el entrehilo.
- Nada volátil: sin relojes, UUID, puertos ni rutas absolutas (el directorio
  temporal de la corrida se sustituye por `<tmp>`). El tamaño de una locución
  del VAD tampoco se anota: puede variar en un bloque de 30 ms y no aporta.
- `seq` y el agrupado hacen que `--verify` compare bytes idénticos.

Eventos que emite el arnés:

| Evento | Campos | Origen |
|---|---|---|
| `app.start` / `app.end` | `scenario` / `state`, `in_call`, `muted` | arnés |
| `driver.step` | `index`, `step` | guion |
| `hud.show` | `state` | cambio de estado real |
| `hud.in_call`, `hud.muted` | `value` | solo cambios |
| `hud.notice`, `hud.confirm`, `hud.confirm_done`, `hud.testing`, `hud.hide`, `hud.close` | — / `summary`, `strong` | HUD |
| `chat.user` | `text`, `source` | mensaje del usuario |
| `chat.assistant` | `text` | respuesta completa (también si se cortó) |
| `chat.cancel`, `chat.fail`, `chat.note` | `text` | chat |
| `chat.snapshot` | `role`, `text` | volcado final, en orden |
| `file.write` | `name` (`voice.json`, `chat.jsonl`), `role` en chat | escritura real |
| `vad.speech_start`, `vad.barge_in`, `vad.utterance`, `vad.discarded` | `reason` en la locución | hilo del micrófono |
| `stt.transcript` | `text` | respuesta de `fake-whisper` |
| `speech.say`, `speech.cut` | `text` | altavoz doble |
| `tool.call` / `tool.result` | `name`, `args` / `ok`, `outcome` | núcleo agéntico |
| `audit.line` | `tool`, `level`, `decision`, `ok`, `taint`, `args` | `audit.jsonl` proyectado (sin `ts`, `turn_id` ni `ms`) |
| `llm.requests` | `count` | líneas de `requests.jsonl` del falso |

## Escenarios de esta tarea

| # | Archivo | Qué mide |
|---|---|---|
| 1 | `01-greeting.yaml` | saludo al LLM, respuesta hablada y ruido de sala que no despierta al VAD |
| 2 | `02-time-tool.yaml` | `get_datetime` real, resultado al LLM y respuesta; línea de auditoría |
| 3 | `03-local-shortcuts.yaml` | «Repite», «Ok, gracias. Mutéate» (cortesía), reactivación, «Cuelga» y atajos de llamada/corte |
| 4 | `04-split-phrase.yaml` | frase partida: el segundo trozo llega en `thinking` y se transcribe unido; el primer turno del LLM se cancela |
| 5 | `05-barge-in.yaml` | barge-in sostenido: corta la frase, invalida el turno y abre otro |
| 6 | `06-echo-discarded.yaml` | eco flojo durante el TTS: ni `stt.transcript` ni mensaje de usuario |

## Decisiones y límites

- **Sin Tk ni HUD reales.** Todo lo que la app pide a la interfaz pasa por
  `RecordingHud`; los métodos no registrados son no-ops.
- **La voz se simula.** El altavoz doble no reproduce audio: anota la frase,
  simula su duración y respeta `stop_event` igual que `Speaker.play`. Los
  tiempos exactos de Kokoro no importan para estos escenarios.
- **`chat_voice: true` por defecto en el arnés.** En el Python, el interruptor
  «Responder por voz» también decide si los turnos de micrófono se dicen
  (`_answer_worker`); con el valor de fábrica (`false`) la app no hablaría y no
  se podría medir la voz. Es un ajuste del escenario, no un cambio de `src/`.
- **`smart_turn: false`.** Evita cargar el modelo ONNX; el VAD por silencio es
  el que se mide aquí.
- **`_warm_stt` desactivado.** El precalentamiento real consultaría a whisper
  antes del primer turno y consumiría una transcripción del guion.
- **Sin enjambre, bandeja ni atajos globales reales.** Se sustituyen por
  no-ops o eventos; los escenarios 7-25 podrán levantar el enjambre con su
  propio estado.
- **Un proceso por escenario** (ver arriba) y `parity/gen/manifest.json` no se
  toca.

## Para F1-12b / F1-12c

1. Añade `parity/replay/scenarios/NN-nombre.yaml` con audio ya existente en
   `parity/replay/audio/` o un WAV nuevo (PCM16 mono 16 kHz) generado con
   Kokoro y el mismo normalizado.
2. Usa `hc:` para el estado inicial de `fake-hc` y `agent:` para el guion de
   `fake-agent`; el arnés escribe ambos en el directorio temporal de la corrida.
3. Si necesitas una espera nueva, añádela como paso del `Driver` (y documéntala
   aquí); si necesitas una aserción nueva, añádela a `check_expect`.
4. Ejecuta `--scenario NN` y luego `--verify` antes de dar el escenario por
   bueno.

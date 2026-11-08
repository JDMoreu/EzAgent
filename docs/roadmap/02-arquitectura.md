# 02 · Arquitectura objetivo

Estado al que se llega al final de E5. E2 y E3 construyen las piezas dentro de
la estructura actual; E5 las reubica. Los nombres de módulos son vinculantes
para los briefs.

## 1. Vista general

```
┌──────────────────────── proceso: ezagent (Python) ────────────────────────┐
│                                                                            │
│  platform/win  hotkeys · tray · single-instance                            │
│        │ comandos                                                          │
│        ▼                                                                   │
│  core.session ──────── core.events (EventBus tipado) ───────► ui.bridge ───┼──► notch (proceso UI)
│   máquina de estados        ▲            ▲            ▲        WebSocket   │     Python (actual) o
│        │                    │            │            │        127.0.0.1   │     Tauri/WebView2 (E6)
│        ▼                    │            │            │                    │
│  core.turn  ◄── voice.pipeline ──► llm.turn ──► agents.service             │
│   (un turno)    captura→VAD→STT     router→LLM    backends: Herdr | Direct │
│                 TTS→reproducción    narración     watcher · tasks          │
└────────────────────────────────────────────────────────────────────────────┘
        │ HTTP local                    │ HTTPS                   │ subprocess (lista de args)
        ▼                               ▼                         ▼
  whisper-server (whisper.cpp)    proveedor LLM            claude · opencode · codex · hc
```

## 2. Árbol de módulos objetivo

```
src/ezagent/
  core/
    events.py        dataclasses de eventos + EventBus (thread-safe, colas)
    state.py         máquina de estados pura (sustituye la lógica de app.py)
    session.py       ciclo de vida: arranque, llamada, mute, apagado
    turn.py          coordina un turno: utterance → texto → respuesta → audio
    config.py        VoiceConfig + AgentConfig unificados, carga/guardado
    i18n/            es.toml, en.toml + t(key, **kw)
  voice/
    capture.py       PyAudio entrada, hilo propio, cola de chunks
    vad.py           Silero VAD (ONNX) + fallback por energía
    endpoint.py      decisión de fin de turno (silencio + heurística semántica)
    stt.py           cliente whisper.cpp con httpx.Client persistente
    tts/engine.py    Kokoro: síntesis en hilo propio, cola de frases
    tts/player.py    stream de salida persistente, cola de PCM, stop inmediato
    earcons.py       sonidos de interfaz sintetizados
    pipeline.py      conecta capture→vad→endpoint→stt y tts→player
  llm/
    client.py        cliente OpenAI-compatible persistente, warm-up, cancelación
    providers.py     (de llm_providers.py)
    prompt.py        prompt estable + bloque <contexto> variable
    router.py        intención local: comando | charla | acción
    tools.py         esquemas de las 6 herramientas
    parser.py        tool-calls nativos + respaldo desde texto + coerción
    narrate.py       plantillas de narración y acuse (i18n)
    turn.py          bucle de 1–2 rondas con streaming
  agents/
    backend.py       Protocol AgentBackend + tipos
    herdr.py         HerdrBackend (de voice/herdr.py)
    direct.py        DirectBackend (subprocess + logs)
    registry.py      carga agents.toml, detección en PATH
    resolve.py       resolución difusa de agente/proyecto/alias
    service.py       API usada por las herramientas; estado de sesión
    watcher.py       eventos de fin/bloqueo; polling o stream según backend
    tasks.py         tareas pendientes y cadenas input_from
  memory/
    profile.py       USER.md (perfil corto)
    projects.py      projects.toml (alias → carpeta)
  ui/
    protocol.py      esquema de mensajes versionado
    bridge.py        servidor WebSocket local con token
  notch/             cliente UI Python actual (hasta que E6 decida)
  platform/win/      hotkeys.py, tray.py, focus.py, single_instance.py
  app.py             punto de entrada: arma session + bridge + UI (≤ 200 l.)
```

Regla de dependencias (se comprueba con una prueba de imports en E5):

```
platform ─► core ◄─ voice
              ▲  ◄─ llm ─► agents ─► memory
              └──── ui
```

- `core` no importa nada de `voice`, `llm`, `agents`, `ui` ni `notch`.
- `agents` no importa `voice` ni `ui`.
- `notch` solo habla con el núcleo por `ui.protocol`.

## 3. Modelo de concurrencia

| Hilo / tarea | Responsabilidad | Regla |
|---|---|---|
| `capture` (hilo) | Lee el micrófono en chunks de 30 ms | Nunca bloquea en nada más que `stream.read` |
| `vad` (hilo) | VAD + endpoint, emite `SpeechStart`, `SpeechEnd`, `Utterance` | Sin E/S de red |
| `core` (event loop asyncio) | Estado, turnos, STT, LLM, agentes | Toda E/S de red y `subprocess` es async |
| `tts-synth` (hilo) | Kokoro: frase → PCM | ONNX libera el GIL; cola de entrada acotada |
| `tts-play` (hilo) | Stream de salida persistente, bloques de 20 ms | Revisa el `stop` en cada bloque |
| `ui-bridge` (asyncio) | WebSocket; publica eventos, recibe comandos | Limita los eventos `Level` a 30 Hz |
| UI (proceso aparte) | Render y entrada | Nunca ejecuta lógica de negocio |

La cancelación es por **generación de turno**: cada turno tiene un
`TurnContext(id, cancel: asyncio.Event, stop_audio: threading.Event)` creado
por `core.turn` e inmutable. El barge-in cancela el contexto activo; nadie
reasigna eventos compartidos (corrige R1).

## 4. Turno objetivo (línea temporal)

```
t=0     el usuario deja de hablar
+0..250 VAD Silero confirma silencio corto ─► earcon «escuché» (opcional) + ojos PIENSO
+250    STT especulativo (si el usuario sigue hablando, se descarta)
+600    endpoint confirma fin (600–700 ms; +400 ms si el texto parcial queda abierto)
+650    transcripción lista (whisper turbo q5, GPU)
        router local:
          comando ─► acción local + plantilla ─► TTS            (≤ 300 ms)
          charla  ─► LLM sin herramientas, streaming ─► TTS por cláusulas
          acción  ─► LLM con 6 herramientas
                     └ tool-call detectado ─► acuse con plantilla al TTS (en paralelo)
                                           ─► agents.service ejecuta
                                           ─► narración con plantilla (sin 2.ª llamada)
+900    primer sonido (objetivo P50)
```

## 5. Backends de agentes

```python
# src/ezagent/agents/backend.py
from typing import Protocol, AsyncIterator, Literal
from dataclasses import dataclass

AgentState = Literal["starting", "working", "idle", "blocked", "done", "error", "gone"]

@dataclass(frozen=True)
class RunSpec:
    agent: str            # clave de agents.toml, p. ej. "opencode"
    task: str
    cwd: str              # ya resuelto por agents.resolve
    model: str | None = None
    effort: str | None = None
    visible: bool = True  # ventana visible vs segundo plano
    input_text: str | None = None  # salida de otro agente (input_from)

@dataclass(frozen=True)
class AgentHandle:
    id: str               # dirección Herdr o id de ejecución directa
    agent: str
    project: str
    task: str

@dataclass(frozen=True)
class AgentStatus:
    handle: AgentHandle
    state: AgentState
    last_line: str = ""

@dataclass(frozen=True)
class AgentEvent:
    handle: AgentHandle
    state: AgentState
    detail: str = ""

class AgentBackend(Protocol):
    name: str
    def available(self) -> bool: ...
    async def run(self, spec: RunSpec) -> AgentHandle: ...
    async def send(self, handle: AgentHandle, text: str) -> None: ...
    async def status(self) -> list[AgentStatus]: ...
    async def read_output(self, handle: AgentHandle, lines: int = 60) -> str: ...
    async def stop(self, handle: AgentHandle) -> None: ...
    def events(self) -> AsyncIterator[AgentEvent]: ...
```

### HerdrBackend
Envuelve `voice/herdr.py` y `voice/watcher.py` actuales; sin cambios de
comportamiento. `available()` = `hc` en PATH y respondiendo.

### DirectBackend
- **Segundo plano:** `subprocess` con argumentos de `agents.toml`
  (`headless_args`), salida a `%LOCALAPPDATA%\EzAgent\runs\<id>.log`, estado por
  código de salida y por patrones de bloqueo configurables.
- **Visible:** `wt.exe new-tab --title <agente·proyecto> -d <cwd> <interactive_args>`
  (si `wt.exe` no existe, `conhost`). En modo visible, `read_output` y `send` no
  están disponibles: la narración lo dice («Está en una pestaña visible; no
  puedo leerla sin Herdr»).
- Los flags concretos de cada CLI **no se codifican**: van en `agents.toml`
  porque cambian entre versiones.

```toml
# %APPDATA%\EzAgent\agents.toml (plantilla por defecto)
default_agent = "opencode"
backend = "auto"          # auto | herdr | direct

[agents.opencode]
label = "OpenCode"
aliases = ["open code", "opencode"]
command = ["opencode"]
headless_args = ["run", "{task}"]
interactive_args = []
model_args = ["--model", "{model}"]
models = ["deepseek-v4.1-flash"]
blocked_patterns = ["(?i)permission", "(?i)approve"]

[agents.claude]
label = "Claude Code"
aliases = ["claude", "claude code", "cloud code"]
command = ["claude"]
headless_args = ["-p", "{task}"]
model_args = ["--model", "{model}"]
effort_args = ["--effort", "{effort}"]

[agents.codex]
label = "Codex"
aliases = ["codex", "códex"]
command = ["codex"]
headless_args = ["exec", "{task}"]
effort_args = ["-c", "model_reasoning_effort={effort}"]
```

```toml
# %APPDATA%\EzAgent\projects.toml
[projects.ezagent]
path = "C:/Users/<user>/Documents/EzAgent"
aliases = ["ez agent", "easy agent", "el agente"]
```

`agents.resolve` normaliza (minúsculas, sin acentos), compara con los alias y
usa similitud (`difflib.SequenceMatcher`, umbral 0,75) para absorber errores de
whisper («cloud code» → claude). Si hay empate o nada supera el umbral, la
herramienta devuelve `need_clarification` con las opciones y la plantilla
pregunta.

## 6. Protocolo UI (resumen; el esquema completo se escribe en E5)

Transporte: WebSocket en `127.0.0.1:<puerto libre>`. El token se pasa al
proceso UI por variable de entorno. Mensajes JSON con `{"v":1,"type":...}`.

| Dirección | Tipo | Campos |
|---|---|---|
| núcleo → UI | `state` | `state` (idle/listen/think/speak/notice/mute/error/off), `muted`, `listening` |
| núcleo → UI | `level` | `mic`, `voice` (0–1), máx. 30 Hz |
| núcleo → UI | `agent` | `id`, `agent`, `project`, `state` |
| núcleo → UI | `confirm` | `id`, `summary`, `strong` |
| núcleo → UI | `settings` | instantánea de ajustes + esquema de controles |
| núcleo → UI | `toast` | `kind`, `text` (errores de configuración) |
| UI → núcleo | `cmd` | `toggle_call`, `toggle_mute`, `open_menu`, `confirm_answer`, `set_setting`, `save_settings`, `test_voice`, `quit` |

La UI es un cliente «tonto»: deriva la pose de los ojos de `state` + `agent` +
`level` con la función pura actual de `notch/state.py`.

## 7. Configuración y datos

| Archivo | Contenido | Quién escribe |
|---|---|---|
| `%APPDATA%\EzAgent\voice.json` | Ajustes de voz, audio, UI | menú / usuario |
| `%APPDATA%\EzAgent\llm.json` | Perfiles de proveedor | menú / usuario |
| `%APPDATA%\EzAgent\agents.toml` | Agentes y flags | usuario |
| `%APPDATA%\EzAgent\projects.toml` | Proyectos y alias | usuario / `remember` |
| `%APPDATA%\EzAgent\USER.md` | Perfil corto (≤ 800 caracteres) | `remember` con confirmación |
| `%APPDATA%\EzAgent\tasks.json` | Tareas pendientes | núcleo |
| `%APPDATA%\EzAgent\logs\` | `ezagent.log`, `audit.jsonl`, `latency.jsonl` | núcleo |
| `%LOCALAPPDATA%\EzAgent\models\` | whisper, Kokoro, Silero | instalador / descargas |
| `%LOCALAPPDATA%\EzAgent\whisper\` | binarios whisper.cpp | instalador / descargas |
| `%LOCALAPPDATA%\EzAgent\runs\` | logs de ejecuciones directas | DirectBackend |

Las migraciones de `voice.json` llevan campo `"schema": N` y funciones
`migrate_N_to_N1` probadas.

## 8. Seguridad del nuevo núcleo

- Solo se ejecutan los comandos de `agents.toml`, con argumentos en lista; la
  tarea va como **un solo argumento**, sin interpolarse en un shell.
- `cwd` siempre sale de `projects.toml`, de la carpeta actual de Herdr o de la
  carpeta de trabajo por defecto; nunca de texto libre sin resolver.
- Confirmación obligatoria: `stop_agent` y `send_to_agent` a un agente en estado
  `working`. Todo lo demás se ejecuta y se narra.
- La salida de agentes que se lee al LLM (resúmenes) va envuelta en
  `<agent_output>` y marcada como dato; nunca se ejecuta una herramienta en el
  mismo turno a partir de esa salida sin confirmar (versión mínima del taint actual).
- El servidor IPC rechaza conexiones sin token y orígenes no locales.

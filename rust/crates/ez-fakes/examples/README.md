# Servicios falsos (`ez-fakes`)

Los cuatro servicios los comparten la app Python y la de Rust: ninguno cambia
`src/`; solo se apunta la configuración a ellos.

```
cargo build -p ez-fakes --bins
```

| Binario | Qué imita | Escucha |
|---|---|---|
| `fake-llm` | OpenAI Chat Completions y Anthropic Messages (SSE) | `127.0.0.1:8790` |
| `fake-whisper` | `whisper-server` (`POST /inference`) | `127.0.0.1:8088` |
| `fake-hc` | `hc` y `herdr-app` | — (CLI) |
| `fake-agent` | Pantalla de un agente en un terminal | — (stdout) |

Todos aceptan `--help`.

## fake-llm

```
fake-llm --scenario examples/fake_llm.json --port 8790 --record requests.jsonl
```

Rutas:

- `POST …/chat/completions`: OpenAI. `stream: true` responde SSE y termina en
  `data: [DONE]`; sin `stream`, un `chat.completion` normal.
- `POST …/messages`: Anthropic Messages. Siempre eventos SSE con
  `message_start`, bloques, `message_delta` y `message_stop`.
- `GET /health` y `GET /v1/models`.

Escenario (`examples/fake_llm.json`):

| Campo | Efecto |
|---|---|
| `model` | Modelo por defecto si la petición no trae uno. |
| `chunk_chars` | Tamaño de cada `delta` de texto (también de los argumentos, ×2). |
| `turns[]` | Una respuesta por petición de completación, en orden. |
| `fallback` | Respuesta cuando se agotan los turnos (defecto: «Fin del guion.»). |

Cada turno:

| Campo | Efecto |
|---|---|
| `content` | Texto del asistente. |
| `tool_calls[]` | `{id, name, arguments}`; `arguments` es el JSON tal cual y puede estar roto a propósito. |
| `finish_reason` | `stop`, `length` o `tool_calls`; si falta se deduce. |
| `status` | Código HTTP de error (401, 429, 500…). `0`/ausente = 200. |
| `error`, `error_type`, `retry_after` | Cuerpo del error y cabecera `Retry-After`. |
| `max_chunks` | Emite solo los primeros N eventos SSE y cierra (corte a mitad). |
| `delay_ms` | Espera antes de responder. |
| `usage` | Objeto `usage` exacto del último chunk; si falta se inventa. |

Cada petición de completación se anota como una línea de `requests.jsonl`:
`n`, `route` (`openai`/`anthropic`), `method`, `path`, `stream`,
`has_api_key`, `headers` (solo `accept`, `anthropic-version`, `content-type`,
`user-agent`), `body` (JSON parseado) y `raw_body` (cuerpo exacto). Nunca se
escribe la clave de API.

Apuntar la app Python:

```
# OpenAI
LLM_PROVIDER=custom LLM_API_STYLE=openai \
LLM_BASE_URL=http://127.0.0.1:8790/v1 LLM_MODEL=fake-model LLM_API_KEY=sin-clave

# Anthropic
LLM_PROVIDER=custom LLM_API_STYLE=anthropic \
LLM_BASE_URL=http://127.0.0.1:8790 LLM_MODEL=fake-model LLM_API_KEY=sin-clave
```

## fake-whisper

```
fake-whisper --scenario examples/fake_whisper.json --port 8088
```

`POST /inference` (y `/v1/audio/transcriptions`) recibe
`multipart/form-data` con el campo `file` y responde `{"text": "…"}`.

| Campo del escenario | Efecto |
|---|---|
| `transcripts[]` | Textos en orden, uno por petición. |
| `by_hash` | SHA-256 hexadecimal del WAV → texto; gana al orden y no lo consume. |
| `repeat_last` | Repite el último texto al agotarse (defecto: `false`, texto vacío). |

`GET /health` responde `{"status":"ok"}`. La app Python solo comprueba que el
puerto TCP esté abierto (`ensure_whisper`), y se apunta con:

```
WHISPER_URL=http://127.0.0.1:8088/inference
```

y `auto_start_whisper: false` en `voice.json`.

## fake-hc

```
fake-hc --state C:/tmp/hc.json --seed examples/fake_hc.json ls --json
```

El estado es el JSON de `--state` (o `FAKE_HC_STATE`); si no existe, se copia
de `--seed`. Está protegido con `<estado>.lock` para que el sondeo del vigía y
una acción simultánea no se pisen.

Comandos: `ls [--space X]`, `spaces`, `tabs`, `get`, `read [--lines N]`,
`send`, `put`, `keys`, `kill`, `run`, `do tab.new`, `do agent.focus`. Las
salidas JSON y los códigos (0 correcto, 1 no encontrado, 2 uso) imitan a `hc`;
`read` devuelve texto plano.

Un terminal del seed puede traer `done_after_reads` (termina tras N lecturas) o
`done_after_ms` (termina N ms después del primer sondeo que lo ve `working`).
Al terminar pasa a `idle` y añade `conclusion` a la pantalla, así el `Watcher`
Python observa `working → idle`. También se puede editar el JSON entre llamadas
para forzar `blocked`, `error` o cualquier pantalla.

Apuntar la app Python: `hc_path` y `herdr_app_path` en `voice.json` con la ruta
absoluta de `fake-hc.exe`.

## fake-agent

```
fake-agent --scenario examples/fake_agent.json --agent claude
```

Imprime las pantallas del guion (`working`, `done`, `blocked`, `usage_limit`,
`context_full`) separadas por saltos de línea, con `delay_ms` entre ellas.
`--scene` se puede repetir para sustituir el guion; `--task` y `--conclusion`
cambian los textos. Las pantallas tienen varias líneas y no acaban en el prompt
de PowerShell, para que `wait_ready` acepte el arranque.

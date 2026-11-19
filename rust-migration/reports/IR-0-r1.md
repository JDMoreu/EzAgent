# IR-0 — Investigación: uso, límites, versión y modelos de cada agente sin gastar cuota

Estado: completado

Encargo de investigación solamente: **no se cambió código del producto**. Todo lo de abajo
se obtuvo con `--version`, listados locales y lectura de archivos locales. No se ejecutó
ningún agente con un mensaje, no se abrió ninguna sesión con modelo y no se llamó a
ninguna API de modelo.

Máquina: Windows 11 (10.0.26300 x64), PowerShell 5.1. Fecha de la investigación:
2026-11-19. Todos los "ejemplos" de abajo son **nombres de campo, tipos y agregados**
(sumas, mínimos, máximos, recuentos, ventanas). No hay claves, tokens, correos,
contenido de conversaciones ni rutas de proyectos de la persona.

---

## 0. Resumen ejecutivo

| Agente | Instalado | Versión | Executable nativo | Modelos sin gastar | Uso local | Límite/ventana local | Plan local |
|---|---|---|---|---|---|---|---|
| Claude Code | sí | `2.1.328` | `.exe` nativo | catálogo en caché + `settings.json` | tokens por mensaje | **no** (solo tokens) | sí (`claude_pro`) |
| OpenCode | sí | `v2.3.0` | `.exe` nativo (Bun) | `opencode models` + caché `kv` + `model.json` | `opencode stats --json` | **parcial**: solo en el log, sin % | parcial (solo en el mensaje de error) |
| MiniMax Code | sí | `0.7.6` | `node` + `cli.js` (sin `.exe`) | `config.yaml` + `provider list` | SQLite `local_runtime_token_usage` | **no** | parcial (proveedor OAuth gestionado) |
| Codex | sí | `codex-cli 0.169.0` | `.exe` nativo | `models_cache.json` + `config.toml` | rollout `.jsonl` | **sí**: `used_percent` + `resets_at` | sí (`plan_type`) |
| Qwen Code | **no** | — | — | — | — | — | — |
| Pi | sí | `0.85.1` | `node` + `cli.js` (sin `.exe`) | catálogos `pi-ai` + `--list-models` | `sessions/*.jsonl` → `message.usage` | **no** | no |

Los dos únicos agentes que dejan **porcentaje de ventana y fecha de reinicio** en disco
son **Codex** (`rate_limits`) y, de forma degradada, **OpenCode** (el mensaje de error
del log, sin porcentaje). Los demás solo permiten reconstruir **tokens gastados**, no
«qué tanto de la ventana llevo»: para ellos el estado de cuota será `desconocido`.

---

## 1. Claude Code (`claude`)

### 1.1 Executable nativo y versión

- Ruta real: `C:\Users\JDMoreu\.local\bin\claude.exe` (binario nativo de ~380 MB; no es
  un guion de npm). `where.exe claude` devuelve solo ese `.exe`.
- Versión: `claude.exe --version` → `2.1.328 (Claude Code)`.
- `~/.local/share/claude/versions/` guarda las versiones descargadas (el actual y un
  `.old.<ts>`), y `~/.claude/.last-update-result.json` trae
  `{timestamp, path, outcome, status, version_from, version_to, error_code}`.

### 1.2 Modelos y esfuerzos disponibles sin gastar

Dos fuentes locales, ambas sin red:

1. **Catálogo de modelos en caché** —
   `~/.claude/cache/model-catalog/<hash>-cc.json` (un solo archivo; el nombre lleva un
   hash, así que hay que *enumerar* la carpeta, no hardcodear el nombre).
   Forma: `{version:int, fetchedAt:int, staleAt:int, catalog:{surface, config:{…},
   state:{…}}}`. En `catalog.config`:
   - `models`: lista de 13 objetos con `id`, `name`, `short_name`, `description`,
     `section`, `capabilities{…}`, `thinking{…}`, `fast_mode{…}`, `quick_select:bool`,
     `runtime{…}`, `min_claude_code_version`.
   - `provider_alias_targets`: alias → `{default: <model id>, per_provider:{…}}`.
     En esta máquina: `fable→claude-fable-5-2`, `haiku→claude-haiku-5-5`,
     `opus→claude-opus-5-5`, `sonnet→claude-sonnet-5-5`.
   - `settings_vocabulary.effort_level`: el vocabulario de esfuerzo permitido
     (`high`, `low`, `max`, `medium`, `xhigh`).
   - `catalog.state`: `{model, selection_source, thinking:{type, effort}}` — el modelo y
     el esfuerzo que la persona tenía seleccionados en el momento de la caché.
   - `fetchedAt`/`staleAt` son **epoch en milisegundos**; la caché se refresca sola
     (`fetchedAt` 2026-11-19T13:55Z, `staleAt` 2026-11-19T14:58Z en la lectura).

2. **Configuración activa** — `~/.claude/settings.json`:
   - `model`: alias activo (`"opus"`).
   - `modelSettings.<model id>.effortLevel`: esfuerzo por modelo. Valores vistos:
     `high`, `xhigh`.
   - `autoUpdatesChannel`, `theme`, `enabledPlugins`, `autoMode.environment` (lista de
     25 cadenas).

3. **Costes por modelo** — `~/.claude.json` → `additionalModelCostsCache`: 14 ids de
   modelo → `{inputTokens:int, outputTokens:int, promptCacheWriteTokens:num,
   promptCacheWrite1hTokens:num, promptCacheReadTokens:num, webSearchRequests:num}`.
   Sirve para el nivel `costo` (`gratis` / `plan` / `pago`) de `modelos.toml` (IR-3).

### 1.3 Uso y límites observables

- **Ruta:** `~/.claude/projects/<slug-de-ruta>/<uuid-de-sesión>.jsonl` (81 archivos de
  sesión en esta máquina). Formato: un JSON por línea.
- **Tokens por mensaje:** en los registros de asistente, `message.usage`:
  `input_tokens:int`, `cache_creation_input_tokens:int`, `cache_read_input_tokens:int`,
  `output_tokens:int`, `output_tokens_details.thinking_tokens:int`,
  `cache_creation.ephemeral_1h_input_tokens:int`,
  `cache_creation.ephemeral_5m_input_tokens:int`, `service_tier:str`,
  `speed:str`, `server_tool_use.web_search_requests:int`,
  `server_tool_use.web_fetch_requests:int`, `inference_geo:str`, `iterations:list`,
  `fallback_credit` (normalmente `null`).
- **Modelo y esfuerzo por turno:** en el mismo registro, `message.model`,
  `requestedModel`, `advisorModel`, `effort`, `perTurnEffort`.
- **Agregado observado** (solo agregados, sin contenido): 36 573 registros de uso en
  81 sesiones; `input_tokens` 171 715 797, `output_tokens` 43 891 111,
  `cache_creation_input_tokens` 67 795 128, `cache_read_input_tokens` 15 260 019 765;
  `service_tier` = `standard` en 36 559 de 36 573; `fallback_credit` **no nulo en 0 casos**.
- **Recencia:** la sesión más reciente se modificó hoy (2026-11-19 12:37 local).

> **Hallazgo importante:** Claude Code **no persiste el porcentaje de la ventana de 5 h
> ni su fecha de reinicio en disco**. Un barrido de las 1 141 claves distintas de todos
> los `.jsonl` no encuentra ninguna clave de tipo `limit` / `reset` / `rate` / `window`
> / `quota`: lo único es `message.usage`. La UI de Claude Code sabe el porcentaje porque
> lo pide al servidor en cada llamada; ese dato no se cachea. Para Claude Code el
> estado de ventana será **siempre `desconocido`** (o «tokens gastados» si se prefiere
> un indicador pobre pero real).

- **Aviso de límite alcanzado:** el único rastro local es `message.usage.fallback_credit`
  (no nulo cuando se usó crédito extra/respaldo). En esta máquina siempre fue `null`,
> así que la señal es útil pero poco frecuente.

### 1.4 Plan o suscripción

De `~/.claude.json` → `oauthAccount` (20 campos). **Los que interesan, sin identidad**:

| Campo | Valor observado | Tipo |
|---|---|---|
| `organizationType` | `claude_pro` | str |
| `billingType` | `stripe_subscription` | str |
| `organizationRateLimitTier` | `default_claude_ai` | str |
| `organizationRole` | `admin` | str |
| `seatTier` | `null` | str\|null |
| `userRateLimitTier` | `null` | str\|null |
| `hasExtraUsageEnabled` | `false` | bool |
| `claudeCodeTrialEndsAt` | `null` | str\|null |
| `subscriptionCreatedAt` | ISO-8601 | str |

Además, en la raíz de `~/.claude.json`:
- `cachedExtraUsageDisabledReason`: `org_level_disabled` (str).
- `unusedLimitResetStatusCache`: `{accountUuid:str, holdsUnusedReset:bool,
  checkedAt:int, validUntil:int}` — epoch en **milisegundos**; marca que queda un
  reinicio de cuota sin usar.

> `oauthAccount` **también** trae `emailAddress`, `fullName`, `displayName`,
> `organizationName` y varios UUID: **no copiar ni registrar**. El lector debe leer
> solo las claves de plan listadas arriba.

---

## 2. OpenCode (`opencode`)

### 2.1 Executable nativo y versión

- Ruta real: `C:\Users\JDMoreu\AppData\Local\pi-node\current\node_modules\@opencode\cli\bin\opencode.exe`.
- **Trampa de HD-0c repetida:** en `C:\Users\JDMoreu\AppData\Local\pi-node\current\`
  conviven `opencode` (guion `#!/bin/sh` de npm, 298 B), `opencode.cmd` (shim que invoca
  el `.exe`) y `opencode.ps1`. `where.exe opencode` devuelve **los dos primeros**, y el
  que no tiene extensión es el guion de bash: ejecutarlo «a pelo» en Windows no
  arranca el binario real. Hay que resolver el `.exe` de dentro de
  `node_modules\@opencode\cli\bin\`.
- Versión: `opencode.exe --version` → `opencode v2.3.0`.
- El proceso de servicio deja su estado en `~/.local/state/opencode/service.json`
  (`{id, version, url, pid, password}`) — **el `password` es un secreto**: no leer ni
  copiar ese archivo.

### 2.2 Modelos y esfuerzos disponibles sin gastar

Tres fuentes:

1. **`opencode models`** (listado local; el catálogo viene de `models.dev`, así que la
   primera vez hace red, pero **no consume cuota de modelo**). Alternativa 100 % local:
   el propio OpenCode guarda la respuesta en caché en su base:
   `~/.local/share/opencode/opencode.db` → tabla `kv`, clave
   `models-dev:v2:catalog` (≈13 MB de JSON) y `models-dev:v2:catalog:meta`
   (`{updatedAt:int epoch ms, digest:str, etag:str}`). El `updatedAt` dice si la caché
   está fresca.
   > `opencode models` **no se ejecutó** en esta investigación (evita la llamada de
   > red a `models.dev`); se comprobó su existencia con `opencode --help` y se verificó
   > que la caché local equivalente existe y está al día. La caché `kv` es la vía
   > recomendada para `ez init` sin red.
2. **`~/.local/state/opencode/model.json`** (347 B) — el estado de modelos de la
   persona: `recent:[{providerID, modelID}]`, `favorite:[]`,
   `variant:{"<provider>/<model>": "<esfuerzo>"}`. El `variant` **es el nivel de
   esfuerzo** por modelo; valores vistos: `max`, `low`, `default`.
3. **`~/.config/opencode/opencode.jsonc`** — configuración del usuario (proveedores
   declarados). **No leer a ciegas**: puede traer credenciales en `options`.

### 2.3 Uso y límites observables

- **Uso (tokens y coste):** `opencode stats --standalone --json` (local, lee la base).
  Salida: `{range:{from:int ms, to:int ms}, sessions:int, subagents:int, prompts:int,
  steps:int, tokens:{input, output, reasoning, cache:{read, write}}, cost:float,
  tools:{mode, totals:{calls,succeeded,failed,unfinished}, usage:[{name, calls, …,
  durationP50}]}, activeDays:int, streak:int, activity:[{date:"YYYY-MM-DD",
  steps:int}], models:[{model:{id, providerID, variant}, steps, tokens, cost}]}`.
  - **Aviso operativo:** sin `--standalone` falla con `Background service failed to
    start` cuando el servicio de fondo no puede arrancar. Con `--standalone` funciona
    (arranca un servidor privado efímero). IR-2 debe usar siempre `--standalone` o
    `--server <url>`.
  - La base es grande: `opencode.db` ≈1,5 GB (+86 MB de WAL). Leer siempre con
    `sqlite3` en modo `ro` y solo las tablas necesarias; nunca copiar la base.
- **Límite / ventana:** **no está en la base**. Solo aparece en el log
  `~/.local/share/opencode/log/opencode.log` (≈22 MB, texto plano, se rota por
  fecha). Dos formatos observados:

  ```
  AI.Error: OpenCode Go usage limit reached (5-hour, resets in 4h 46m). To keep going now, turn on Extra Usage or upgrade to Go Plus at <url>
  AI.Error: OpenCode Go usage limit reached (monthly, resets in 10d 3h). …
  AI_APICallError: Free usage limit reached. Subscribe to OpenCode Go to keep going: https://opencode.ai/go
  ```

  Se puede extraer: **etiqueta de ventana** (`5-hour`, `monthly`; también aparece
  `weekly`), **cuenta atrás al reinicio** (`4h 46m`, `10d 3h`) y **plan** (`OpenCode
  Go`, `Go Plus`, `Free`). **No hay porcentaje**: no se puede saber «voy al 60 % de la
  ventana» sin consultar la API. Gana el último aviso del log.
- **Recencia:** log modificado hoy (2026-11-19 12:37). La caché `models-dev` se
  actualizó hoy (epoch ms `1795109767274`).

> La memoria del proyecto ya tiene una nota del propio usuario sobre este mismo
> punto (`opencode-quota-fallback.md`), lo que confirma que el aviso del log es la
> única señal local.

### 2.4 Plan o suscripción

Solo inferible del mensaje de error (`OpenCode Go` / `Go Plus` / `Free`). La base tiene
tablas `account`, `control_account`, `account_state` y `credential` con `email`,
`access_token`, `refresh_token`, `token_expiry` y `credential.value`: **son secretos,
no se leen**. La instalación usa además `~/.local/share/opencode/auth.json`
(no leer).

---

## 3. MiniMax Code (`mcode`)

### 3.1 Executable nativo y versión

- Ruta real: no hay `.exe`. La cadena es
  `C:\Users\JDMoreu\.minimax-code\mcode.cmd` → lee `C:\Users\JDMoreu\.minimax-code\current`
  (un número de release) → `C:\Users\JDMoreu\.minimax-code\releases\<release>\.mcode-launcher.cmd`
  → `node.exe <prefix>\node_modules\@minimax-ai\code\cli.js`.
- `C:\Users\JDMoreu\.minimax-code\install.json` describe el diseño:
  `{schemaVersion, product:"minimax-code", updateOwner:"npm-prefix",
  packageManager:"npm", packageName:"@minimax-ai/code", registry, distTag,
  npmExecutable, prefix, layoutVersion, releasesDirectory, currentFile}`.
- Versión: `node.exe <cli.js> --version` → `0.7.6` (release activo `0.7.6`).
  `mcode.cmd --version` da lo mismo.
- Nota: hay dos raíces de datos distintas — `~/.minimax-code` (instalación del CLI) y
  `~/.minimax` (datos del runtime, que es el `activeDataDir` de esta sesión).

### 3.2 Modelos y esfuerzos disponibles sin gastar

- **`~/.minimax/config.yaml`** (YAML, 17 KB):
  - `provider.minimax.models`: mapa de ids de modelo; `provider.minimax.model_order`
    (orden de preferencia) y `provider.minimax.whitelist` (lista).
  - `custom_provider.<id>.models`: mapa de ids por proveedor personalizado. Aquí:
    `opencode-go` (21 modelos), `opencode-go-responses` (5), `opencode-go-anthropic` (2).
  - `defaultModel`: `"<proveedor>/<modelo>"`.
  - `defaultModelThinking.effort`: nivel de esfuerzo por defecto (str, p. ej. `high`).
  - `defaultModelVariant`: p. ej. `thinking`.
  - `defaultModelContextWindow`: int (512 000 en esta máquina).
  - `permissionMode`, `logLevel`, `dataContribution.enabled`, `memory.enabled`.
  - **Cuidado:** `provider.minimax.options` y `custom_provider.*.options` (3 claves)
    son donde vive la credencial. No leer ni copiar.
- **`mcode provider list`** (local): una línea por proveedor con
  `<id>\t<estado>\t<origen de credencial>`. Formato observado:
  `minimax_oauth  active  managed login`, `minimax_api  enabled  no key`,
  `custom_provider:opencode-go  enabled  oc_s****Gw6M` (clave **enmascarada** por el
  propio CLI). Sirve para saber qué proveedores están utilizables sin gastar nada.
- **`~/.minimax/cache/models-dev-catalog.json`** (≈6,8 MB) y
  `~/.minimax/model-cache.json` (`{version, provider_status{}, model_status{}}`).

### 3.3 Uso y límites observables

- **Ruta:** `~/.minimax/v2/sqlite/runtime-state.sqlite` (≈192 MB, WAL de 4,4 MB).
  Tabla clave: `local_runtime_token_usage`:
  `id:int, session_id:str, agent_name:str, framework_type:str, turn_id:str, model:str,
  ts:int (epoch ms), input_tokens:int, output_tokens:int, reasoning_tokens:int,
  cache_read_tokens:int, cache_write_tokens:int, cost_usd:real, raw:str`.
  - Agregados observados: 6 459 filas; `ts` entre 2026-10-23T21:20Z y
    2026-11-19T17:40Z (¡hoy!); `input_tokens` 20 327 002, `output_tokens` 5 391 597,
    `cache_read_tokens` 1 560 087 343, `cache_write_tokens` 973 419,
    `cost_usd` 0.0.
  - `model` observed: `custom_provider:opencode-go/deepseek-v4.1-flash` (3 251),
    `null` (1 719), `minimax/MiniMax-M3.1-Flash` (1 390),
    `custom_provider:opencode-go-anthropic/claude-haiku-5-5` (72),
    `custom_provider:opencode-go-responses/gpt-6-luna` (27).
  - `framework_type`: `pi-agent` (6 459). Ojo: esta tabla la escribe el runtime de
    EzAgent/MiniMax Code, no el CLI `mcode` suelto.
  - Tabla relacionada: `local_runtime_v2_goal_token_charges`
    (`request_id, goal_id, session_id, turn_id, source, phase, attempts, outcome,
    input_tokens, output_tokens, cache…`).
- **Límite / ventana:** **no hay**. Un barrido de los logs de
  `~/.minimax/v2/observability/logs/runtime-*.log` (hoy: hasta 710 KB) buscando
  `rate limit|usage limit|quota|429|too many requests|insufficient` no encontró ningún
  evento de cuota real (los falsos positivos eran códigos de salida `4294967295`).
  El runtime registra `llm_response_identifiers` con `provider`, `model`,
  `response_status:int` (200) y `downstream_request_id`.
- **Plan:** la credencial es `minimax_oauth` (login gestionado), así que los límites
  son del lado del servidor de MiniMax: **no hay nada local que leer**.
  `~/.minimax/auth/prod` existe: **no leer** (credenciales).

### 3.4 Nota de privacidad

`~/.minimax/config.yaml` mezcla catálogo (público) y credenciales (`options`).
El lector de IR-2 debe abrir el YAML y **seleccionar solo** las claves
`defaultModel`, `defaultModelThinking`, `defaultModelVariant`, `provider.*.models`,
`provider.*.model_order`, `provider.*.whitelist` y `custom_provider.*.models`, nunca
`.options`.

---

## 4. Codex (`codex`)

### 4.1 Executable nativo y versión

- Ruta real: `C:\Users\JDMoreu\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe`.
- Versión: `codex.exe --version` → `codex-cli 0.169.0`.
- `~/.codex/version.json` → `{latest_version, last_checked_at, dismissed_version}`.
- `codex --help` **no tiene** subcomando de uso o de cuota (sí `doctor`, `debug`,
  `features`, `login`): no hay lectura directa de límites desde la CLI.

### 4.2 Modelos y esfuerzos disponibles sin gastar

- **`~/.codex/models_cache.json`** (465 KB), refrescado por el propio Codex:
  `{fetched_at:str ISO-8601, etag:str, client_version:str, identity:str,
  models:[…]}`. Cada modelo: `slug`, `display_name`, `description`,
  `default_reasoning_level`, `supported_reasoning_levels:[{effort, description}]`,
  `shell_type`, `visibility` (`list`/`hide`), `supported_in_api:bool`, `priority:int`,
  `additional_speed_tiers`, `service_tiers`, `available_access_programs`,
  `model_messages{…}` (no necesario para IR-2).
  - En esta máquina: 10 modelos; niveles de esfuerzo disponibles
    `low, medium, high, xhigh, max, ultra` (los `hide` no se ofrecen).
  - `fetched_at` 2026-11-17T17:25:42Z, `client_version` 0.169.0.
  - El campo `identity` es un identificador de 64 caracteres: **no copiar**.
- **Modelo y esfuerzo activos:** `~/.codex/config.toml`, claves de nivel superior
  `model` (str) y `model_reasoning_effort` (str, p. ej. `high`).
  - **El mismo archivo** contiene `[projects.'<ruta>'] trust_level` con rutas de
    proyectos de la persona: el lector debe leer **solo** `model` y
    `model_reasoning_effort` (parseo TOML por clave, no volcar el archivo).
- `~/.codex/config.toml` no guarda claves; las credenciales están en
  `~/.codex/auth.json` — **no leer**.

### 4.3 Uso y límites observables — **la mejor fuente de todas**

- **Ruta:** `~/.codex/sessions/<YYYY>/<MM>/<DD>/rollout-<ISO>-<uuid>.jsonl`
  (23 archivos en esta máquina; el más reciente del 2026-11-17).
  Un JSON por línea; el registro que nos interesa es
  `{"type":"…","timestamp":…,"ordinal":int,"payload":{…}}` con
  `payload.type == "token_count"`.
- **Estructura de `payload` (campos y unidades):**
  - `payload.info.total_token_usage` y `payload.info.last_token_usage`: ambos con
    `input_tokens:int`, `cached_input_tokens:int`, `cache_write_input_tokens:int`,
    `output_tokens:int`, `reasoning_output_tokens:int`, `total_tokens:int`.
  - `payload.info.model_context_window:int`.
  - `payload.info.last_usage_includes_reasoning:bool`.
  - **`payload.rate_limits`** (9 campos):
    | Campo | Tipo | Unidad / significado |
    |---|---|---|
    | `limit_id` | str | id del límite (`codex`) |
    | `limit_name` | str\|null | nombre del límite |
    | `primary.used_percent` | float | **porcentaje 0–100 de la ventana primaria** |
    | `primary.window_minutes` | int | **minutos** de la ventana primaria |
    | `primary.resets_at` | int | **epoch en segundos** → reinicio |
    | `secondary.used_percent` | float | porcentaje 0–100 de la ventana secundaria |
    | `secondary.window_minutes` | int | minutos de la ventana secundaria |
    | `secondary.resets_at` | int | epoch en segundos |
    | `credits.has_credits` / `credits.unlimited` | bool | crédito extra |
    | `credits.balance` | str | saldo |
    | `individual_limit` | null\|obj | límite individual (aquí `null`) |
    | `spend_control_reached` | null\|bool | control de gasto |
    | `plan_type` | str | **plan** (`free`, `plus`) |
    | `rate_limit_reached_type` | null\|str | tipo de límite alcanzado |
- **Agregados observados (23 rollouts, 894 registros con `rate_limits`):**
  - Ventanas `primary.window_minutes` distintas: **300** (5 h), **43 200** (30 días) y
    `secondary.window_minutes` **10 080** (7 días).
  - `primary.used_percent`: mín 0,0 / máx 90,0. `secondary.used_percent`: mín 0,0 /
    máx 54,0.
  - `plan_type` distintos: `free`, `plus`.
  - Último registro (2026-11-17T17:25:59Z): `primary` 7,0 % / 300 min,
    `secondary` 54,0 % / 10 080 min, `primary.resets_at` epoch de segundos.
  - 21 de 23 archivos contienen `rate_limits` (2 sesiones viejas no lo traen).
- **Recencia:** la última sesión de Codex es del **2026-11-17** (2 días antes de la
  investigación) — es el agente con el dato de cuota más viejo, porque Codex solo
  escribe cuando se usa.
- **Otros archivos de Codex:** `session_index.jsonl` (índice de hilos),
  `state_5.sqlite` (proyectos/hilos; **sin** tablas de cuota),
  `logs_2.sqlite` (37 MB + WAL de 4,5 MB; logs de sesión), `thread_history_1.sqlite`.

> **Conclusión para IR-2:** Codex es el único agente con el que se puede construir un
> lector de «uso en la ventana» de verdad: `%` usado, tamaño de ventana en minutos y
> fecha de reinicio, todo ya normalizado.

---

## 5. Qwen Code (`qwen`)

**No está instalado en esta máquina.** Evidencia:

- `where.exe qwen` y `where.exe qwen.cmd` → sin resultados.
- `npm ls -g --depth=0` → `@earendil-works/pi-coding-agent`, `@opencode/cli`,
  `agent-browser`, `bun`, `corepack`, `npm`, `pnpm`. **Sin `@qwen-code/qwen-code`.**
- No existen `~/.qwen`, `%APPDATA%\Qwen`, `%LOCALAPPDATA%\Qwen` ni
  `%LOCALAPPDATA%\Programs\qwen`.
- Lo único que aparece con «qwen» son archivos de datos de proveedores **de Pi**
  (`pi-ai/dist/providers/data/qwen-token-plan*.json`), que son catálogo de Pi, no la
  instalación de Qwen Code.

Consecuencia para IR-1: `qwen` debe reportarse como **no instalado** y sus lectores no
deben ejecutarse. Cuando la persona lo instale, el punto de partida sería el mismo que
para Claude Code (`~/.qwen/`), pero **esto no está verificado en esta máquina** y hay
que decirlo como tal.

---

## 6. Pi (`pi`)

### 6.1 Executable nativo y versión

- Ruta real: no hay `.exe`. `C:\Users\JDMoreu\AppData\Local\pi-node\current\pi.cmd`
  → `node.exe node_modules\@earendil-works\pi-coding-agent\dist\bundle\cli.js`
  (el shim `.cmd` busca `node.exe` en el mismo directorio y cae a `node` del PATH).
- Versión: `pi --version` → `0.85.1`; `npm ls -g` → `@earendil-works/pi-coding-agent@0.85.1`;
  `~/.pi/agent/settings.json` → `lastChangelogVersion: "0.85.1"`.
- Datos en `~/.pi/agent/`: `settings.json`, `models-store.json`, `auth.json`, `sessions/`,
  `skills/`, `bin/` (`fd.exe`, `rg.exe`), `npm/`.
  `~/.pi/agent/settings.json` → `{packages:[str], lastChangelogVersion:str, theme:str}`.

### 6.2 Modelos y esfuerzos disponibles sin gastar

- **Catálogo local de proveedores**:
  `…\@earendil-works\pi-coding-agent\node_modules\@earendil-works\pi-ai\dist\providers\data\`
  → 39 archivos JSON (uno por proveedor) + `.manifest.json`
  (`{schemaVersion, generatedAt, structureHash, files}`). Cada archivo agrupa por
  familia de API (`openai-completions`, `openai-responses`, `anthropic-messages`, …) y
  dentro, por modelo: `id`, `name`, `api`, `provider`, `baseUrl`, `compat{…}`,
  `reasoning:bool`, `input:[str]`, `cost{input,output,cacheRead,cacheWrite}`,
  `contextWindow:int`, `maxTokens:int` y, cuando aplica,
  `thinkingLevelMap:{off,minimal,low,medium,high,xhigh,max → str|null}`.
  Ejemplos: `opencode-go` → 21 modelos `openai-completions` + 4 `openai-responses` +
  2 `anthropic-messages`; existen proveedores de *plan de tokens*
  (`qwen-token-plan`, `qwen-token-plan-cn`, `qwen-token-plan-individual`,
  `xiaomi-token-plan-{cn,ams,sgp}`).
- **`pi --list-models`** es el listado oficial, pero en esta máquina responde
  `No models available. Use /login to log into a provider via OAuth or API key.`
  porque no hay credenciales configuradas. Es decir: **el catálogo existe pero no hay
  modelos usables**; el comando sirve para distinguir «catálogo vacío» de
  «sin credenciales».
- **Estado de credenciales (local y sin red):**
  `pi auth check --provider <p> --json --no-refresh` →
  `{"status":"not_ready","provider":"minimax","reason":"credentials_not_configured"}`.
  **Nunca** usar `--credentials`, `print-api-key` ni `print-bearer-token`.
  `--no-refresh` evita el refresco de OAuth (que sí tocaría la red).
- **Niveles de esfuerzo:** los de Pi son `off, minimal, low, medium, high, xhigh, max`
  (`pi --thinking <nivel>`, o `pi --model sonnet:high`). Por modelo, el
  `thinkingLevelMap` indica qué niveles existen y con qué cadena de esfuerzo se
  envían (los no soportados valen `null`).

### 6.3 Uso y límites observables

- **Ruta:** `~/.pi/agent/sessions/<slug-de-ruta>/<ISO>_<uuid>.jsonl`.
- **Estructura:** un JSON por línea; tipos de registro `session`, `model_change`,
  `thinking_level_change`, `message`, `custom`.
  - `session`: `{type, version:int, id, timestamp, cwd}`.
  - `model_change`: `{provider:str, modelId:str}`.
  - `thinking_level_change`: `{thinkingLevel:str}`.
  - `message.usage`: **`input:int`, `output:int`, `cacheRead:int`, `cacheWrite:int`,
    `reasoning:int`, `totalTokens:int`, `cost:{…5 claves…}}`**.
  - `message` además: `role`, `content`, `timestamp:int`, `api`, `provider`, `model`,
    `stopReason`, `responseId`, `rawStopReason`, `toolCallId`, `toolName`,
    `isError`, `details`.
- **Agregado observado:** 1 sesión, 97 registros, 45 con `usage`
  (`input`, `output`, `cacheRead`, `cacheWrite`, `reasoning`, `totalTokens`, `cost`).
- **Límite / ventana:** **no hay**. Los proveedores de *plan de tokens* existen en el
  catálogo, pero el estado de cuota no se guarda en disco: solo lo sabría la API del
  proveedor.
- **Recencia:** la sesión más reciente es del **2026-09-05** (algo más de dos meses).
  `models-store.json` y `auth.json` están **vacíos** (2 bytes).

---

## 7. Tabla resumen — qué se puede saber sin gastar cuota

| Dato | Claude Code | OpenCode | MiniMax Code | Codex | Qwen Code | Pi |
|---|---|---|---|---|---|---|
| Instalado | sí | sí | sí | sí | **no** | sí |
| Versión (local) | ✓ `settings`/`.last-update-result.json` | ✓ `service.json`, `kv` | ✓ release `current` | ✓ `version.json` | — | ✓ `settings.json` |
| Lista de modelos | ✓ catálogo en caché | ✓ `models` + `kv` | ✓ `config.yaml` | ✓ `models_cache.json` | — | ✓ `pi-ai` + `--list-models` |
| Esfuerzo por modelo | ✓ `modelSettings.*.effortLevel`, `settings_vocabulary` | ✓ `model.json.variant` | ✓ `defaultModelThinking.effort` | ✓ `supported_reasoning_levels[].effort` | — | ✓ `thinkingLevelMap` |
| Modelo activo | ✓ `settings.json.model` | ✓ `model.json.recent` | ✓ `config.yaml.defaultModel` | ✓ `config.toml.model` | — | ✓ `model_change` |
| Tokens gastados | ✓ `message.usage` | ✓ `stats --json` | ✓ SQLite `local_runtime_token_usage` | ✓ `payload.info.*_token_usage` | — | ✓ `message.usage` |
| Coste | ✓ `additionalModelCostsCache` | ✓ `stats.cost` | ✓ `cost_usd` (0.0 con OAuth) | ✓ en `stats`/`message.usage` | — | ✓ `message.usage.cost` |
| **% de ventana** | ✗ | ✗ | ✗ | **✓ `used_percent`** | — | ✗ |
| **Fecha de reinicio** | ✗ | parcial (cuenta atrás en el log) | ✗ | **✓ `resets_at`** | — | ✗ |
| **Tamaño de ventana** | ✗ | parcial (etiqueta `5-hour`/`monthly`) | ✗ | **✓ `window_minutes`** | — | ✗ |
| Plan / suscripción | ✓ `organizationType` | parcial (texto del error) | parcial (proveedor OAuth) | ✓ `plan_type` | — | ✗ |
| Aviso de agotado | parcial (`fallback_credit`) | ✓ (log) | ✗ | ✓ (`rate_limit_reached_type`) | — | ✗ |
| Preparación del agente | ✓ | ✓ | ✓ (`provider list`) | ✓ | — | ✓ (`auth check --json`) |

---

## 8. Propuesta de lector por fuente

Reglas comunes:
- Abrir siempre en **solo lectura**; los SQLite con `?mode=ro` + `PRAGMA query_only=1`.
- **Nunca** copiar al `entorno.json` los campos marcados como secretos.
- Tolerancia a formatos que cambian: si falta un campo, el lector devuelve
  `desconocido` para ese dato en vez de fallar; nunca adivinar un límite.

### 8.1 Codex — lector de cuota (plantilla de referencia)

- **Qué leer:** `~/.codex/sessions/<YYYY>/<MM>/<DD>/rollout-*.jsonl`. Recorrer los
  archivos **del más reciente al más viejo** (por `mtime`), línea a línea, y quedarse
  con el **último** registro cuyo `payload.rate_limits` exista.
- **Cómo calcular el uso en la ventana:** para cada bloque `primary` y `secondary`:
  - `used_percent` → porcentaje (float 0–100) tal cual.
  - `window_minutes` → tamaño de ventana en minutos (300 = 5 h, 10 080 = 7 d,
    43 200 = 30 d). **Clasificar por `window_minutes`, no por la posición
    `primary`/`secondary`**: la ventana primaria puede ser de 300 o de 43 200 min.
  - `resets_at` → epoch en **segundos** → `datetime.fromtimestamp(resets_at, tz)`.
  - `plan_type` → plan declarado/observado.
  - `credits.has_credits` / `credits.unlimited` → si hay crédito extra disponible.
- **Tolerancia:** `rate_limits` ausente (sesiones viejas) → `desconocido`;
  `resets_at:null` → «reinicio desconocido»; `used_percent` fuera de 0–100 → se
  descarta y se marca `desconocido`; si el último registro con cuota es de otro día,
  indicar la antigüedad del dato («dato de hace N días») en vez de presentarlo como
  actual.
- **Memoria:** los `.jsonl` de Codex llegan a 2 MB por sesión; leer en streaming.

### 8.2 Codex — lector de modelos

- `~/.codex/models_cache.json` → `models[]`; quedarse con `slug`,
  `display_name`, `default_reasoning_level`, `supported_reasoning_levels[].effort`,
  `visibility`, `supported_in_api`.
- De `~/.codex/config.toml` leer **solo** `model` y `model_reasoning_effort`.
- Tolerancia: si `models_cache.json` no está (Codex no lo descargó), usar un catálogo
  de fábrica y marcar `catálogo: "estático"`; `visibility == "hide"` → no ofrecer.

### 8.3 Claude Code — lector de modelos, esfuerzo y plan

- Modelos: enumerar `~/.claude/cache/model-catalog/*.json` (el nombre tiene un hash) y
  quedarse con el de `staleAt` más reciente; extraer `catalog.config.models[].id`,
  `catalog.config.provider_alias_targets.*.default` y
  `catalog.config.settings_vocabulary.effort_level`.
- Esfuerzo/modelo activo: `~/.claude/settings.json` → `model` y
  `modelSettings.<id>.effortLevel`.
- Coste por modelo: `~/.claude.json` → `additionalModelCostsCache`.
- Plan: `~/.claude.json` → `oauthAccount.organizationType` (+ `billingType`,
  `organizationRateLimitTier`, `hasExtraUsageEnabled`,
  `cachedExtraUsageDisabledReason`, `unusedLimitResetStatusCache.holdsUnusedReset`).
  **Ignorar por completo** `emailAddress`, `fullName`, `displayName`,
  `organizationName` y los UUID.
- Uso (tokens): `~/.claude/projects/*/*.jsonl` → sumar `message.usage.*`; si además se
  quiere el coste, `additionalModelCostsCache` × tokens.
- **Ventana:** no hay fuente → el lector debe devolver `desconocido` para
  «uso % de ventana» y, si se quiere un sustituto, reportar *tokens de la última hora*
  como *proxy* etiquetado como tal.
- Tolerancia: `settings.json` sin `modelSettings` → esfuerzo por defecto del catálogo;
  `model-catalog` ausente → catálogo de fábrica; `oauthAccount` ausente → plan
  `desconocido`.

### 8.4 OpenCode — lector de uso y de cuota

- **Uso:** `opencode stats --standalone --json` (siempre `--standalone`; sin él falla
  con `Background service failed to start`). Leer `tokens`, `cost`, `models[]`
  (con `model.variant` = esfuerzo) y `activity[]` para el gráfico diario.
  - Alternativa sin arrancar nada: consultar la base en modo `ro`
    (`session`, `session_message`, `message`, `part`) — pero `opencode stats` es
    mucho más simple y ya agrega todo.
- **Modelos y esfuerzo:** `~/.local/state/opencode/model.json` → `recent` y
  `variant`; `opencode models` para el catálogo completo; y, si se quiere evitar la
  red, el valor `kv['models-dev:v2:catalog']` + `kv['models-dev:v2:catalog:meta']`
  (`.updatedAt` dice si la caché está fresca) en
  `~/.local/share/opencode/opencode.db`.
- **Cuota:** recorrer **la cola** de `~/.local/share/opencode/log/opencode.log` y
  quedarse con el último aviso que case con:
  - `usage limit reached (5-hour|monthly|weekly, resets in Nd Nh)` →
    ventana = etiqueta, reinicio = ahora + la cuenta atrás;
  - `Free usage limit reached` → plan `Free`, estado `agotado`.
  - No hay porcentaje → el estado es `agotado` si el aviso es reciente, si no
    `desconocido`.
  - Tolerancia: el texto puede cambiar (OpenCode es Bun y los mensajes vienen del
    servidor). Regex robusta sugerida:
    `usage limit reached \((?<ventana>[^)]+),\s*resets in (?<r>[\d]+\s*[dhm]\s*[\dhm]*)\)`
    y, si no casa, buscar `(?<ventana>5-hour|weekly|monthly)` cerca de
    `limit reached`. Si el log no existe o está rotado → `desconocido`.
- Memoria: el log llega a 22 MB y la base a 1,5 GB → leer el log **de cola hacia
  atrás** (no cargarlo entero) y la base en modo `ro` con consultas concretas.
- Secretos: **no** abrir `~/.local/state/opencode/service.json` (contraseña del
  servidor local), `~/.local/share/opencode/auth.json`, ni las tablas `account`,
  `control_account` y `credential` de la base.

### 8.5 MiniMax Code — lector de modelos, proveedores y uso

- Modelos y esfuerzo: `~/.minimax/config.yaml` → `defaultModel`,
  `defaultModelThinking.effort`, `defaultModelVariant`,
  `provider.minimax.models`/`model_order`/`whitelist`,
  `custom_provider.*.models`. **Seleccionar claves; nunca `.options`.**
- Proveedores listos: `mcode provider list` → `<id>\t<estado>\t<origen>`; la clave
  sale enmascarada por el propio CLI (no filtrar a mano salvo por defecto).
- Uso: `~/.minimax/v2/sqlite/runtime-state.sqlite` (modo `ro`) →
  `select model, count(*), sum(input_tokens), sum(output_tokens), sum(cache_read_tokens),
  sum(cache_write_tokens), max(ts) from local_runtime_token_usage group by model`.
  Los `ts` son epoch en **milisegundos** (¡cuidado: Codex usa segundos!).
- Límite: no hay fuente local → `desconocido`. Si la persona la declara en
  `plans.toml`, manda lo declarado (principio de §2 del diseño).
- Secretos: `~/.minimax/auth/**`, `~/.minimax/codex-auth.json` — no leer.

### 8.6 Pi — lector de modelos, disponibilidad y uso

- Catálogo: leer los JSON de
  `…\@earendil-works\pi-coding-agent\node_modules\@earendil-works\pi-ai\dist\providers\data\`
  (38 proveedores + `.manifest.json`) y quedarse con `id`, `name`, `provider`,
  `api`, `contextWindow`, `cost`, `thinkingLevelMap`.
- Disponibilidad real: `pi --list-models` (si dice `No models available` → hay
  catálogo pero **sin credenciales**) y `pi auth check --provider <p> --json
  --no-refresh` → `{status, provider, reason}` (`not_ready` /
  `credentials_not_configured`). `--no-refresh` para no tocar la red.
- Uso: `~/.pi/agent/sessions/*/*.jsonl` → `message.usage` (`input`, `output`,
  `cacheRead`, `cacheWrite`, `reasoning`, `totalTokens`, `cost`).
- Límite: no hay fuente local → `desconocido`.
- Secretos: `~/.pi/agent/auth.json` — no leer.

### 8.7 Qwen Code

- No instalado: el lector debe **saltar** el tipo y no fallar. Si algún día se
  instala, el punto de partida sería `~/.qwen/` (patrón tipo Claude Code) **pero esto
  no está verificado**: hay que marcarlo como supuesto hasta comprobarlo.

---

## 9. Recomendaciones para IR-1 / IR-2 / IR-3

1. **`ez init` no debe copiar catálogos enteros.** El catálogo de OpenCode son 13 MB y
   el de Claude 16 KB: copiar solo ids, alias y niveles de esfuerzo.
2. **`ez usage` debe distinguir tres estados de ventana**: `observado` (Codex: % real),
   `inferido` (OpenCode: etiqueta + cuenta atrás, sin %) y `desconocido` (Claude Code,
   mcode, Pi). No inventar un porcentaje donde no existe.
3. **El detector de «agotado» debe buscar en varios sitios por agente**, porque ninguno
   tiene el mismo formato: Codex (`rate_limits`), OpenCode (log), Claude Code
   (`fallback_credit`), mcode/Pi (nada local → solo lo declarado por la persona).
4. **`plans.toml` es imprescindible** para Claude Code, mcode y Pi: son los tres sin
   ningún límite observable en disco, y el principio de §2 del diseño ya dice que lo
   declarado gana.
5. **`ez init` debería evitar la red por defecto.** Todo lo anterior es local excepto
   `opencode models` (que se puede sustituir por la caché `kv`) y la primera escritura
   de `models_cache.json` de Codex.
6. **Formatos que cambian:** los campos observados en esta máquina son de versiones
   concretas (Claude Code 2.1.328, OpenCode 2.3.0, Codex 0.169.0, mcode 0.7.6,
   Pi 0.85.1). Los lectores deben degradar a `desconocido`, nunca a error, y registrar
   la versión del agente que produjo el dato.

---

## 10. Lo que **no** se hizo (y por qué)

- **No** se ejecutó ningún agente con un mensaje, ni se abrió ninguna sesión con
  modelo, ni se llamó a ninguna API de modelo. Solo `--version`, `--help`, listados
  locales (`opencode stats`, `opencode models` indirectamente vía caché, `pi
  --list-models`, `pi auth check`, `mcode provider list`) y lectura de archivos.
- **No** se leyó el contenido de ninguna conversación. De las sesiones de Claude, Codex
  y Pi solo se extrajeron **nombres de campo, tipos y agregados** (sumas, mínimos,
  máximos, recuentos) mediante un script de inspección que recorre una línea JSON y
  describe claves y tipos. Del log de OpenCode se extrajeron **líneas de error
  del límite de cuota**, redactadas.
- **No** se leyó ningún archivo de credenciales: `~/.codex/auth.json`,
  `~/.claude/.credentials.json`, `~/.claude.json` → `oauthAccount` (solo campos de
  plan), `~/.local/share/opencode/auth.json`, `~/.local/state/opencode/service.json`
  (se vio el nombre del campo `password` al listar el archivo; **no se leyó su
  valor**), `~/.minimax/auth/**`, `~/.pi/agent/auth.json`.
  `mcode provider list` muestra la clave ya **enmascarada** por el propio CLI.
- **No** se tocó la app de voz ni la ventana de Herdr, ni ningún archivo de producto.
  El único archivo escrito es este informe.
- **Memoria:** todos los procesos fueron de un solo hilo y bajo el tope de 1 GB; las
  bases grandes (`opencode.db` 1,5 GB, `runtime-state.sqlite` 192 MB) se abrieron en
  modo solo lectura y con consultas concretas.

---

## 11. Evidencia: comandos ejecutados (resumen)

```
where.exe claude|opencode|mcode|codex|qwen|pi
claude.exe --version                                  -> 2.1.328 (Claude Code)
opencode.exe --version                               -> opencode v2.3.0
codex.exe --version                                  -> codex-cli 0.169.0
node <pi-coding-agent/dist/bundle/cli.js> --version  -> 0.85.1
node <@minimax-ai/code/cli.js> --version             -> 0.7.6
mcode --help / provider --help / login --help
pi --help / --list-models / auth --help / auth check --provider X --json --no-refresh
opencode --help / stats --help / stats --standalone --json
npm ls -g --depth=0                                  (confirma que qwen no está)
```

Inspección (solo estructura y agregados):
`~/.claude/{settings.json,.claude.json,cache/model-catalog/*.json,projects/**/*.jsonl}`,
`~/.codex/{models_cache.json,config.toml,version.json,sessions/**/rollout-*.jsonl,state_5.sqlite}`,
`~/.local/share/opencode/{opencode.db,log/opencode.log}`, `~/.local/state/opencode/*.json`,
`~/.minimax/{config.yaml,model-cache.json,cache/,v2/sqlite/runtime-state.sqlite,v2/observability/logs/}`,
`~/.pi/agent/{settings.json,sessions/**/*.jsonl}`,
`…/pi-ai/dist/providers/data/*.json`.

Estado: completado

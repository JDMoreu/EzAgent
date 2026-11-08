# E3 · Núcleo de orquestación y backends de agentes

| Campo | Valor |
|---|---|
| Depende de | E1 (set dorado) |
| Agente sugerido | MiniMax M3.1 Flash; T1, T2, T5 y T6 aptas para DeepSeek |
| Esfuerzo | L (1–2 semanas) |
| Rama | `stage/E3-orquestacion` |
| Hallazgos | P2, L5, L6, L7, L8, T1, T2, T4, T5, T6, T7, T8, T9, T11, T12, T14, D2 |
| Paralelo con | E2 |

## Objetivo
Un núcleo que orquesta agentes con **una sola llamada al LLM por turno**,
6 herramientas planas, prompt estable y cacheable, resolución de nombres en
código, narración por plantillas y dos backends (Herdr y Direct), con ≥ 95 %
de acierto en el set dorado usando DeepSeek V4.1 Flash.

## Por qué
Es el corazón del nuevo posicionamiento. Hoy el modelo barato recibe ≈ 5 000
tokens, 14 herramientas y una megaherramienta de 9 acciones; tarda hasta 4
rondas y falla en acciones y argumentos. Sin backend directo, nadie sin
Herdr puede usar EzAgent.

## Dentro del alcance
- T1 `agents/backend.py`, `agents/registry.py` (agents.toml) y `agents/resolve.py`.
- T2 `agents/herdr.py`: `HerdrBackend` envolviendo `voice/herdr.py` + `voice/watcher.py`.
- T3 `agents/direct.py`: `DirectBackend`.
- T4 `agents/service.py` + `agents/tasks.py` (migración de `voice/tasks.py`): estado de sesión persistente, cadenas `after`, avisos.
- T5 `llm/tools.py`, `llm/parser.py` (respaldo + coerción), `llm/narrate.py` + catálogo `es`/`en`.
- T6 `llm/router.py` y `llm/prompt.py`.
- T7 `llm/turn.py`: bucle de 1 llamada + reintento + resumen; `voice/llm.py::Messenger` pasa a ser un adaptador fino.
- T8 Memoria mínima: `memory/profile.py` (USER.md ≤ 800 caracteres) y `memory/projects.py` (projects.toml).
- T9 Integración en `voice/app.py`: eventos de agentes (`done`, `blocked`) → earcons/narración; unificación de palabras clave.

## Fuera del alcance
- No borrar todavía `tools_fs`, `tools_web`, skills ni memoria FTS (es E4):
  simplemente **dejan de registrarse** en el turno (las herramientas nuevas
  sustituyen al registro anterior). Sus pruebas siguen pasando porque prueban
  módulos, no el turno.
- No tocar la UI salvo publicar eventos de agentes por la API existente del
  notch (`set_state`, `notice`).
- No implementar la tecla de confirmación nueva: se reutiliza
  `agent/confirm.py` para `stop_agent` y envíos a agentes ocupados.

## Archivos que se tocan
| Archivo | Acción | Tarea |
|---|---|---|
| `src/ezagent/agents/__init__.py`, `backend.py`, `registry.py`, `resolve.py` | crear | T1 |
| `src/ezagent/agents/herdr.py` | crear | T2 |
| `src/ezagent/agents/direct.py` | crear | T3 |
| `src/ezagent/agents/service.py`, `tasks.py`, `watcher.py` | crear | T4 |
| `src/ezagent/llm/__init__.py`, `tools.py`, `parser.py`, `narrate.py` | crear | T5 |
| `src/ezagent/core/i18n/__init__.py`, `es.toml`, `en.toml` | crear | T5 |
| `src/ezagent/llm/router.py`, `prompt.py` | crear | T6 |
| `src/ezagent/llm/turn.py` | crear | T7 |
| `src/ezagent/voice/llm.py` | modificar | T7: `Messenger.ask_stream` delega en `llm.turn.TurnRunner` |
| `src/ezagent/memory/__init__.py`, `profile.py`, `projects.py` | crear | T8 |
| `src/ezagent/voice/app.py` | modificar | T9: construcción de servicios y callbacks de eventos (≈ l. 335-415, watcher y `_fast_command`) |
| `src/ezagent/voice/tasks.py` | modificar | T4: reexporta desde `agents/tasks.py` (compatibilidad) |
| `src/ezagent/agent/runtime.py` | modificar | T7: deja de construir el turno; se conserva solo lo que use `confirm`/`audit` |
| `src/ezagent/agent/loop.py` | borrar | T7: sus pruebas de política se migran a `tests/test_llm_turn.py` |
| `tests/test_agents_*.py`, `tests/test_llm_*.py`, `tests/test_memory_min.py` | crear | todas |

## Archivos prohibidos
`src/ezagent/voice/notch/**`, `src/ezagent/voice/audio.py`,
`src/ezagent/voice/tts*.py`, `src/ezagent/voice/stt.py` (son de E2).

## Interfaces y contratos

- `AgentBackend`, `RunSpec`, `AgentHandle`, `AgentStatus`, `AgentEvent`:
  **exactamente** como en `docs/roadmap/02-arquitectura.md` §5.
- Plantillas de `agents.toml` y `projects.toml`: `02-arquitectura.md` §5. Se
  escriben en `%APPDATA%\EzAgent\` en el primer arranque si no existen; si
  `voice.json` tiene `agents`/`agent_profiles` (formato actual, ver README),
  se migran a `agents.toml` una sola vez y se deja una copia `voice.json.bak`.
- Esquemas de herramientas, prompt, parámetros por modo, parser y plantillas:
  `docs/roadmap/04-estrategia-llm.md` §2–§7.

```python
# agents/resolve.py
@dataclass(frozen=True)
class Resolved:
    key: str | None              # clave en agents.toml / projects.toml
    options: tuple[str, ...]     # candidatos si es ambiguo
def resolve_agent(spoken: str, registry: "AgentRegistry", default: str) -> Resolved: ...
def resolve_project(spoken: str, projects: "Projects", current: str | None) -> Resolved: ...
# Normaliza (minúsculas, sin acentos, sin puntuación); alias exacto > prefijo
# > difflib ratio ≥ 0.75; empate (< 0.05 de diferencia) ⇒ ambiguo.
```

```python
# agents/service.py
@dataclass
class ToolOutcome:
    ok: bool
    speak_key: str                  # clave de narrate (p. ej. "result.run_agent_ok")
    params: dict[str, str]          # para la plantilla
    needs_confirmation: str | None = None   # texto generado por código
    data: str | None = None         # salida de agente para resumir (summarize)

class AgentService:
    def __init__(self, backends: list[AgentBackend], registry, projects, tasks, confirm) -> None: ...
    async def run_agent(self, agent: str = "", project: str = "", task: str = "", model: str = "",
                        effort: str = "", notify: bool = True, background: bool = False,
                        after: str = "") -> ToolOutcome: ...
    async def agent_status(self, agent: str = "", summarize: bool = False) -> ToolOutcome: ...
    async def send_to_agent(self, message: str, agent: str = "") -> ToolOutcome: ...
    async def stop_agent(self, agent: str) -> ToolOutcome: ...
    @property
    def last_agent(self) -> AgentHandle | None: ...   # persiste entre turnos (corrige T8)
    def events(self) -> AsyncIterator[AgentEvent]: ...
```
- `agent=""` en `send_to_agent`/`agent_status` ⇒ `last_agent`.
- `notify` por defecto `True` en `run_agent` (el producto es «te aviso»);
  el prompt no necesita palabras clave de aviso (corrige T9). Se borra la
  triple lista de palabras clave.
- Backend: `backend="auto"` usa Herdr si `available()`, si no Direct.

```python
# llm/turn.py
@dataclass
class TurnResult:
    spoken: str                 # texto final dicho
    tool: str | None
    llm_calls: int              # 0 (command), 1, o 2 (retry / summarize)
    route: str

class TurnRunner:
    def __init__(self, client, prompt: "PromptBuilder", service: AgentService,
                 voice_ctl: "VoiceControl", narrator: "Narrator", cfg) -> None: ...
    def run(self, text: str, on_speak: Callable[[str], None],
            cancel: threading.Event) -> TurnResult: ...
```
- `on_speak` recibe fragmentos (cláusulas) para el TTS; el acuse de
  herramienta se emite en cuanto el stream revela el nombre de la herramienta.
- Texto del modelo antes de un tool-call: se retiene; si al final hay
  tool-call, se descarta (no se habla).
- Historial: 6 intercambios de texto (sin mensajes `tool`).
- El servicio es async; `TurnRunner.run` es síncrono para encajar con el hilo
  de respuesta actual (usa un event loop propio del runner, creado una vez).

## Pasos
1. T1 + pruebas de `resolve` (≥ 20 casos: alias, errores de whisper, empates).
2. T2 `HerdrBackend` con `Herdr` falso; las pruebas de `tests/test_agent_herdr.py`
   relevantes se reescriben contra el backend.
3. T3 `DirectBackend` con un ejecutable falso (`python -c` que imprime y
   sale): run, status, read_output, stop, detección de bloqueo por patrón,
   logs en carpeta temporal.
4. T4 `AgentService` + tareas + cadenas `after` + eventos.
5. T5 tools/parser/narrate + i18n. Parser con ≥ 15 casos de texto «sucio»
   (JSON en texto, `<tool_call>`, `run_agent(...)`, tipos como string).
6. T6 router (casos de `cases.jsonl` con `route`) + prompt (prueba: el
   bloque system es idéntico en dos turnos con distinta hora).
7. T7 `TurnRunner` + adaptación de `Messenger`; borrar `agent/loop.py` y migrar
   sus pruebas de política que sigan aplicando.
8. T8 memoria mínima + `remember` con confirmación.
9. T9 integración en `app.py`; prueba manual con Herdr y sin Herdr.
10. Ejecutar `evals.intents --tools v2` con DeepSeek V4.1 Flash y MiniMax M3.1
    Flash; banco de latencia con frases de acción. Registrar en `metricas.md`.

## Criterios de aceptación
- [ ] Set dorado `--tools v2` con DeepSeek V4.1 Flash: ≥ 95 % (es) y ≥ 90 % (en).
- [ ] Bloque `system` ≤ 1 200 tokens (estimación: caracteres / 4) e idéntico
      entre turnos mientras no cambien los TOML (prueba).
- [ ] Esquemas de herramientas ≤ 2 800 caracteres en total (prueba).
- [ ] Turno con herramienta: `llm_calls == 1` salvo reintento o resumen (prueba
      con cliente falso + banco).
- [ ] Fin de voz → acuse en turnos con herramienta P50 ≤ 700 ms (banco).
- [ ] Con `hc` ausente del PATH: «abre opencode en ezagent y arregla los tests»
      lanza OpenCode por `DirectBackend` y avisa al terminar (prueba con
      ejecutable falso + prueba manual real).
- [ ] «¿Cómo va?» en el turno siguiente a un `run_agent` se refiere al último
      agente (prueba).
- [ ] Un tool-call escrito como texto nunca llega al TTS (prueba).
- [ ] `agent/` ya no importa `voice/` (prueba de imports).
- [ ] `code-review` y `security-review` sin hallazgos altos abiertos.

## Pruebas
- `tests/test_agents_resolve.py`, `test_agents_direct.py`,
  `test_agents_herdr.py`, `test_agents_service.py`,
  `test_llm_parser.py`, `test_llm_router.py`, `test_llm_prompt.py`,
  `test_llm_turn.py`, `test_memory_min.py`, `test_imports.py`.

## Comandos de verificación
```powershell
uv run pytest -q
uv run ruff check src tests
uv run python -m evals.intents --provider opencode --model deepseek-v4.1-flash --tools v2
uv run python -m bench.latency --wav-dir bench/latency/fixtures --runs 3
```

## Riesgos y rollback
- Flags de CLIs de agentes cambian entre versiones → todo en `agents.toml`;
  el agente constructor **no** inventa flags: usa los de la plantilla de
  `02-arquitectura.md` y marca `# DECISION` si cree que alguno es incorrecto.
- Migración de `voice.json` → copia `.bak` y prueba de ida.
- Si el set no llega al 95 %: iterar solo `prompt.py` (ejemplos) y
  descripciones de herramientas, midiendo cada cambio; no añadir herramientas.

## Definición de hecho
Criterios cumplidos, métricas registradas, revisiones hechas, informe de
`AGENTS.md` §6.

## Prompt para el agente
```
Lee AGENTS.md, docs/roadmap/etapas/E3-orquestacion.md,
docs/roadmap/02-arquitectura.md §5 y docs/roadmap/04-estrategia-llm.md completos.
Ejecuta solo la tarea <T#> de E3. Las firmas, esquemas, plantillas y prompt
son contractuales: cópialos, no los reinterpretes. No inventes flags de CLIs.
Entrega el informe de AGENTS.md §6 con el resultado de pytest y, si la tarea
es T7 o posterior, del set dorado.
```

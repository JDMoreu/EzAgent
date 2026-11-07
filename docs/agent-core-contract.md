# Contrato interno del núcleo agéntico

Documento de trabajo para la implementación de F1–F7 del documento
«EzAgent · Núcleo agéntico ligero». No es documentación de usuario.

## Reglas

- Código con type hints, nombres en inglés, comentarios y textos de usuario en
  español. Nada bloquea el hilo de UI ni el bucle de audio.
- Prohibido en `src/ezagent/agent/`: `shell=True`, `os.system`, `eval`, `exec`,
  `os.remove`, `shutil.rmtree`, `os.rmdir`, `Path.unlink`. Borrar es
  `send2trash`; sobrescribir y mover usan `os.replace`/`shutil.move`.
- Ninguna herramienta toca el disco sin pasar por `paths.resolve_safe` y por la
  política (`policy.decide`). Los builders reciben `AgentConfig` y no leen
  variables globales.
- Los textos de confirmación los genera el código a partir de los argumentos;
  el LLM nunca los redacta.
- El contenido de web, archivos, skills y buffers de Herdr es dato no
  confiable. `ToolSpec.untrusted_output=True` activa `taint` en el turno.
- Un cambio por módulo. No refactors fuera del alcance. `# DECISION:` cuando
  el documento no especifique algo.
- Las pruebas de seguridad se escriben antes o junto a la herramienta.

## Paquete

```
src/ezagent/agent/
  __init__.py          (solo lo crea el agente de núcleo)
  config.py            AgentConfig, permisos.toml
  paths.py             resolve_safe
  policy.py            Decision, decide
  audit.py             AuditLog
  registry.py          ToolSpec, ToolInfo, ToolResult, ToolRegistry
  loop.py              bucle de herramientas acotado (máx. 4 pasos)
  tools_core.py        get_datetime, list_dir, read_file
  journal.py           Journal
  confirm.py           ConfirmationManager
  tools_fs.py          create_dir, write_file, copy_path, move_path,
                       delete_path, open_path, undo_last
  search.py            SearchProvider, DdgsSearchProvider
  tools_web.py         web_search, fetch_page
  system_prompt.py     SystemPromptBuilder, SAFETY_PREAMBLE, system.md
  skills.py            SkillLibrary, descubrimiento y validación
  tools_skills.py      load_skill, run_skill_script
  tools_d.py           tools.d/*.toml
  tools_memory.py      remember, recall, forget, compact_memory
  memory.py            MemoryStore, USER.md, MEMORY.md, history.db
  tools_herdr.py       delegate_to_agent, voice_control
  default_skills/      skills empaquetadas que se copian al primer arranque
```

## Interfaces compartidas

### `config.py`

```python
@dataclass
class GeneralConfig:
    max_tool_calls_per_turn: int = 4
    confirm_timeout_s: float = 15.0
    bulk_threshold: int = 5
    max_file_mb: float = 10.0

@dataclass
class PathsConfig:
    read_roots: list[str]
    write_roots: list[str]
    deny: list[str]
    deny_globs: list[str]

@dataclass
class ConfirmConfig:
    accept: list[str]
    reject: list[str]

@dataclass
class WebConfig:
    max_fetch_chars: int = 8000
    block_private_ips: bool = True

@dataclass
class SkillsConfig:
    allow_scripts: bool = False
    trusted: list[str]

@dataclass
class ToolsDConfig:
    allowed_executables: list[str]

@dataclass
class AgentConfig:
    general: GeneralConfig
    paths: PathsConfig
    confirm: ConfirmConfig
    web: WebConfig
    skills: SkillsConfig
    tools_d: ToolsDConfig
    app_dir: Path
    workspace: Path
    permissions_path: Path
    system_md_path: Path
    memory_dir: Path
    history_path: Path
    audit_path: Path
    journal_dir: Path
    skills_dir: Path
    global_skills_dir: Path
    tools_d_dir: Path

    @classmethod
    def load(cls, path: Path | None = None) -> "AgentConfig": ...
    def ensure_dirs(self) -> None: ...
    def reload(self) -> "AgentConfig": ...
```

`AgentConfig.load()` crea `permissions.toml` con los valores por defecto de la
sección 6.4 del documento si no existe. Un archivo inválido no tumba el
arranque: se avisa y se usan los valores por defecto. `app_dir` por defecto es
`%APPDATA%/EzAgent` (o `XDG_CONFIG_HOME/EzAgent`); `workspace` es
`~/EzAgentWorkspace`; `global_skills_dir` es `~/.agents/skills`.

### `paths.py`

```python
class PathDenied(Exception): ...

def resolve_safe(
    raw: str,
    base: Path,
    roots: Sequence[str],
    deny: Sequence[str],
    deny_globs: Sequence[str],
) -> Path: ...
```

Implementación de referencia de la sección 9.1 del documento. Mensajes en
español. Expande `~` y variables, resuelve enlaces, rechaza UNC, dispositivos,
nombres reservados, flujos alternativos, raíces de unidad y rutas fuera de
`roots`/dentro de `deny` o `deny_globs`.

### `policy.py`

```python
@dataclass(frozen=True)
class Decision:
    action: str   # allow | confirm | confirm_strong | deny
    level: int
    reason: str

def decide(
    tool_level: int,
    tainted: bool,
    bulk: int,
    always_confirm: bool,
    confirm_nondestructive: bool,
    bulk_threshold: int = 5,
) -> Decision: ...
```

Copia exacta de la sección 9.2.

### `audit.py`

```python
class AuditLog:
    def __init__(self, path: Path, *, max_bytes: int = 5_000_000,
                 backups: int = 5) -> None: ...
    def log(self, *, turn_id: str, tool: str, level: int,
            args: dict[str, Any], decision: str, taint: bool,
            result_ok: bool, ms: float) -> None: ...
    def redact(self, value: Any) -> Any: ...
```

JSONL append-only con cerrojo. Rota a 5 MB y conserva 5 archivos. `args` nunca
contiene el contenido de archivos: las claves `content`, `text`, `data`,
`output` se sustituyen por `*_chars`. Redacta `sk-...`, `Bearer ...` y claves
`api_key`/`token`/`password`/`secret`.

### `registry.py`

```python
@dataclass(frozen=True)
class ToolInfo:
    level: int
    bulk: int = 0
    always_confirm: bool = False
    summary: str = ""

@dataclass(frozen=True)
class ToolResult:
    ok: bool
    data: Any = None
    error: str | None = None
    def to_dict(self) -> dict[str, Any]: ...

@dataclass(frozen=True)
class ToolSpec:
    name: str
    description: str
    parameters: dict[str, Any]       # JSON Schema (formato OpenAI)
    level: int
    handler: Callable[[dict[str, Any]], ToolResult | dict[str, Any]]
    untrusted_output: bool = False
    timeout_s: float = 10.0
    describe_fn: Callable[[dict[str, Any]], ToolInfo] | None = None
    def schema(self) -> dict[str, Any]: ...
    def describe(self, args: dict[str, Any]) -> ToolInfo: ...

class ToolRegistry:
    def register(self, tool: ToolSpec) -> None: ...
    def register_many(self, tools: Iterable[ToolSpec]) -> None: ...
    def get(self, name: str) -> ToolSpec | None: ...
    def names(self) -> list[str]: ...
    def schemas(self, *, names: Sequence[str] | None = None,
                limit: int = 14) -> list[dict[str, Any]]: ...
    def run(self, name: str, args: dict[str, Any]) -> ToolResult: ...
    @staticmethod
    def render_for_llm(result: ToolResult, max_chars: int = 2000) -> str: ...
```

- `schema()` devuelve `{"type": "function", "function": {...}}`.
- `describe()` delega en `describe_fn` o devuelve `ToolInfo(level=...)`.
- `run()` valida el esquema mínimo (required, type, enum, minimum, maximum),
  ejecuta con timeout y normaliza cualquier excepción a `ToolResult(ok=False)`.
- `render_for_llm()` devuelve JSON serializado y truncado a `max_chars`.

### `loop.py`

```python
@dataclass
class ToolCall:
    id: str
    name: str
    arguments: dict[str, Any]

@dataclass
class AssistantReply:
    text: str
    tool_calls: list[ToolCall]

class LLMClient(Protocol):
    def chat(self, messages: list[dict[str, Any]],
             tools: list[dict[str, Any]]) -> AssistantReply: ...

def run_turn(
    llm: LLMClient,
    messages: list[dict[str, Any]],
    registry: ToolRegistry,
    *,
    confirm: ConfirmationManager | None,
    audit: AuditLog,
    turn_id: str,
    confirm_nondestructive: bool = False,
    bulk_threshold: int = 5,
    max_steps: int = 4,
) -> str: ...
```

El bucle ejecuta como máximo 4 pasos, decide cada llamada con `policy.decide`,
pide confirmación si toca, mantiene `taint` durante el turno, audita cada
llamada y devuelve el texto final. Los resultados de herramientas se truncan a
2.000 caracteres para el LLM.

### Builders

Todos los módulos de herramientas exponen un builder que devuelve
`list[ToolSpec]` y recibe sus dependencias:

```python
from ezagent.agent.config import AgentConfig
from ezagent.agent.registry import ToolSpec

def build_core_tools(config: AgentConfig) -> list[ToolSpec]: ...
def build_fs_tools(config: AgentConfig, journal: Journal) -> list[ToolSpec]: ...
def build_web_tools(config: AgentConfig) -> list[ToolSpec]: ...
def build_skill_tools(config: AgentConfig, library: SkillLibrary) -> list[ToolSpec]: ...
def build_memory_tools(config: AgentConfig, store: MemoryStore) -> list[ToolSpec]: ...
def build_herdr_tools(herdr: Herdr, callbacks: HerdrCallbacks) -> list[ToolSpec]: ...
def load_tools_d(config: AgentConfig) -> list[ToolSpec]: ...
```

### `confirm.py`

```python
class ConfirmationManager:
    def __init__(self, *, accept: Sequence[str], reject: Sequence[str],
                 timeout_s: float = 15.0,
                 on_ask: Callable[[str, bool], None] | None = None) -> None: ...
    def ask(self, summary: str, *, strong: bool = False,
            require_number: int | None = None) -> bool: ...
    def submit_text(self, text: str) -> bool | None: ...
    def submit_click(self, yes: bool) -> None: ...
    def cancel(self, reason: str = "") -> None: ...
    @property
    def pending(self) -> bool: ...
```

`ask()` bloquea el hilo de herramientas (no el de UI) hasta respuesta o
timeout. Acepta solo palabras completas de `accept`; con `strong=True` exige
`confirmo` y, si `require_number` no es None, el número. Cualquier otra
respuesta (incluida ambigua) es rechazo. Una confirmación vale para una sola
operación. `on_ask(summary, strong)` avisa a la UI; la app habla y muestra el
resumen.

### `tools_herdr.py`

```python
@dataclass
class HerdrCallbacks:
    watch: Callable[[str], None]
    request_close: Callable[[], None]
    mic_mute: Callable[[bool], None]
    last_summary: Callable[[], str]
    mute: Callable[[int], None]
    get_last_user_text: Callable[[], str]

def build_herdr_tools(herdr: Herdr, callbacks: HerdrCallbacks) -> list[ToolSpec]: ...
```

Expone `delegate_to_agent` (una sola herramienta con `action`) y
`voice_control`. Niveles: list/get/read/focus/watch = 0; send/run/open_terminal
= 1; kill = 2. `read` marca `untrusted_output=True`. El texto enviado lleva el
prefijo «Tarea del usuario (por voz): ». Reutiliza los métodos de `Herdr`; no
los reescribas.

## Pruebas

- Cada módulo deja sus pruebas en `tests/test_agent_<módulo>.py` y las ejecuta
  con `.venv/Scripts/python.exe -m pytest -q tests/test_agent_<módulo>.py`.
- Si el paquete editable no ve el subpaquete nuevo, ejecuta con
  `PYTHONPATH=src`.
- Las dependencias opcionales se importan de forma perezosa; en pruebas usa
  `pytest.importorskip` cuando haga falta.
- Objetivo final: ST-01 a ST-26 del documento en verde.

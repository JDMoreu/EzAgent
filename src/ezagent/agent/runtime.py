"""Runtime del núcleo agéntico de EzAgent.

Une configuración, registro de herramientas, política, confirmación, auditoría,
memoria y skills en un único objeto que el mensajero de voz puede usar sin
conocer los detalles de cada herramienta.

El runtime no crea ventanas ni habla: publica intenciones en los callbacks que
le entrega ``VoiceApp``. El bucle de herramientas corre en el hilo de respuesta
del LLM; la UI, el audio y Tk siguen en sus hilos.
"""

from __future__ import annotations

import json
import re
import time
import uuid
from dataclasses import dataclass, field
from typing import Any, Callable

from .audit import AuditLog
from .config import AgentConfig
from .confirm import ConfirmationManager
from .journal import Journal
from .memory import MemoryStore
from .paths import PathDenied, resolve_safe
from .policy import decide
from .registry import ToolRegistry, ToolResult
from .skills import SkillLibrary, ensure_default_skills
from .system_prompt import SystemPromptBuilder
from .tools_core import build_core_tools
from .tools_d import load_tools_d
from .tools_fs import build_fs_tools
from .tools_herdr import HerdrCallbacks, build_herdr_tools
from .tools_memory import build_memory_tools
from .tools_skills import build_skill_tools
from .tools_web import build_web_tools

# Herramientas que escriben en disco. El documento limita a 3 escrituras de
# nivel 1 por turno sin aviso adicional y a 20 operaciones de nivel 1 por
# minuto; el runtime audita el aviso y bloquea el límite por minuto.
WRITE_TOOLS = frozenset(
    {
        "create_dir",
        "write_file",
        "copy_path",
        "move_path",
        "open_path",
        "undo_last",
        "run_skill_script",
    }
)

# Base estable de 14 herramientas (el tope del documento). Las consultas de
# hora cambian fetch_page por get_datetime; copiar/abrir/olvidar/compactar y
# ejecutar scripts de skills sustituyen una herramienta equivalente. El LLM
# siempre puede delegar una tarea compleja en Herdr.
BASE_TOOLS: tuple[str, ...] = (
    "voice_control",
    "web_search",
    "fetch_page",
    "list_dir",
    "read_file",
    "write_file",
    "create_dir",
    "move_path",
    "delete_path",
    "undo_last",
    "load_skill",
    "remember",
    "recall",
    "delegate_to_agent",
)

_TIME_RE = re.compile(r"\b(hora|fecha|d[ií]a|hoy|calendario)\b", re.IGNORECASE)
_COPY_RE = re.compile(r"\b(copia|duplica)\b", re.IGNORECASE)
_OPEN_RE = re.compile(r"\b(abre|abrir|abreme|ábreme)\b", re.IGNORECASE)
_FORGET_RE = re.compile(r"\b(olvida|olvidar|borra .*memoria)\b", re.IGNORECASE)
_COMPACT_RE = re.compile(r"\b(compacta|compactar|memoria llena)\b", re.IGNORECASE)


def select_tool_names(
    text: str,
    *,
    scripts_loaded: bool = False,
    has_memory: bool = True,
) -> list[str]:
    """Elige hasta 14 nombres para el prompt de este turno.

    # DECISION: el documento fija 14 herramientas y la app ya usaba 14 solo
    # para Herdr. Se consolida Herdr en ``delegate_to_agent`` y se cambia una
    # herramienta por otra según la intención explícita del turno.
    """
    names = list(BASE_TOOLS)
    if not has_memory:
        names.remove("recall")
    if _TIME_RE.search(text):
        names[names.index("fetch_page")] = "get_datetime"
    if _COPY_RE.search(text) and "create_dir" in names:
        names[names.index("create_dir")] = "copy_path"
    elif _OPEN_RE.search(text) and "create_dir" in names:
        names[names.index("create_dir")] = "open_path"
    if _FORGET_RE.search(text) and "remember" in names:
        names[names.index("remember")] = "forget"
    if _COMPACT_RE.search(text) and "web_search" in names:
        names[names.index("web_search")] = "compact_memory"
    if scripts_loaded and "fetch_page" in names:
        names[names.index("fetch_page")] = "run_skill_script"
    return names[:14]


@dataclass
class RuntimeCallbacks:
    """Puentes entre el runtime y la aplicación de voz."""

    watch: Callable[[str], None] = lambda address: None
    request_close: Callable[[], None] = lambda: None
    mic_mute: Callable[[bool], None] = lambda muted: None
    last_summary: Callable[[], str] = lambda: ""
    mute: Callable[[int], None] = lambda minutes: None
    get_last_user_text: Callable[[], str] = lambda: ""
    remember_task: Callable[[str, str, str], None] = (
        lambda address, agent, task: None
    )
    enqueue_chain: Callable[[dict[str, Any]], None] = lambda spec: None
    pending_tasks: Callable[[], str] = lambda: ""
    on_confirm_ask: Callable[[str, bool], None] | None = None
    on_confirm_done: Callable[[], None] | None = None
    on_warn: Callable[[str], None] | None = None


class AgentRuntime:
    """Fachada del núcleo agéntico para el mensajero de voz."""

    def __init__(
        self,
        voice_config: Any,
        herdr: Any,
        callbacks: RuntimeCallbacks | None = None,
        *,
        config_path: Any = None,
    ) -> None:
        self.voice_config = voice_config
        self.herdr = herdr
        self.callbacks = callbacks or RuntimeCallbacks()
        self.config = AgentConfig.load(config_path)
        self.config.ensure_dirs()
        self._permissions_mtime: float = 0.0
        self._warned_system_md = False
        self.audit = AuditLog(self.config.audit_path)
        self.session_id = uuid.uuid4().hex
        self.turn_id = ""
        self.tainted = False
        self.confirm_nondestructive = bool(
            getattr(voice_config, "confirm_actions", False)
        )
        self.last_agent_address: str | None = None
        self._loaded_skills: set[str] = set()
        self._calls_this_turn = 0
        self._level1_writes = 0
        self._level1_times: list[float] = []
        self._last_user_text = ""
        self.memory = MemoryStore(self.config, session=self.session_id)
        self.skills = SkillLibrary(self.config, audit_warn=self._audit_warn)
        self.skills.refresh()
        try:
            ensure_default_skills(self.config)
            self.skills.refresh()
        except Exception:
            pass
        self.journal = Journal(self.config.journal_dir)
        self.confirm = self._make_confirm()
        self.prompt = SystemPromptBuilder(
            self.config,
            memory_block=self._memory_and_tasks,
            skills_catalog=self.skills.catalog_text,
            audit_warn=self._audit_warn,
            say_warn=self._warn,
        )
        self.registry = ToolRegistry()
        self._rebuild_tools()

    # ------------------------------------------------------------- ciclo
    def _make_confirm(self) -> ConfirmationManager:
        return ConfirmationManager(
            accept=self.config.confirm.accept,
            reject=self.config.confirm.reject,
            timeout_s=self.config.general.confirm_timeout_s,
            on_ask=self.callbacks.on_confirm_ask,
        )

    def _rebuild_tools(self) -> None:
        registry = ToolRegistry()
        builders: list[Callable[[], list[Any]]] = [
            lambda: build_core_tools(self.config),
            lambda: build_fs_tools(self.config, self.journal),
            lambda: build_web_tools(self.config),
            lambda: build_skill_tools(self.config, self.skills),
            lambda: build_memory_tools(
                self.config,
                self.memory,
                confirm=lambda summary, strong: bool(
                    self.confirm is not None
                    and self.confirm.ask(summary, strong=strong)
                ),
            ),
            lambda: build_herdr_tools(
                self.herdr,
                HerdrCallbacks(
                    watch=self._watch_agent,
                    request_close=self.callbacks.request_close,
                    mic_mute=self.callbacks.mic_mute,
                    last_summary=self.callbacks.last_summary,
                    mute=self.callbacks.mute,
                    get_last_user_text=self._get_last_user_text,
                    remember_task=self.callbacks.remember_task,
                    enqueue_chain=self.callbacks.enqueue_chain,
                ),
            ),
            lambda: load_tools_d(self.config),
        ]
        for build in builders:
            try:
                registry.register_many(build())
            except Exception as exc:  # noqa: BLE001 - un tools.d roto no tumba el arranque
                self._audit_warn(f"No pude cargar un grupo de herramientas: {exc}")
        self.registry = registry

    def _audit_warn(self, message: str) -> None:
        try:
            self.audit.log(
                turn_id=self.turn_id or "startup",
                tool="system",
                level=0,
                args={"warning": message[:500]},
                decision="allow",
                taint=False,
                result_ok=True,
                ms=0.0,
            )
        except Exception:
            pass

    def _warn(self, message: str) -> None:
        if self.callbacks.on_warn is not None:
            try:
                self.callbacks.on_warn(message)
            except Exception:
                pass

    def _watch_agent(self, address: str) -> None:
        self.last_agent_address = str(address)
        self.callbacks.watch(str(address))

    def _memory_and_tasks(self) -> str:
        """Memoria persistente + tareas pendientes, dentro del prompt."""
        parts: list[str] = []
        try:
            memory = self.memory.memory_block()
            if memory:
                parts.append(memory)
        except Exception:
            pass
        try:
            tasks = self.callbacks.pending_tasks()
            if tasks:
                parts.append(tasks)
        except Exception:
            pass
        return "\n\n".join(parts)

    def _get_last_user_text(self) -> str:
        value = self.callbacks.get_last_user_text()
        if value:
            return value
        return self._last_user_text

    def refresh(self) -> None:
        """Relee permisos, skills y tools.d al inicio de cada turno."""
        try:
            mtime = float(self.config.permissions_path.stat().st_mtime)
        except OSError:
            mtime = 0.0
        if mtime != self._permissions_mtime:
            self._permissions_mtime = mtime
            try:
                self.config = AgentConfig.load(self.config.permissions_path)
                self.config.ensure_dirs()
                self.prompt = SystemPromptBuilder(
                    self.config,
                    memory_block=self._memory_and_tasks,
                    skills_catalog=self.skills.catalog_text,
                    audit_warn=self._audit_warn,
                    say_warn=self._warn,
                )
                self.confirm = self._make_confirm()
            except Exception as exc:  # noqa: BLE001
                self._audit_warn(f"No pude recargar permissions.toml: {exc}")
        try:
            self.skills.refresh()
        except Exception:
            pass
        self._rebuild_tools()

    def begin_turn(self, text: str) -> None:
        self.turn_id = uuid.uuid4().hex
        self._last_user_text = text
        self.tainted = False
        self._calls_this_turn = 0
        self._level1_writes = 0
        self._loaded_skills.clear()
        self.refresh()
        try:
            self.memory.record("user", text, session=self.session_id)
        except Exception:
            pass

    def record_assistant(self, text: str) -> None:
        if not text:
            return
        try:
            self.memory.record("assistant", text, session=self.session_id)
        except Exception:
            pass

    def system_prompt(self) -> str:
        try:
            return self.prompt.build()
        except Exception:
            return "Eres EzAgent, un asistente de voz local para Windows."

    def close(self) -> None:
        try:
            self.audit.close()
        except Exception:
            pass

    # -------------------------------------------------------- herramientas
    def tools_for(self, text: str) -> list[dict[str, Any]]:
        scripts = any(
            self._skill_has_scripts(name) for name in self._loaded_skills
        )
        names = select_tool_names(
            text,
            scripts_loaded=scripts,
            has_memory=True,
        )
        try:
            return self.registry.schemas(names=names, limit=14)
        except TypeError:
            # Compatibilidad con una primera versión del registro.
            return self.registry.schemas()[:14]

    def _skill_has_scripts(self, name: str) -> bool:
        try:
            skill = self.skills.get(name)
            return bool(getattr(skill, "scripts", ()))
        except Exception:
            return False

    def execute(self, name: str, raw_arguments: str | dict[str, Any]) -> ToolResult:
        if self._calls_this_turn >= self.config.general.max_tool_calls_per_turn:
            return ToolResult(
                ok=False,
                error="He alcanzado el límite de herramientas de este turno.",
            )
        spec = self.registry.get(name)
        if spec is None:
            return ToolResult(ok=False, error=f"Herramienta desconocida: {name}")
        try:
            args = (
                dict(raw_arguments)
                if isinstance(raw_arguments, dict)
                else json.loads(raw_arguments or "{}")
            )
        except (TypeError, ValueError, json.JSONDecodeError):
            return ToolResult(ok=False, error="Argumentos JSON inválidos.")
        if not isinstance(args, dict):
            return ToolResult(ok=False, error="Los argumentos deben ser un objeto JSON.")
        validation_error = self.registry.validate(name, args)
        if validation_error is not None:
            self._audit(name, spec.level, args, "deny", False, 0.0)
            return ToolResult(ok=False, error=validation_error)
        cwd_error = self._validate_delegate_cwd(name, args)
        if cwd_error is not None:
            self._audit(name, 1, args, "deny", False, 0.0)
            return ToolResult(ok=False, error=cwd_error)
        try:
            info = spec.describe(args)
        except Exception as exc:  # noqa: BLE001
            return ToolResult(ok=False, error=f"Argumentos inválidos: {exc}")
        decision = decide(
            info.level,
            self.tainted,
            info.bulk,
            info.always_confirm,
            self.confirm_nondestructive,
            self.config.general.bulk_threshold,
        )
        self._calls_this_turn += 1
        if decision.action == "deny":
            self._audit(name, info.level, args, "deny", False, 0.0)
            return ToolResult(ok=False, error=f"Denegado: {decision.reason}")
        if decision.action in ("confirm", "confirm_strong"):
            summary = info.summary or f"Voy a ejecutar {name}. ¿Confirmas?"
            strong = decision.action == "confirm_strong"
            ok = False
            if self.confirm is not None:
                try:
                    ok = self.confirm.ask(
                        summary,
                        strong=strong,
                        require_number=info.bulk if strong else None,
                    )
                finally:
                    if self.callbacks.on_confirm_done is not None:
                        try:
                            self.callbacks.on_confirm_done()
                        except Exception:
                            pass
            if not ok:
                self._audit(name, info.level, args, "confirm_no", False, 0.0)
                return ToolResult(ok=False, error="El usuario canceló la operación.")
        if info.level == 1 and name in WRITE_TOOLS:
            now = time.monotonic()
            self._level1_times = [
                moment for moment in self._level1_times if now - moment < 60.0
            ]
            if len(self._level1_times) >= 20:
                self._audit(name, info.level, args, "deny", False, 0.0)
                return ToolResult(
                    ok=False,
                    error="Demasiadas operaciones de escritura por minuto.",
                )
            self._level1_times.append(now)
            self._level1_writes += 1
            if self._level1_writes == 4:
                self._audit_warn(
                    "Más de 3 escrituras de nivel 1 en un turno."
                )
        started = time.monotonic()
        try:
            result = self.registry.run(name, args)
        except Exception as exc:  # noqa: BLE001
            result = ToolResult(ok=False, error=f"{type(exc).__name__}: {exc}")
        elapsed = (time.monotonic() - started) * 1000.0
        if result.ok and spec.untrusted_output:
            # # DECISION: delegate_to_agent agrupa acciones de distinta
            # naturaleza. Solo leer el buffer de un agente aporta contenido
            # externo que deba marcar taint; lanzar, enfocar o matar no.
            if name != "delegate_to_agent" or str(
                args.get("action", "")
            ).lower() == "read":
                self.tainted = True
        if name == "load_skill":
            skill_name = str(args.get("name") or "")
            if skill_name:
                self._loaded_skills.add(skill_name)
        self._audit(
            name,
            info.level,
            args,
            "confirm_yes" if decision.action in ("confirm", "confirm_strong") else "allow",
            result.ok,
            elapsed,
        )
        return result

    def _validate_delegate_cwd(
        self, name: str, args: dict[str, Any]
    ) -> str | None:
        """La carpeta de trabajo de Herdr también pasa por la política.

        # DECISION: el documento exige que ninguna herramienta toque disco sin
        # resolve_safe; Herdr recibe ``cwd`` y lo usa para abrir la pestaña, así
        # que se valida contra las raíces de lectura/escritura.
        """
        if name != "delegate_to_agent":
            return None
        action = str(args.get("action", "")).lower()
        raw = args.get("cwd")
        if raw is None or raw == "":
            if action not in ("run", "open_terminal"):
                return None
            # Herdr usa default_cwd cuando el LLM no pasa carpeta; también
            # debe estar dentro de las raíces permitidas.
            raw = getattr(self.voice_config, "default_cwd", None)
            if not raw:
                return None
        if not isinstance(raw, str):
            return "La carpeta de trabajo debe ser texto."
        try:
            resolve_safe(
                raw,
                self.config.workspace,
                self.config.paths.read_roots,
                self.config.paths.deny,
                self.config.paths.deny_globs,
            )
        except PathDenied as exc:
            return f"Carpeta de trabajo no permitida: {exc}"
        return None

    def _audit(
        self,
        tool: str,
        level: int,
        args: dict[str, Any],
        decision: str,
        ok: bool,
        ms: float,
    ) -> None:
        try:
            self.audit.log(
                turn_id=self.turn_id or "no-turn",
                tool=tool,
                level=level,
                args=args,
                decision=decision,
                taint=self.tainted,
                result_ok=bool(ok),
                ms=float(ms),
            )
        except Exception:
            pass

    def render_result(self, result: ToolResult) -> str:
        return ToolRegistry.render_for_llm(result, max_chars=2000)

    # ------------------------------------------------------- confirmación
    @property
    def confirm_pending(self) -> bool:
        return bool(self.confirm is not None and self.confirm.pending)

    def submit_confirmation(self, text: str) -> bool | None:
        if self.confirm is None:
            return None
        return self.confirm.submit_text(text)

    def submit_confirmation_click(self, yes: bool) -> None:
        if self.confirm is not None:
            self.confirm.submit_click(bool(yes))

    def cancel_confirmation(self) -> None:
        if self.confirm is not None:
            self.confirm.cancel()

    def set_confirm_actions(self, enabled: bool) -> None:
        self.confirm_nondestructive = bool(enabled)

    # ------------------------------------------------------------- avisos
    def pending_notify_address(self) -> str | None:
        text = self._get_last_user_text().lower()
        wants = any(
            word in text
            for word in (
                "avísame",
                "avisame",
                "avisa",
                "avísame cuando",
                "me avisas",
                "me avise",
                "me avises",
                "notifícame",
                "notificame",
                "notifica",
                "cuando termine",
                "cuando acabe",
                "cuando hayas terminado",
                "cuando esté",
                "cuando este",
                "al terminar",
                "al acabar",
                "estate atento",
                "estate pendiente",
                "esté atento",
                "este atento",
                "vigila",
                "vigílame",
                "vigilame",
                "pendiente de",
                "me dice",
                "me digas",
                "me dices",
                "dime",
                "me pasas",
                "me cuentas",
                "resumen",
                "resúmeme",
                "resumeme",
            )
        )
        if wants and self.last_agent_address:
            return self.last_agent_address
        return None

    def note_done(
        self,
        address: str,
        agent: str,
        reason: str,
        output: str,
        *,
        task: str = "",
    ) -> None:
        """El mensajero añade el aviso a su historial; aquí no hay nada más."""

"""Herramientas de memoria del agente: remember, recall, forget y compact_memory.

Los textos de confirmación los genera este módulo a partir de los argumentos.
``remember`` nunca escribe sin un sí explícito: si no se inyecta un callback de
confirmación, la operación se cancela.
"""

from __future__ import annotations

import logging
import time
from typing import Any, Callable

from ezagent.agent.config import AgentConfig
from ezagent.agent.confirm import ConfirmationManager
from ezagent.agent.memory import (
    DEFAULT_RECALL_LIMIT,
    MAX_ENTRY_CHARS,
    MEMORY_LIMIT,
    USER_LIMIT,
    MemoryStore,
    summarize_text,
)
from ezagent.agent.registry import ToolInfo, ToolResult, ToolSpec

logger = logging.getLogger(__name__)

ConfirmCallback = Callable[[str, bool], bool]
_REJECT_STATUSES = frozenset({"invalid_kind", "empty", "too_long", "secret", "web"})


def _deny_confirmation(summary: str, strong: bool) -> bool:
    logger.info("Sin callback de confirmación; se cancela: %s", summary)
    return False


# DECISION: el callback inyectado manda; si no, se adapta un ConfirmationManager.
def _resolve_confirmation(
    confirm: ConfirmCallback | None,
    confirmation: ConfirmationManager | None,
) -> ConfirmCallback:
    if confirm is not None:
        return confirm
    if confirmation is not None:
        return lambda summary, strong: bool(
            confirmation.ask(summary, strong=strong)
        )
    return _deny_confirmation


def _safe_ask(ask: ConfirmCallback, summary: str, strong: bool) -> bool:
    try:
        return bool(ask(summary, strong))
    except Exception:
        logger.exception("Falló la confirmación de memoria; se cancela.")
        return False


# DECISION: las herramientas que esperan confirmación reciben más timeout que
# el resto para no cortar mientras el usuario responde.
def _tool_timeout(config: AgentConfig) -> float:
    general = getattr(config, "general", None)
    raw_timeout = getattr(general, "confirm_timeout_s", 15.0)
    try:
        base = float(raw_timeout)
    except (TypeError, ValueError):
        base = 15.0
    return max(30.0, base + 10.0)


def _count_lines(text: Any) -> int:
    return len([line for line in str(text or "").splitlines() if line.strip()])


# DECISION: el registro puede abandonar un manejador por timeout mientras sigue
# vivo en su hilo; se comprueba el plazo antes de escribir para no guardar nada
# después de haber informado de un fallo.
def _deadline(timeout: float) -> float:
    return time.monotonic() + max(0.0, timeout - 1.0)


def build_memory_tools(
    config: AgentConfig,
    store: MemoryStore,
    *,
    confirm: ConfirmCallback | None = None,
    confirmation: ConfirmationManager | None = None,
) -> list[ToolSpec]:
    """Construye las cuatro herramientas de memoria.

    ``confirm(summary, strong) -> bool`` es el callback inyectable. Sin callback
    (y sin ``confirmation``), toda operación que necesite un sí se cancela.
    """
    ask = _resolve_confirmation(confirm, confirmation)
    timeout = _tool_timeout(config)

    def remember_tool(args: dict[str, Any]) -> ToolResult:
        deadline = _deadline(timeout)
        kind = str(args.get("kind", ""))
        text = str(args.get("text", ""))
        validation = store.validate_entry(kind, text)
        if validation.status in _REJECT_STATUSES:
            return ToolResult(ok=False, error=validation.message)
        if validation.status in {"duplicate", "full"}:
            return ToolResult(ok=True, data=validation.to_dict())
        # DECISION: la confirmación corta sigue el ejemplo del documento y no
        # menciona el archivo; el tipo se ve en el resumen de la herramienta.
        question = f"¿Guardo que «{summarize_text(text)}»?"
        if not _safe_ask(ask, question, False):
            return ToolResult(
                ok=True,
                data={
                    "status": "cancelled",
                    "message": "No se guardó: falta la confirmación del usuario.",
                },
            )
        if time.monotonic() > deadline:
            return ToolResult(
                ok=True,
                data={
                    "status": "cancelled",
                    "message": "No se guardó: se agotó el tiempo de confirmación.",
                },
            )
        result = store.remember(kind, text)
        if result.status in _REJECT_STATUSES:
            return ToolResult(ok=False, error=result.message)
        return ToolResult(ok=True, data=result.to_dict())

    def recall_tool(args: dict[str, Any]) -> ToolResult:
        query = str(args.get("query", "")).strip()
        if not query:
            return ToolResult(ok=False, error="La consulta no puede estar vacía.")
        try:
            limit = int(args.get("limit", DEFAULT_RECALL_LIMIT))
        except (TypeError, ValueError):
            limit = DEFAULT_RECALL_LIMIT
        results = store.recall(query, limit=limit)
        payload: dict[str, Any] = {
            "query": query,
            "results": results,
            "count": len(results),
            "backend": store.last_backend,
        }
        if not results:
            payload["message"] = "No encontré nada en el historial."
        return ToolResult(ok=True, data=payload)

    def forget_tool(args: dict[str, Any]) -> ToolResult:
        deadline = _deadline(timeout)
        query = str(args.get("query", "")).strip()
        if not query:
            return ToolResult(ok=False, error="La consulta no puede estar vacía.")
        matches = store.find_matches(query)
        total = sum(len(lines) for lines in matches.values())
        if total == 0:
            return ToolResult(
                ok=True,
                data={
                    "status": "not_found",
                    "removed": 0,
                    "message": (
                        f"No encontré líneas que coincidan con "
                        f"«{summarize_text(query)}»."
                    ),
                },
            )
        detail = ", ".join(
            f"{store.filename(kind)} ({len(lines)})"
            for kind, lines in matches.items()
        )
        question = (
            f"¿Borro {total} línea(s) de {detail} que coinciden con "
            f"«{summarize_text(query)}»? Esta acción no se puede deshacer."
        )
        if not _safe_ask(ask, question, True):
            return ToolResult(
                ok=True,
                data={
                    "status": "cancelled",
                    "message": "No se borró nada: falta la confirmación del usuario.",
                },
            )
        if time.monotonic() > deadline:
            return ToolResult(
                ok=True,
                data={
                    "status": "cancelled",
                    "message": "No se borró nada: se agotó el tiempo de confirmación.",
                },
            )
        result = store.forget(query)
        return ToolResult(ok=True, data=result.to_dict())

    def compact_tool(args: dict[str, Any]) -> ToolResult:
        deadline = _deadline(timeout)
        kind = str(args.get("kind", ""))
        text = str(args.get("text", ""))
        prepared = store.prepare_compaction(kind, text)
        if prepared.status != "ready":
            return ToolResult(ok=False, error=prepared.message)
        question = (
            f"¿Reescribo {store.filename(kind)} con "
            f"{prepared.line_count} línea(s)? Se guardará una copia .bak de la "
            "versión anterior."
        )
        if not _safe_ask(ask, question, True):
            return ToolResult(
                ok=True,
                data={
                    "status": "cancelled",
                    "message": "No se compactó: falta la confirmación del usuario.",
                },
            )
        if time.monotonic() > deadline:
            return ToolResult(
                ok=True,
                data={
                    "status": "cancelled",
                    "message": "No se compactó: se agotó el tiempo de confirmación.",
                },
            )
        result = store.compact_memory(kind, text)
        if result.status != "compacted":
            return ToolResult(ok=False, error=result.message)
        return ToolResult(ok=True, data=result.to_dict())

    def describe_remember(args: dict[str, Any]) -> ToolInfo:
        return ToolInfo(
            level=1,
            summary=(
                f"Guardar en memoria ({args.get('kind', '')}): "
                f"«{summarize_text(args.get('text', ''))}»"
            ),
        )

    def describe_recall(args: dict[str, Any]) -> ToolInfo:
        return ToolInfo(
            level=0,
            summary=(
                f"Buscar en el historial: «{summarize_text(args.get('query', ''))}» "
                f"(límite {args.get('limit', DEFAULT_RECALL_LIMIT)})"
            ),
        )

    def describe_forget(args: dict[str, Any]) -> ToolInfo:
        return ToolInfo(
            level=2,
            summary=(
                "Olvidar líneas que coincidan con "
                f"«{summarize_text(args.get('query', ''))}»"
            ),
        )

    def describe_compact(args: dict[str, Any]) -> ToolInfo:
        return ToolInfo(
            level=2,
            summary=(
                f"Compactar {args.get('kind', '')} con "
                f"{_count_lines(args.get('text', ''))} línea(s)"
            ),
        )

    # DECISION: recall devuelve texto ya guardado por el usuario/asistente, no
    # contenido de herramientas; no se marca como salida no confiable.
    return [
        ToolSpec(
            name="remember",
            description=(
                "Propone guardar un dato estable en la memoria personal "
                "(USER.md o MEMORY.md). El código valida el texto y pide "
                "confirmación; nunca guarda sin un sí."
            ),
            parameters={
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["user", "memory"],
                        "description": (
                            "user = quién es el usuario y sus preferencias; "
                            "memory = hechos y flujo de trabajo."
                        ),
                    },
                    "text": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": MAX_ENTRY_CHARS,
                        "description": "Dato estable, en una sola línea.",
                    },
                },
                "required": ["kind", "text"],
                "additionalProperties": False,
            },
            level=1,
            handler=remember_tool,
            untrusted_output=False,
            timeout_s=timeout,
            describe_fn=describe_remember,
        ),
        ToolSpec(
            name="recall",
            description=(
                "Busca conversaciones anteriores en el historial local. "
                "Devuelve hasta `limit` fragmentos de 300 caracteres."
            ),
            parameters={
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Texto o palabras a buscar.",
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 10,
                        "default": DEFAULT_RECALL_LIMIT,
                        "description": "Número máximo de fragmentos.",
                    },
                },
                "required": ["query"],
                "additionalProperties": False,
            },
            level=0,
            handler=recall_tool,
            untrusted_output=False,
            timeout_s=10.0,
            describe_fn=describe_recall,
        ),
        ToolSpec(
            name="forget",
            description=(
                "Borra de USER.md y MEMORY.md las líneas que coinciden con la "
                "consulta. Requiere confirmación fuerte."
            ),
            parameters={
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "minLength": 1,
                        "description": "Texto de las líneas que se deben olvidar.",
                    }
                },
                "required": ["query"],
                "additionalProperties": False,
            },
            level=2,
            handler=forget_tool,
            untrusted_output=False,
            timeout_s=timeout,
            describe_fn=describe_forget,
        ),
        ToolSpec(
            name="compact_memory",
            description=(
                "Reescribe USER.md o MEMORY.md con una versión compactada "
                "acordada con el usuario. Requiere confirmación y deja una copia "
                ".bak."
            ),
            parameters={
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["user", "memory"],
                        "description": "Archivo que se va a compactar.",
                    },
                    "text": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": max(USER_LIMIT, MEMORY_LIMIT),
                        "description": (
                            "Contenido completo de la versión fusionada, en "
                            "viñetas «- [AAAA-MM-DD] texto»."
                        ),
                    },
                },
                "required": ["kind", "text"],
                "additionalProperties": False,
            },
            level=2,
            handler=compact_tool,
            untrusted_output=False,
            timeout_s=timeout,
            describe_fn=describe_compact,
        ),
    ]

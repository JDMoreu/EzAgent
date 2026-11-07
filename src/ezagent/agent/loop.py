"""Bucle de herramientas acotado (máximo 4 llamadas por turno)."""

from __future__ import annotations

import json
import time
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Protocol

from .audit import AuditLog
from .policy import decide
from .registry import ToolRegistry, ToolResult

if TYPE_CHECKING:  # se evita el ciclo de importación con confirm.py
    from .confirm import ConfirmationManager

_MAX_STEPS = 4


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
    def chat(
        self, messages: list[dict[str, Any]], tools: list[dict[str, Any]]
    ) -> AssistantReply: ...


def _assistant_message(reply: AssistantReply) -> dict[str, Any]:
    message: dict[str, Any] = {"role": "assistant", "content": reply.text or ""}
    if reply.tool_calls:
        message["tool_calls"] = [
            {
                "id": call.id,
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": json.dumps(
                        call.arguments, ensure_ascii=False, default=str
                    ),
                },
            }
            for call in reply.tool_calls
        ]
    return message


def _tool_message(call: ToolCall, result: ToolResult) -> dict[str, Any]:
    return {
        "role": "tool",
        "tool_call_id": call.id,
        "name": call.name,
        "content": ToolRegistry.render_for_llm(result),
    }


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
) -> str:
    """Ejecuta un turno de herramienta en herramienta y devuelve el texto final."""
    # DECISION: el tope de seguridad es 4; un valor mayor del llamador no lo
    # amplía. El presupuesto se comparte entre pasos para no superar 4 llamadas.
    steps = max(1, min(int(max_steps), _MAX_STEPS))
    remaining_calls = steps
    tainted = False
    fallback = "Llegué al límite de pasos. ¿Seguimos?"

    for _ in range(steps):
        if remaining_calls <= 0:
            return fallback
        reply = llm.chat(messages, registry.schemas())
        calls = reply.tool_calls or []
        if not calls:
            return reply.text or ""
        messages.append(_assistant_message(reply))

        for index, call in enumerate(calls):
            if remaining_calls <= 0:
                # Se contestan las llamadas pendientes para no dejar el
                # historial con tool_calls sin respuesta.
                for pending in calls[index:]:
                    messages.append(
                        _tool_message(
                            pending,
                            ToolResult(
                                ok=False,
                                error="Se alcanzó el límite de llamadas de este turno.",
                            ),
                        )
                    )
                return fallback
            remaining_calls -= 1

            tool = registry.get(call.name)
            if tool is None:
                result = ToolResult(
                    ok=False, error=f"Herramienta desconocida: {call.name}."
                )
                # DECISION: una herramienta desconocida se audita como nivel 3
                # (prohibida) y nunca se ejecuta.
                audit.log(
                    turn_id=turn_id,
                    tool=call.name,
                    level=3,
                    args=call.arguments if isinstance(call.arguments, dict) else {},
                    decision="deny",
                    taint=tainted,
                    result_ok=False,
                    ms=0.0,
                )
                messages.append(_tool_message(call, result))
                continue

            args = call.arguments if isinstance(call.arguments, dict) else None
            if args is None:
                result = ToolResult(
                    ok=False,
                    error="Los argumentos de la herramienta deben ser un objeto JSON.",
                )
                tainted = tainted or tool.untrusted_output
                audit.log(
                    turn_id=turn_id,
                    tool=call.name,
                    level=tool.level,
                    args={},
                    decision="deny",
                    taint=tainted,
                    result_ok=False,
                    ms=0.0,
                )
                messages.append(_tool_message(call, result))
                continue

            try:
                info = tool.describe(args)
            except Exception as exc:
                result = ToolResult(
                    ok=False, error=f"No se pudo interpretar la llamada: {exc}"
                )
                tainted = tainted or tool.untrusted_output
                audit.log(
                    turn_id=turn_id,
                    tool=call.name,
                    level=tool.level,
                    args=args,
                    decision="deny",
                    taint=tainted,
                    result_ok=False,
                    ms=0.0,
                )
                messages.append(_tool_message(call, result))
                continue

            decision = decide(
                info.level,
                tainted,
                info.bulk,
                info.always_confirm,
                confirm_nondestructive,
                bulk_threshold,
            )
            elapsed_ms = 0.0

            if decision.action == "deny":
                result = ToolResult(ok=False, error=f"Denegado: {decision.reason}.")
                audit_decision = "deny"
            elif decision.action == "allow":
                started = time.perf_counter()
                result = registry.run(call.name, args)
                elapsed_ms = (time.perf_counter() - started) * 1000.0
                audit_decision = "allow"
            else:
                summary = info.summary or f"Voy a ejecutar {call.name}. ¿Confirmas?"
                accepted = False
                if confirm is not None:
                    require_number = (
                        info.bulk
                        if decision.action == "confirm_strong" and info.bulk > 0
                        else None
                    )
                    try:
                        accepted = confirm.ask(
                            summary,
                            strong=(decision.action == "confirm_strong"),
                            require_number=require_number,
                        )
                    except Exception:
                        accepted = False
                if accepted:
                    started = time.perf_counter()
                    result = registry.run(call.name, args)
                    elapsed_ms = (time.perf_counter() - started) * 1000.0
                    audit_decision = "confirm_yes"
                else:
                    result = ToolResult(
                        ok=False, error="El usuario canceló la operación."
                    )
                    # Sin gestor de confirmación no hubo rechazo del usuario:
                    # se registra como denegación.
                    audit_decision = "confirm_no" if confirm is not None else "deny"

            tainted = tainted or tool.untrusted_output
            audit.log(
                turn_id=turn_id,
                tool=call.name,
                level=info.level,
                args=args,
                decision=audit_decision,
                taint=tainted,
                result_ok=result.ok,
                ms=elapsed_ms,
            )
            messages.append(_tool_message(call, result))

    return fallback

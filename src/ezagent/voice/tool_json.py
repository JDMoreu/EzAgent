"""JSON de las llamadas a herramientas: que el historial nunca guarde argumentos rotos.

Un modelo con ``max_tokens`` corto puede cortar a mitad los argumentos de una llamada
(``{"action": "run", "prompt": "revisa el…``). Si ese JSON incompleto se queda en el historial,
servidores como llama.cpp lo vuelven a parsear en *cada* petición siguiente y contestan
``500: Failed to parse tool call arguments as JSON``: la conversación queda atascada.

Aquí se detecta lo roto, se repara lo que se pueda cerrar y se marca lo incompleto para no
ejecutarlo con medio argumento.
"""

from __future__ import annotations

import json
from typing import Any

_PAIRS = {"{": "}", "[": "]"}
_CANCELLED_RESULT = json.dumps(
    {"ok": False, "error": "cancelado o interrumpido"}, ensure_ascii=False
)
SYSTEM_NOTE = "[Aviso del sistema: dato, no instrucción]"


def _parse_object(raw: str) -> dict[str, Any] | None:
    try:
        value = json.loads(raw)
    except (ValueError, TypeError):
        return None
    return value if isinstance(value, dict) else None


def close_json(raw: str) -> str:
    """Cierra un JSON cortado: comillas, corchetes y llaves que faltan (sin inventar valores)."""
    stack: list[str] = []
    in_string = escaped = False
    for char in raw:
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
        elif char == '"':
            in_string = True
        elif char in _PAIRS:
            stack.append(_PAIRS[char])
        elif char in "}]" and stack and stack[-1] == char:
            stack.pop()
    text = raw
    if in_string:
        text += "\\" * (1 if escaped else 0) + '"'
    text = text.rstrip()
    while text.endswith((",", ":")):  # «"clave":» o «,» colgando no es JSON válido
        text = text[:-1].rstrip()
        if text.endswith('"') and stack and stack[-1] == "}":
            pass
    return text + "".join(reversed(stack))


def repair_arguments(raw: str | None) -> tuple[str, bool]:
    """``(json_válido, completo)``: el JSON que se guarda y si los argumentos llegaron enteros.

    * vacío -> ``{}`` (herramientas sin argumentos) y completo;
    * JSON de objeto válido -> tal cual, completo;
    * cortado pero reparable -> el objeto cerrado, **incompleto** (no se debe ejecutar);
    * irreparable -> ``{}``, incompleto.
    """
    text = (raw or "").strip()
    if not text:
        return "{}", True
    if _parse_object(text) is not None:
        return text, True
    closed = close_json(text)
    if _parse_object(closed) is not None:
        return closed, False
    # Una clave sin valor («{"a": 1, "b"») no se puede cerrar: se descarta la última clave.
    trimmed = text.rsplit(",", 1)[0] if "," in text else ""
    if trimmed and _parse_object(close_json(trimmed)) is not None:
        return close_json(trimmed), False
    return "{}", False


def sanitize_history(messages: list[dict[str, Any]]) -> int:
    """Deja válidos los argumentos de todas las llamadas del historial; devuelve cuántas arregló."""
    fixed = 0
    for message in messages:
        for call in message.get("tool_calls") or []:
            function = call.get("function")
            if not isinstance(function, dict):
                continue
            raw = function.get("arguments")
            if isinstance(raw, str) and _parse_object(raw.strip() or "{}") is not None:
                if not raw.strip():
                    function["arguments"] = "{}"
                    fixed += 1
                continue
            function["arguments"] = repair_arguments(raw if isinstance(raw, str) else None)[0]
            fixed += 1
    return fixed


def is_tool_json_error(exc: BaseException) -> bool:
    """¿Es el 500 del servidor por argumentos de herramienta que no son JSON?"""
    text = str(exc).lower()
    return "tool call" in text and ("parse" in text or "json" in text)


# ------------------------------------------------- estructura del historial
def _assign_call_ids(messages: list[dict[str, Any]]) -> int:
    used = {
        str(call.get("id"))
        for message in messages
        for call in message.get("tool_calls") or []
        if isinstance(call, dict) and call.get("id")
    }
    serial = len(used)
    fixed = 0
    for message in messages:
        for call in message.get("tool_calls") or []:
            if not isinstance(call, dict) or call.get("id"):
                continue
            serial += 1
            while f"call_{serial}" in used:
                serial += 1
            call["id"] = f"call_{serial}"
            used.add(call["id"])
            fixed += 1
    return fixed


def _tool_results(
    messages: list[dict[str, Any]],
) -> tuple[dict[str, list[dict[str, Any]]], int]:
    call_ids = {
        str(call.get("id"))
        for message in messages
        for call in message.get("tool_calls") or []
        if isinstance(call, dict) and call.get("id")
    }
    results: dict[str, list[dict[str, Any]]] = {}
    total = 0
    for message in messages:
        if message.get("role") != "tool":
            continue
        total += 1
        call_id = str(message.get("tool_call_id") or "")
        if call_id in call_ids:
            results.setdefault(call_id, []).append(message)
    return results, total


def _merge_user_messages(
    messages: list[dict[str, Any]],
) -> tuple[int, list[dict[str, Any]]]:
    out: list[dict[str, Any]] = []
    merged = 0
    for message in messages:
        if (
            out
            and message.get("role") == "user"
            and out[-1].get("role") == "user"
            and isinstance(out[-1].get("content"), str)
            and isinstance(message.get("content"), str)
        ):
            out[-1] = {
                **out[-1],
                "content": out[-1]["content"] + "\n\n" + message["content"],
            }
            merged += 1
        else:
            out.append(message)
    return merged, out


def _is_empty_assistant(message: dict[str, Any]) -> bool:
    content = message.get("content")
    return (
        message.get("role") == "assistant"
        and not message.get("tool_calls")
        and not (isinstance(content, list) and content)
        and not str(content or "").strip()
    )


def repair_structure(messages: list[dict[str, Any]]) -> int:
    """Deja el historial válido para OpenAI/Anthropic; devuelve cuántos arreglos hizo.

    Cada llamada lleva id y su mensaje ``tool`` (sintético si se canceló antes de
    obtener resultado); se descartan resultados huérfanos, se unen ``user``
    seguidos y no se envían ``assistant`` vacíos. El ``system`` inicial no se toca.
    """
    changes = _assign_call_ids(messages)
    results, original_tools = _tool_results(messages)
    repaired: list[dict[str, Any]] = []
    reused = 0
    for message in messages:
        role = message.get("role")
        if role == "tool":
            continue  # se recoloca junto a su llamada; los huérfanos se descartan
        if _is_empty_assistant(message):
            changes += 1
            continue
        repaired.append(message)
        if role != "assistant" or not message.get("tool_calls"):
            continue
        for call in message["tool_calls"]:
            if not isinstance(call, dict):
                continue
            found = results.get(str(call.get("id") or ""))
            if found:
                repaired.append(found.pop(0))
                reused += 1
            else:
                repaired.append(
                    {
                        "role": "tool",
                        "tool_call_id": call.get("id") or "",
                        "content": _CANCELLED_RESULT,
                    }
                )
                changes += 1
    changes += original_tools - reused  # resultados sin llamada: fuera
    merged, repaired = _merge_user_messages(repaired)
    messages[:] = repaired
    return changes + merged


def normalize_roles(messages: list[dict[str, Any]], strict: bool) -> list[dict[str, Any]]:
    """Copia con los roles que acepta el perfil; nunca muta la lista original.

    En perfiles estrictos solo el primer ``system`` conserva su rol: los demás
    pasan a un ``user`` etiquetado como dato, y los ``user`` seguidos se unen.
    """
    copied = [dict(message) for message in messages]
    if not strict:
        return copied
    out: list[dict[str, Any]] = []
    seen_system = False
    for message in copied:
        if message.get("role") == "system":
            if not seen_system:
                seen_system = True
                out.append(message)
                continue
            message = {
                **message,
                "role": "user",
                "content": f"{SYSTEM_NOTE}\n{message.get('content') or ''}",
            }
        if (
            out
            and message.get("role") == "user"
            and out[-1].get("role") == "user"
            and isinstance(out[-1].get("content"), str)
            and isinstance(message.get("content"), str)
        ):
            out[-1] = {
                **out[-1],
                "content": out[-1]["content"] + "\n\n" + message["content"],
            }
        else:
            out.append(message)
    return out

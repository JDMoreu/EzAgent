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

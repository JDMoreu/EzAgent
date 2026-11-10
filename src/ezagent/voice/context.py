"""Memoria de contexto: medir la conversación y compactar lo viejo en un resumen.

Un modelo tiene una ventana de contexto finita. Cortar el historial a un número fijo de
mensajes pierde en silencio lo hablado antes. Aquí se hace distinto:

* se **estima** cuántos tokens ocupa la conversación;
* al acercarse al límite configurado, lo más viejo se **resume** (hechos, decisiones, rutas,
  pendientes) y solo las últimas vueltas siguen enteras;
* los resultados de herramientas antiguos (archivos leídos, páginas) se **recortan**, porque
  son lo que más contexto gasta y lo que menos se vuelve a necesitar.

Todo es lógica pura; ``llm.Messenger`` pone la llamada al modelo.
"""

from __future__ import annotations

import json
from typing import Any, Final

CHARS_PER_TOKEN: Final[float] = 3.2  # español y código: ~3,2 caracteres por token
MESSAGE_OVERHEAD: Final[int] = 6  # tokens de formato por mensaje
COMPACT_AT: Final[float] = 0.70  # se compacta al llegar al 70 % del límite…
TARGET_AT: Final[float] = 0.50  # …y se baja hasta el 50 %: así no se resume en cada turno
CLIP_AT: Final[float] = 0.45  # a partir del 45 % se recortan los resultados viejos de herramientas
KEEP_TURNS: Final[int] = 4  # vueltas recientes que se conservan enteras
SUMMARY_TOKENS: Final[int] = 900
TOOL_RESULT_KEEP: Final[int] = (
    1500  # caracteres que se guardan de un resultado de herramienta viejo
)
TRANSCRIPT_CHARS: Final[int] = 48_000  # tope de lo que se le pasa al modelo para resumir
MIN_BUDGET: Final[int] = 4_000
RESERVE_MARGIN: Final[int] = 500  # tokens de formato/plantilla que no se cuentan aparte

SUMMARY_SYSTEM: Final[str] = (
    "Eres el archivista de una conversación entre un usuario y un asistente de voz y chat. "
    "Resume la conversación para que el asistente pueda continuarla sin releerla. "
    "Escribe en español, en puntos breves, sin saludos ni relleno. Conserva SIEMPRE: lo que el "
    "usuario quiere o pidió, hechos y cifras dichos, decisiones tomadas, nombres, rutas de "
    "archivos y direcciones exactas, tareas lanzadas o pendientes y su estado, y preferencias "
    "del usuario. Omite lo irrelevante y el contenido largo de herramientas (resúmelo en una línea). "
    "Si hay un resumen previo, intégralo en el nuevo sin perder nada importante. Máximo 350 palabras."
)


def _text_of(message: dict[str, Any]) -> str:
    content = message.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):  # formato por partes
        return " ".join(str(part.get("text", "")) for part in content if isinstance(part, dict))
    return ""


def estimate_tokens(messages: list[dict[str, Any]]) -> int:
    """Estimación de tokens de una lista de mensajes (barata, sin tokenizador)."""
    chars = 0
    for message in messages:
        chars += len(_text_of(message))
        for call in message.get("tool_calls") or []:
            function = call.get("function") or {}
            chars += len(str(function.get("name", ""))) + len(str(function.get("arguments", "")))
    return int(chars / CHARS_PER_TOKEN) + MESSAGE_OVERHEAD * len(messages)


def estimate_tools_tokens(schemas: list[dict[str, Any]] | None) -> int:
    """Tokens estimados de los esquemas de herramientas tal como se envían."""
    if not schemas:
        return 0
    chars = 0
    for schema in schemas:
        if isinstance(schema, dict):
            chars += len(json.dumps(schema, ensure_ascii=False, separators=(",", ":")))
        else:
            chars += len(str(schema))
    return int(chars / CHARS_PER_TOKEN) + MESSAGE_OVERHEAD * len(schemas)


def reserve_tokens(tools_schemas: list[dict[str, Any]] | None, max_tokens: int) -> int:
    """Tokens que hay que reservar: esquemas reales + salida + margen de formato."""
    try:
        output = max(0, int(max_tokens))
    except (TypeError, ValueError):
        output = 0
    return estimate_tools_tokens(tools_schemas) + output + RESERVE_MARGIN


def budget_tokens(configured: Any) -> int:
    """Límite de contexto configurado, con un mínimo razonable."""
    try:
        value = int(configured)
    except (TypeError, ValueError):
        value = 32_000
    return max(MIN_BUDGET, value)


def needs_compaction(messages: list[dict[str, Any]], budget: int, reserve: int = 0) -> bool:
    """¿Hay que resumir? ``reserve`` son los tokens que ocuparán las herramientas y la respuesta."""
    return estimate_tokens(messages) + reserve > budget * COMPACT_AT


def needs_clipping(messages: list[dict[str, Any]], budget: int) -> bool:
    return estimate_tokens(messages) > budget * CLIP_AT


def split_history(
    messages: list[dict[str, Any]], keep_turns: int = KEEP_TURNS
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    """``(viejo, reciente)`` del historial sin el mensaje de sistema.

    ``reciente`` empieza siempre en un mensaje del usuario (nunca parte una llamada a
    herramienta de su resultado) y tiene como mucho ``keep_turns`` vueltas.
    """
    history = messages[1:]
    starts = [i for i, m in enumerate(history) if m.get("role") == "user"]
    if len(starts) <= keep_turns:
        return [], history
    cut = starts[-keep_turns]
    return history[:cut], history[cut:]


def fit_recent(
    messages: list[dict[str, Any]], budget: int, reserve: int = 0
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    """``(viejo, reciente)`` con las vueltas recientes que caben en el 50 % del límite.

    Empieza conservando ``KEEP_TURNS`` vueltas y baja hasta una si hace falta; así, tras
    compactar queda holgura y el siguiente turno no vuelve a disparar un resumen.
    """
    system = messages[:1]
    for keep in range(KEEP_TURNS, 0, -1):
        old, recent = split_history(messages, keep)
        if not old:
            return old, recent
        if estimate_tokens(system + recent) + reserve <= budget * TARGET_AT or keep == 1:
            return old, recent
    return split_history(messages, 1)


def render_transcript(old: list[dict[str, Any]], previous_summary: str = "") -> str:
    """Texto plano de lo viejo para que el modelo lo resuma (recortado a un tamaño sensato)."""
    lines: list[str] = []
    header = ""
    if previous_summary:
        header = "RESUMEN PREVIO:\n" + previous_summary.strip() + "\n\nCONVERSACIÓN NUEVA:\n"
    for message in old:
        role, text = message.get("role"), " ".join(_text_of(message).split())
        if role == "user":
            lines.append(f"Usuario: {text}")
        elif role == "assistant":
            calls = message.get("tool_calls") or []
            for call in calls:
                fn = call.get("function") or {}
                args = str(fn.get("arguments", ""))
                lines.append(f"[herramienta {fn.get('name', '?')} {args[:180]}]")
            if text:
                lines.append(f"Asistente: {text}")
        elif role == "tool":
            lines.append(f"[resultado] {text[:300]}")
    body = "\n".join(lines)
    if len(body) > TRANSCRIPT_CHARS:  # lo más antiguo es lo menos útil: se conserva el final
        body = "…\n" + body[-TRANSCRIPT_CHARS:]
    return header + body  # el resumen previo nunca se recorta: es lo que no se puede perder


def summary_messages(old: list[dict[str, Any]], previous_summary: str = "") -> list[dict[str, str]]:
    """Mensajes para pedirle al modelo el resumen."""
    return [
        {"role": "system", "content": SUMMARY_SYSTEM},
        {"role": "user", "content": render_transcript(old, previous_summary)},
    ]


def with_summary(system_prompt: str, summary: str) -> str:
    """El prompt de sistema con el resumen de lo anterior al final."""
    if not summary.strip():
        return system_prompt
    return (
        f"{system_prompt}\n\n# Resumen de la conversación hasta ahora\n"
        f"{summary.strip()}\n(Es lo hablado antes; lo reciente sigue a continuación.)"
    )


def _clip_tool_content(content: str) -> str:
    """Recorta un resultado largo; si era JSON, el resultado sigue siendo JSON válido."""
    try:
        json.loads(content)
    except (TypeError, ValueError):
        omitted = len(content) - TOOL_RESULT_KEEP
        return content[:TOOL_RESULT_KEEP] + f"… [recortado: {omitted} caracteres]"
    # DECISION: ``data`` guarda el prefijo literal (recuperable) en vez de reserializar
    # la estructura; la mitad deja sitio al envoltorio y evita recortes anidados.
    payload = {
        "truncated": True,
        "original_chars": len(content),
        "data": content[: TOOL_RESULT_KEEP // 2],
    }
    return json.dumps(payload, ensure_ascii=False)


def clip_old_tool_results(messages: list[dict[str, Any]], protect_last_turns: int = 1) -> int:
    """Recorta los resultados de herramientas de vueltas antiguas; devuelve cuántos recortó.

    ``protect_last_turns`` vueltas finales se respetan (una pregunta de seguimiento puede
    necesitar lo que acaba de leerse); con 0 se recorta todo.
    """
    starts = [i for i, m in enumerate(messages) if m.get("role") == "user"]
    if protect_last_turns <= 0:
        boundary = len(messages)  # último recurso: se recorta todo
    else:
        boundary = starts[-protect_last_turns] if len(starts) >= protect_last_turns else 0
    clipped = 0
    for message in messages[:boundary]:
        if message.get("role") != "tool":
            continue
        content = message.get("content")
        already = isinstance(content, str) and "… [recortado:" in content[-80:]
        if isinstance(content, str) and len(content) > TOOL_RESULT_KEEP and not already:
            message["content"] = _clip_tool_content(content)
            clipped += 1
    return clipped


def describe(tokens: int, budget: int) -> str:
    """«12 k de 32 k» para registros y avisos."""
    return f"{tokens / 1000:.0f} k de {budget / 1000:.0f} k tokens"


__all__ = [
    "KEEP_TURNS",
    "RESERVE_MARGIN",
    "budget_tokens",
    "clip_old_tool_results",
    "describe",
    "estimate_tokens",
    "estimate_tools_tokens",
    "json",
    "needs_clipping",
    "needs_compaction",
    "render_transcript",
    "reserve_tokens",
    "split_history",
    "summary_messages",
    "with_summary",
]

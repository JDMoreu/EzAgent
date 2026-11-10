"""Registro de herramientas, validación mínima y ejecución con timeout."""

from __future__ import annotations

import json
import logging
import threading
from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass
from typing import Any

logger = logging.getLogger(__name__)

_JSON_TYPES = {"string", "integer", "number", "boolean", "array", "object", "null"}
_TYPE_LABELS = {
    "string": "texto",
    "integer": "entero",
    "number": "número",
    "boolean": "booleano",
    "array": "lista",
    "object": "objeto",
    "null": "nulo",
}


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
    # completed: el manejador terminó; failed: terminó con error;
    # unknown_outcome: el registro dejó de esperarlo y no se sabe si actuó.
    outcome: str = "completed"
    error_kind: str | None = None

    def to_dict(self) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "ok": self.ok,
            "data": self.data,
            "error": self.error,
            "outcome": self.outcome,
        }
        if self.error_kind is not None:
            payload["error_kind"] = self.error_kind
        return payload


@dataclass(frozen=True)
class ToolSpec:
    name: str
    description: str
    parameters: dict[str, Any]
    level: int
    handler: Callable[[dict[str, Any]], ToolResult | dict[str, Any]]
    untrusted_output: bool = False
    timeout_s: float = 10.0
    describe_fn: Callable[[dict[str, Any]], ToolInfo] | None = None

    def schema(self) -> dict[str, Any]:
        return {
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            },
        }

    def describe(self, args: dict[str, Any]) -> ToolInfo:
        if self.describe_fn is None:
            return ToolInfo(level=self.level)
        info = self.describe_fn(args)
        if not isinstance(info, ToolInfo):
            raise TypeError("describe_fn debe devolver ToolInfo")
        return info


def _matches_type(value: Any, expected: Any) -> bool:
    if isinstance(expected, list):
        return any(_matches_type(value, item) for item in expected if isinstance(item, str))
    if not isinstance(expected, str) or expected not in _JSON_TYPES:
        return True  # tipo desconocido: no se valida
    if expected == "string":
        return isinstance(value, str)
    if expected == "integer":
        if isinstance(value, bool):
            return False
        if isinstance(value, int):
            return True
        return isinstance(value, float) and value.is_integer()
    if expected == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if expected == "boolean":
        return isinstance(value, bool)
    if expected == "array":
        return isinstance(value, list)
    if expected == "object":
        return isinstance(value, dict)
    return value is None  # null


def _type_label(expected: Any) -> str:
    if isinstance(expected, list):
        return " o ".join(_type_label(item) for item in expected)
    return _TYPE_LABELS.get(expected, str(expected))


def _is_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def _join_name(parent: str, child: str) -> str:
    return f"{parent}.{child}" if parent else child


def _validate_object(name: str, value: dict[str, Any], spec: dict[str, Any]) -> str | None:
    properties = spec.get("properties")
    if not isinstance(properties, dict):
        properties = {}
    required = spec.get("required")
    if isinstance(required, list):
        for key in required:
            if isinstance(key, str) and key not in value:
                return f"Falta el parámetro obligatorio: '{_join_name(name, key)}'."
    if spec.get("additionalProperties") is False:
        for key in value:
            if key not in properties:
                return f"El parámetro '{_join_name(name, str(key))}' no está permitido."
    for key, item in value.items():
        child = properties.get(key)
        if isinstance(child, dict):
            error = _validate_value(_join_name(name, str(key)), item, child)
            if error is not None:
                return error
    return None


def _validate_value(name: str, value: Any, spec: dict[str, Any]) -> str | None:
    expected = spec.get("type")
    if expected is not None and not _matches_type(value, expected):
        return f"El parámetro '{name}' debe ser de tipo {_type_label(expected)}."
    enum = spec.get("enum")
    if isinstance(enum, list) and value not in enum:
        options = ", ".join(repr(option) for option in enum)
        return f"El parámetro '{name}' debe ser uno de: {options}."
    if isinstance(value, str):
        min_length = spec.get("minLength")
        max_length = spec.get("maxLength")
        if _is_number(min_length) and len(value) < min_length:
            return f"El parámetro '{name}' debe tener al menos {min_length:g} caracteres."
        if _is_number(max_length) and len(value) > max_length:
            return f"El parámetro '{name}' debe tener como máximo {max_length:g} caracteres."
    if _is_number(value):
        minimum = spec.get("minimum")
        maximum = spec.get("maximum")
        if _is_number(minimum) and value < minimum:
            return f"El parámetro '{name}' debe ser mayor o igual que {minimum}."
        if _is_number(maximum) and value > maximum:
            return f"El parámetro '{name}' debe ser menor o igual que {maximum}."
    if isinstance(value, list):
        min_items = spec.get("minItems")
        max_items = spec.get("maxItems")
        if _is_number(min_items) and len(value) < min_items:
            return f"El parámetro '{name}' debe tener al menos {min_items:g} elementos."
        if _is_number(max_items) and len(value) > max_items:
            return f"El parámetro '{name}' debe tener como máximo {max_items:g} elementos."
        items = spec.get("items")
        if isinstance(items, dict):
            for index, item in enumerate(value):
                error = _validate_value(f"{name}[{index}]", item, items)
                if error is not None:
                    return error
    if isinstance(value, dict):
        return _validate_object(name, value, spec)
    return None


def _validate_args(schema: dict[str, Any], args: dict[str, Any]) -> str | None:
    return _validate_object("", args, schema)


def _call_with_timeout(
    handler: Callable[[dict[str, Any]], ToolResult | dict[str, Any]],
    args: dict[str, Any],
    timeout_s: float,
) -> tuple[ToolResult | dict[str, Any] | None, str | None, str]:
    """Ejecuta el manejador en un hilo daemon y espera como máximo timeout_s.

    Los manejadores con E/S deben usar un timeout interno menor que el del
    registro para terminar solos; si aun así se agota, el resultado es
    ``unknown_outcome`` porque el hilo puede seguir actuando.
    """
    outcome: dict[str, Any] = {}

    def runner() -> None:
        try:
            outcome["value"] = handler(args)
        except BaseException as exc:  # se normaliza a ToolResult(ok=False)
            outcome["error"] = exc

    timeout = 10.0 if timeout_s is None else max(float(timeout_s), 0.0)
    thread = threading.Thread(target=runner, name="ezagent-tool", daemon=True)
    thread.start()
    thread.join(timeout)
    if thread.is_alive():
        message = (
            f"La herramienta superó el tiempo límite de {timeout:g} s. "
            "No se sabe si la operación llegó a completarse; no la repitas "
            "sin comprobar antes el resultado."
        )
        return None, message, "unknown_outcome"
    error = outcome.get("error")
    if error is not None:
        return None, f"Error al ejecutar la herramienta: {type(error).__name__}: {error}", "failed"
    return outcome.get("value"), None, "completed"


def _normalize_result(value: ToolResult | dict[str, Any] | None) -> ToolResult:
    if isinstance(value, ToolResult):
        return value
    if isinstance(value, dict):
        if {"ok", "data", "error"} & set(value):
            ok = value.get("ok", "error" not in value)
            error = value.get("error")
            error_kind = value.get("error_kind")
            return ToolResult(
                ok=bool(ok),
                data=value.get("data"),
                error=None if error is None else str(error),
                outcome=str(value.get("outcome") or "completed"),
                error_kind=None if error_kind is None else str(error_kind),
            )
        # DECISION: un dict sin las claves del contrato se devuelve como dato.
        return ToolResult(ok=True, data=value)
    return ToolResult(ok=True, data=value)


class ToolRegistry:
    def __init__(self) -> None:
        self._tools: dict[str, ToolSpec] = {}
        self._lock = threading.Lock()

    def register(self, tool: ToolSpec) -> None:
        if not isinstance(tool, ToolSpec):
            raise TypeError("El registro solo acepta ToolSpec")
        with self._lock:
            if tool.name in self._tools:
                # DECISION: un duplicado se reemplaza y se avisa; así una
                # integración puede corregir una herramienta ya registrada.
                logger.warning("Herramienta duplicada '%s': se reemplaza", tool.name)
            self._tools[tool.name] = tool

    def register_many(self, tools: Iterable[ToolSpec]) -> None:
        for tool in tools:
            self.register(tool)

    def get(self, name: str) -> ToolSpec | None:
        with self._lock:
            return self._tools.get(name)

    def names(self) -> list[str]:
        with self._lock:
            return list(self._tools)

    def schemas(
        self,
        *,
        names: Sequence[str] | None = None,
        limit: int = 14,
    ) -> list[dict[str, Any]]:
        if limit <= 0:
            return []
        selected = list(names) if names is not None else self.names()
        tools: list[ToolSpec] = []
        for name in selected:
            tool = self.get(name)
            if tool is not None:
                tools.append(tool)
            if len(tools) >= limit:
                break
        return [tool.schema() for tool in tools]

    def validate(self, name: str, args: dict[str, Any]) -> str | None:
        """Comprueba el esquema sin ejecutar la herramienta."""
        tool = self.get(name)
        if tool is None:
            return f"Herramienta desconocida: {name}."
        if not isinstance(args, dict):
            return "Los argumentos de la herramienta deben ser un objeto JSON."
        return _validate_args(tool.parameters, args)

    def run(self, name: str, args: dict[str, Any]) -> ToolResult:
        tool = self.get(name)
        if tool is None:
            return ToolResult(ok=False, error=f"Herramienta desconocida: {name}.", outcome="failed")
        if not isinstance(args, dict):
            return ToolResult(
                ok=False,
                error="Los argumentos de la herramienta deben ser un objeto JSON.",
                outcome="failed",
            )
        validation_error = _validate_args(tool.parameters, args)
        if validation_error is not None:
            return ToolResult(ok=False, error=validation_error, outcome="failed")
        value, failure, outcome = _call_with_timeout(tool.handler, args, tool.timeout_s)
        if failure is not None:
            return ToolResult(ok=False, error=failure, outcome=outcome)
        return _normalize_result(value)

    @staticmethod
    def render_for_llm(result: ToolResult, max_chars: int = 2000) -> str:
        """Serializa el resultado sin superar ``max_chars`` y siempre como JSON.

        Si no cabe, recorta las cadenas de ``data`` antes de serializar y
        envuelve el resultado con ``truncated`` y ``original_chars``. Un límite
        menor de 2 caracteres no permite ningún JSON: se devuelve "".
        """
        if max_chars <= 0:
            return ""
        full = _dumps(result.to_dict())
        if len(full) <= max_chars:
            return full

        original_chars = len(full)
        base: dict[str, Any] = {"ok": result.ok, "outcome": result.outcome}
        if result.error_kind is not None:
            base["error_kind"] = result.error_kind
        error_text = None if result.error is None else str(result.error)
        meta: dict[str, Any] = {
            "truncated": True,
            "original_chars": original_chars,
            "note": _TRUNCATED_NOTE,
        }

        best: str | None = None
        low, high = 0, max(_max_string_len(result.data), len(error_text or ""))
        while low <= high:
            middle = (low + high) // 2
            payload = {**base, "data": _clip_strings(result.data, middle), **meta}
            if error_text is not None:
                payload["error"] = _clip_text(error_text, middle)
            text = _dumps(payload)
            if len(text) <= max_chars:
                best = text
                low = middle + 1
            else:
                high = middle - 1
        if best is not None:
            return best

        # El esqueleto de data ya no cabe: se conserva solo la información
        # crítica y, si tampoco cabe, el JSON válido más pequeño posible.
        for payload in (
            {**base, "data": None, **meta},
            {"truncated": True, "original_chars": original_chars, "note": _TRUNCATED_NOTE},
            {"truncated": True, "original_chars": original_chars},
            {"truncated": True},
            {},
        ):
            text = _dumps(payload)
            if len(text) <= max_chars:
                return text
        return "" if max_chars < 2 else "{}"


_TRUNCATED_NOTE = "Resultado recortado; no repitas la operación solo por el tamaño."


def _dumps(payload: dict[str, Any]) -> str:
    try:
        return json.dumps(payload, ensure_ascii=False, default=str)
    except (TypeError, ValueError):
        return json.dumps(
            {"ok": payload.get("ok"), "data": str(payload.get("data")), "truncated": True},
            ensure_ascii=False,
        )


def _clip_text(text: str, limit: int) -> str:
    if len(text) <= limit:
        return text
    return text[: max(0, limit - 1)] + "…"


def _clip_strings(value: Any, limit: int) -> Any:
    if isinstance(value, str):
        return value if len(value) <= limit else value[:limit] + "…"
    if isinstance(value, dict):
        return {key: _clip_strings(item, limit) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [_clip_strings(item, limit) for item in value]
    return value


def _max_string_len(value: Any) -> int:
    if isinstance(value, str):
        return len(value)
    if isinstance(value, dict):
        return max((_max_string_len(item) for item in value.values()), default=0)
    if isinstance(value, (list, tuple)):
        return max((_max_string_len(item) for item in value), default=0)
    return 0

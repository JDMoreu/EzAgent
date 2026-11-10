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

    def to_dict(self) -> dict[str, Any]:
        return {"ok": self.ok, "data": self.data, "error": self.error}


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


def _validate_value(name: str, value: Any, spec: dict[str, Any]) -> str | None:
    expected = spec.get("type")
    if expected is not None and not _matches_type(value, expected):
        return f"El parámetro '{name}' debe ser de tipo {_type_label(expected)}."
    enum = spec.get("enum")
    if isinstance(enum, list) and value not in enum:
        options = ", ".join(repr(option) for option in enum)
        return f"El parámetro '{name}' debe ser uno de: {options}."
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        minimum = spec.get("minimum")
        maximum = spec.get("maximum")
        if isinstance(minimum, (int, float)) and not isinstance(minimum, bool) and value < minimum:
            return f"El parámetro '{name}' debe ser mayor o igual que {minimum}."
        if isinstance(maximum, (int, float)) and not isinstance(maximum, bool) and value > maximum:
            return f"El parámetro '{name}' debe ser menor o igual que {maximum}."
    return None


def _validate_args(schema: dict[str, Any], args: dict[str, Any]) -> str | None:
    properties = schema.get("properties")
    if not isinstance(properties, dict):
        properties = {}
    required = schema.get("required")
    if isinstance(required, list):
        for name in required:
            if isinstance(name, str) and name not in args:
                return f"Falta el parámetro obligatorio: '{name}'."
    for name, value in args.items():
        spec = properties.get(name)
        if isinstance(spec, dict):
            error = _validate_value(name, value, spec)
            if error is not None:
                return error
    return None


def _call_with_timeout(
    handler: Callable[[dict[str, Any]], ToolResult | dict[str, Any]],
    args: dict[str, Any],
    timeout_s: float,
) -> tuple[ToolResult | dict[str, Any] | None, str | None]:
    """Ejecuta el manejador en un hilo daemon y espera como máximo timeout_s."""
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
        return None, f"La herramienta superó el tiempo límite de {timeout:g} s."
    error = outcome.get("error")
    if error is not None:
        return None, f"Error al ejecutar la herramienta: {type(error).__name__}: {error}"
    return outcome.get("value"), None


def _normalize_result(value: ToolResult | dict[str, Any] | None) -> ToolResult:
    if isinstance(value, ToolResult):
        return value
    if isinstance(value, dict):
        if {"ok", "data", "error"} & set(value):
            ok = value.get("ok", "error" not in value)
            error = value.get("error")
            return ToolResult(
                ok=bool(ok),
                data=value.get("data"),
                error=None if error is None else str(error),
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
            return ToolResult(ok=False, error=f"Herramienta desconocida: {name}.")
        if not isinstance(args, dict):
            return ToolResult(
                ok=False, error="Los argumentos de la herramienta deben ser un objeto JSON."
            )
        validation_error = _validate_args(tool.parameters, args)
        if validation_error is not None:
            return ToolResult(ok=False, error=validation_error)
        value, failure = _call_with_timeout(tool.handler, args, tool.timeout_s)
        if failure is not None:
            return ToolResult(ok=False, error=failure)
        return _normalize_result(value)

    @staticmethod
    def render_for_llm(result: ToolResult, max_chars: int = 2000) -> str:
        if max_chars <= 0:
            return ""
        try:
            text = json.dumps(result.to_dict(), ensure_ascii=False, default=str)
        except (TypeError, ValueError):
            text = json.dumps(
                {
                    "ok": result.ok,
                    "data": str(result.data),
                    "error": result.error,
                },
                ensure_ascii=False,
            )
        if len(text) <= max_chars:
            return text
        marker = "…(truncado)"
        if max_chars <= len(marker):
            return text[:max_chars]
        return text[: max_chars - len(marker)] + marker

"""Fixtures del registro de herramientas (dominio `registry`, F1-04).

Fija el JSON exacto de `ToolSpec.schema()`, los mensajes de validación y los
resultados de `ToolRegistry.run` / `render_for_llm`.
"""

from __future__ import annotations

import time
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "registry"

# Descripciones y parámetros reales del proyecto, con tildes y rutas falsas.
SPECS: list[dict[str, Any]] = [
    {
        "id": "simple",
        "name": "get_time",
        "description": "Devuelve la hora actual.",
        "parameters": {"type": "object", "properties": {}},
        "level": 0,
    },
    {
        "id": "required",
        "name": "read_file",
        "description": "Lee un archivo de texto.",
        "parameters": {
            "type": "object",
            "properties": {"path": {"type": "string"}, "max_bytes": {"type": "integer"}},
            "required": ["path"],
        },
        "level": 1,
    },
    {
        "id": "nested",
        "name": "delegate_to_agent",
        "description": "Delega una tarea en un agente de Herdr.",
        "parameters": {
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["run", "list", "read"]},
                "then": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {"agent": {"type": "string"}},
                        "required": ["agent"],
                    },
                },
                "gui": {"type": "boolean"},
            },
            "required": ["action"],
            "additionalProperties": False,
        },
        "level": 2,
    },
]

VALIDATE_CASES: list[dict[str, Any]] = [
    {"id": "unknown-tool", "tool": "nope", "args": {}},
    {"id": "args-not-object", "tool": "t", "args": [1, 2]},
    {"id": "ok", "args": {"x": "hola"}},
    {"id": "missing-required", "args": {}},
    {"id": "required-none", "args": {"x": None}},
    {"id": "extra-tolerated", "args": {"x": "a", "extra": 1}},
    {"id": "extra-rejected", "args": {"x": "a", "extra": 1}, "strict": True},
    {"id": "string-bad", "args": {"x": 1}},
    {"id": "int-float-integral", "args": {"x": 2.0}},
    {"id": "int-float-broken", "args": {"x": 2.5}},
    {"id": "int-bool", "args": {"x": True}},
    {"id": "union-null", "args": {"x": None}, "union": True},
    {"id": "union-text", "args": {"x": "a"}, "union": True},
    {"id": "union-bad", "args": {"x": 1}, "union": True},
    {"id": "enum-bad", "args": {"x": "c"}},
    {"id": "enum-number", "args": {"x": 1}, "enum_numbers": True},
    {"id": "min-length", "args": {"x": "ab"}},
    {"id": "max-length", "args": {"x": "abcd"}},
    {"id": "minimum", "args": {"x": 0}},
    {"id": "maximum", "args": {"x": 11}},
    {"id": "bool-is-not-number", "args": {"x": True}, "no_type": True},
    {"id": "items-bad", "args": {"x": [1, "b"]}},
    {"id": "min-items", "args": {"x": []}},
    {"id": "max-items", "args": {"x": [1, 2, 3]}},
    {"id": "unknown-type", "args": {"x": "lo que sea"}, "weird_type": True},
    {"id": "object-nested-bad", "args": {"x": {"n": "texto"}}, "object_spec": True},
]

_EMPTY = {"type": "object", "properties": {}}
_INT_X = {"type": "object", "properties": {"x": {"type": "integer"}}}

RUN_CASES: list[dict[str, Any]] = [
    {"id": "echo", "handler": "echo", "args": {"x": 4}, "parameters": _INT_X},
    {"id": "returns-dict-contract", "handler": "contract", "args": {}, "parameters": _EMPTY},
    {"id": "returns-dict-plain", "handler": "plain", "args": {}, "parameters": _EMPTY},
    {"id": "returns-none", "handler": "none", "args": {}, "parameters": _EMPTY},
    {"id": "returns-text", "handler": "text", "args": {}, "parameters": _EMPTY},
    {
        "id": "validation-first",
        "handler": "echo",
        "args": {"x": "no"},
        "parameters": {**_INT_X, "required": ["x"]},
    },
    {"id": "raised", "handler": "fail", "args": {}, "parameters": _EMPTY},
    {"id": "timeout", "handler": "slow", "args": {}, "parameters": _EMPTY, "timeout_s": 0.0},
    {"id": "unknown", "handler": "echo", "args": {}, "parameters": _EMPTY, "tool": "nope"},
    {"id": "args-not-object", "handler": "echo", "args": "texto", "parameters": _INT_X},
]

RENDER_CASES: list[dict[str, Any]] = [
    {"id": "fits", "result": {"ok": True, "data": {"á": 1}}, "max_chars": 2000},
    {
        "id": "fits-error-kind",
        "result": {"ok": False, "data": None, "error": "falló", "outcome": "failed", "error_kind": "denied"},
        "max_chars": 2000,
    },
    {"id": "big-string-2000", "result": {"ok": True, "data": "x" * 5000}, "max_chars": 2000},
    {"id": "big-string-100", "result": {"ok": True, "data": "x" * 5000}, "max_chars": 100},
    {"id": "big-string-20", "result": {"ok": True, "data": "x" * 5000}, "max_chars": 20},
    {"id": "tiny-5", "result": {"ok": True, "data": "x" * 5000}, "max_chars": 5},
    {"id": "tiny-2", "result": {"ok": True, "data": "x" * 5000}, "max_chars": 2},
    {"id": "tiny-1", "result": {"ok": True, "data": "x" * 5000}, "max_chars": 1},
    {"id": "zero", "result": {"ok": True, "data": "x"}, "max_chars": 0},
    {"id": "negative", "result": {"ok": True, "data": "x"}, "max_chars": -3},
    {
        "id": "nested-list",
        "result": {"ok": True, "data": {"lista": ["y" * 400, "z" * 400], "n": 1}},
        "max_chars": 300,
    },
    {
        "id": "long-error",
        "result": {"ok": False, "error": "e" * 3000, "outcome": "unknown_outcome"},
        "max_chars": 120,
    },
    {"id": "odd-repr", "result": {"ok": True, "data": {"__repr__": "<raro>"}}, "max_chars": 200},
    {"id": "none-data", "result": {"ok": True, "data": None}, "max_chars": 2},
]

SCHEMA_CASES = ["simple", "required", "nested"]

DESCRIBE_CASES: list[dict[str, Any]] = [
    {"id": "default-level", "level": 1},
    {"id": "custom", "level": 2, "describe": {"bulk": 7, "always_confirm": True, "summary": "borra 7"}},
    {"id": "wrong-type", "level": 1, "describe": {"wrong": True}},
]


def _parameters_for(case: dict[str, Any]) -> dict[str, Any]:
    if case.get("union"):
        return {"type": "object", "properties": {"x": {"type": ["string", "null"]}}}
    if case.get("enum_numbers"):
        return {"type": "object", "properties": {"x": {"type": "integer", "enum": [1, 2]}}}
    if case.get("weird_type"):
        return {"type": "object", "properties": {"x": {"type": "weird"}}}
    if case.get("object_spec"):
        return {
            "type": "object",
            "properties": {"x": {"type": "object", "properties": {"n": {"type": "integer"}}}},
        }
    spec: dict[str, Any] = {"type": "object", "properties": {}}
    ident = case["id"]
    if ident in ("enum-bad",):
        spec["properties"] = {"x": {"type": "string", "enum": ["a", "b"]}}
    elif ident.startswith("min-length"):
        spec["properties"] = {"x": {"type": "string", "minLength": 3}}
    elif ident.startswith("max-length"):
        spec["properties"] = {"x": {"type": "string", "maxLength": 3}}
    elif ident.startswith("minimum"):
        spec["properties"] = {"x": {"type": "integer", "minimum": 1}}
    elif ident.startswith("maximum"):
        spec["properties"] = {"x": {"type": "integer", "maximum": 10}}
    elif case.get("no_type"):
        spec["properties"] = {"x": {"minimum": 5}}
    elif ident.startswith("items-bad"):
        spec["properties"] = {"x": {"type": "array", "items": {"type": "integer"}}}
    elif ident.startswith("min-items"):
        spec["properties"] = {"x": {"type": "array", "minItems": 1}}
    elif ident.startswith("max-items"):
        spec["properties"] = {"x": {"type": "array", "maxItems": 2}}
    elif ident == "int-float-integral" or ident == "int-float-broken" or ident == "int-bool":
        spec["properties"] = {"x": {"type": "integer"}}
    else:
        spec["properties"] = {"x": {"type": "string"}}
    if case.get("strict"):
        spec["additionalProperties"] = False
    if ident in ("missing-required", "required-none", "extra-tolerated", "extra-rejected"):
        spec["properties"] = {"x": {"type": "string"}}
        spec["required"] = ["x"]
    return spec


def _handler_factory() -> dict[str, Any]:
    def slow(_args: dict[str, Any]) -> str:
        time.sleep(0.2)
        return "tarde"

    def fail(_args: dict[str, Any]) -> Any:
        raise ValueError("kaboom")

    return {
        "echo": lambda args: {"x": args.get("x")},
        "contract": lambda _args: {"ok": False, "error": "mal"},
        "plain": lambda _args: {"cualquier": "cosa"},
        "none": lambda _args: None,
        "text": lambda _args: "texto",
        "fail": fail,
        "slow": slow,
    }


def _make_spec(ToolSpec: Any, raw: dict[str, Any], handler: Any) -> Any:
    return ToolSpec(
        name=raw["name"],
        description=raw["description"],
        parameters=raw["parameters"],
        level=raw.get("level", 0),
        handler=handler,
        timeout_s=raw.get("timeout_s", 10.0),
    )


def _make_result(ToolResult: Any, raw: dict[str, Any]) -> Any:
    data = raw.get("data")
    if isinstance(data, dict) and set(data) == {"__repr__"}:
        text = data["__repr__"]

        class _Odd:
            def __repr__(self) -> str:
                return text

        data = _Odd()
    return ToolResult(
        ok=raw["ok"],
        data=data,
        error=raw.get("error"),
        outcome=raw.get("outcome", "completed"),
        error_kind=raw.get("error_kind"),
    )


def _schema_cases(out: FixtureFile, ToolSpec: Any, handlers: dict[str, Any]) -> None:
    for raw in SPECS:
        spec = _make_spec(ToolSpec, raw, handlers["echo"])
        out.add(f"registry/schema/{raw['id']}", "schema", {"spec": raw}, {"schema": spec.schema()})


def _validate_cases(
    out: FixtureFile, ToolRegistry: Any, ToolSpec: Any, handlers: dict[str, Any]
) -> None:
    for case in VALIDATE_CASES:
        parameters = _parameters_for(case)
        tool = case.get("tool", "t")
        registry = ToolRegistry()
        if tool != "nope":
            spec = _make_spec(
                ToolSpec, {"name": "t", "description": "t", "parameters": parameters}, handlers["echo"]
            )
            registry.register(spec)
        payload = {"tool": tool, "parameters": parameters, "args": case["args"]}
        out.add(
            f"registry/validate/{case['id']}",
            "validate",
            payload,
            {"error": registry.validate(tool, case["args"])},
        )


def _run_cases(
    out: FixtureFile, ToolRegistry: Any, ToolSpec: Any, handlers: dict[str, Any]
) -> None:
    for case in RUN_CASES:
        parameters = case["parameters"]
        raw_spec = {
            "name": "t",
            "description": "t",
            "parameters": parameters,
            "timeout_s": case.get("timeout_s", 10.0),
        }
        registry = ToolRegistry()
        registry.register(_make_spec(ToolSpec, raw_spec, handlers[case["handler"]]))
        result = registry.run(case.get("tool", "t"), case["args"])
        payload = {
            "tool": case.get("tool", "t"),
            "parameters": parameters,
            "timeout_s": case.get("timeout_s", 10.0),
            "handler": case["handler"],
            "args": case["args"],
        }
        out.add(f"registry/run/{case['id']}", "run", payload, {"result": result.to_dict()})


def _render_cases(out: FixtureFile, ToolRegistry: Any, ToolResult: Any) -> None:
    for case in RENDER_CASES:
        result = _make_result(ToolResult, case["result"])
        payload = {"result": case["result"], "max_chars": case["max_chars"]}
        out.add(
            f"registry/render/{case['id']}",
            "render_for_llm",
            payload,
            {"text": ToolRegistry.render_for_llm(result, max_chars=case["max_chars"])},
        )

    boundary = ToolResult(ok=True, data={"texto": "x" * 40, "n": 3})
    full = ToolRegistry.render_for_llm(boundary, max_chars=10_000)
    for delta in (0, -1, -2):
        limit = len(full) + delta
        payload = {
            "result": {"ok": True, "data": {"texto": "x" * 40, "n": 3}},
            "max_chars": limit,
        }
        out.add(
            f"registry/render/boundary{delta:+d}",
            "render_for_llm",
            payload,
            {"text": ToolRegistry.render_for_llm(boundary, max_chars=limit)},
        )


def _schemas_cases(
    out: FixtureFile, ToolRegistry: Any, ToolSpec: Any, handlers: dict[str, Any]
) -> None:
    registry = ToolRegistry()
    empty = {"type": "object", "properties": {}}
    registry.register_many(
        [
            _make_spec(ToolSpec, {"name": name, "description": f"tool {name}", "parameters": empty}, handlers["echo"])
            for name in "abcde"
        ]
    )
    schema_args: list[dict[str, Any]] = [
        {"id": "all-default", "kwargs": {}},
        {"id": "names-filter", "kwargs": {"names": ["c", "zzz", "a"]}},
        {"id": "limit-2", "kwargs": {"limit": 2}},
        {"id": "names-limit", "kwargs": {"names": ["e", "d", "c"], "limit": 2}},
        {"id": "limit-0", "kwargs": {"limit": 0}},
        {"id": "limit-negative", "kwargs": {"limit": -1}},
        {"id": "names-empty", "kwargs": {"names": []}},
    ]
    for case in schema_args:
        schemas = registry.schemas(**case["kwargs"])
        out.add(
            f"registry/schemas/{case['id']}",
            "schemas",
            {"tools": list("abcde"), "kwargs": case["kwargs"]},
            {"schemas": schemas, "names": [item["function"]["name"] for item in schemas]},
        )


def _describe_cases(
    out: FixtureFile, ToolInfo: Any, ToolSpec: Any, handlers: dict[str, Any]
) -> None:
    for case in DESCRIBE_CASES:
        describe_fn = None
        if case["id"] == "custom":
            describe_fn = lambda _args, values=case["describe"]: ToolInfo(level=2, **values)  # noqa: E731
        elif case["id"] == "wrong-type":
            describe_fn = lambda _args: {"level": 1}  # noqa: E731
        spec = ToolSpec(
            name="t",
            description="t",
            parameters={"type": "object", "properties": {}},
            level=case["level"],
            handler=handlers["echo"],
            describe_fn=describe_fn,
        )
        try:
            info = spec.describe({"p": "hola"})
        except TypeError as error:
            out.add_error(f"registry/describe/{case['id']}", "describe", case, str(error))
        else:
            tool_info = {
                "level": info.level,
                "bulk": info.bulk,
                "always_confirm": info.always_confirm,
                "summary": info.summary,
            }
            out.add(f"registry/describe/{case['id']}", "describe", case, {"tool_info": tool_info})


def generate(out: FixtureFile) -> None:
    from ezagent.agent.registry import ToolInfo, ToolRegistry, ToolResult, ToolSpec

    handlers = _handler_factory()
    _schema_cases(out, ToolSpec, handlers)
    _validate_cases(out, ToolRegistry, ToolSpec, handlers)
    _run_cases(out, ToolRegistry, ToolSpec, handlers)
    _render_cases(out, ToolRegistry, ToolResult)
    _schemas_cases(out, ToolRegistry, ToolSpec, handlers)
    _describe_cases(out, ToolInfo, ToolSpec, handlers)

    try:
        ToolRegistry().register({"name": "x"})  # type: ignore[arg-type]
    except TypeError as error:
        out.add_error("registry/register/not-a-spec", "register", {"tool": {"name": "x"}}, str(error))
    else:  # pragma: no cover
        raise AssertionError("register aceptó un dict")

    out.add("registry/truncated-note", "truncated_note", {}, {"note": _truncated_note()})


def _truncated_note() -> str:
    from ezagent.agent.registry import _TRUNCATED_NOTE

    return _TRUNCATED_NOTE


if __name__ == "__main__":
    main_guard(DOMAIN, generate)

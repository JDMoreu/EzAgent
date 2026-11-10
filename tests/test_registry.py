"""ToolRegistry: validación de esquema, timeout y normalización de resultados."""

from __future__ import annotations

import json
import threading

import pytest

from ezagent.agent.registry import ToolInfo, ToolRegistry, ToolResult, ToolSpec


def spec(
    name: str = "t",
    *,
    handler=lambda args: ToolResult(ok=True, data=args),
    parameters: dict | None = None,
    level: int = 0,
    timeout_s: float = 2.0,
    **kwargs,
) -> ToolSpec:
    return ToolSpec(
        name=name,
        description=f"tool {name}",
        parameters=parameters or {"type": "object", "properties": {}},
        level=level,
        handler=handler,
        timeout_s=timeout_s,
        **kwargs,
    )


def reg_with(parameters: dict) -> ToolRegistry:
    registry = ToolRegistry()
    registry.register(spec("t", parameters=parameters))
    return registry


# ------------------------------------------------------------------ tipos
@pytest.mark.parametrize(
    ("json_type", "good", "bad"),
    [
        ("string", ["", "x"], [1, None, [], {}, True]),
        ("integer", [0, 3, -2, 2.0], [2.5, "3", None, True, False]),
        ("number", [0, 1.5, -3], ["1", None, True]),
        ("boolean", [True, False], [0, 1, "true", None]),
        ("array", [[], [1]], ["a", {}, None]),
        ("object", [{}, {"a": 1}], [[], "a", None]),
        ("null", [None], [0, "", False]),
    ],
)
def test_validacion_de_tipos(json_type: str, good: list, bad: list) -> None:
    registry = reg_with({"type": "object", "properties": {"x": {"type": json_type}}})
    for value in good:
        assert registry.validate("t", {"x": value}) is None, (json_type, value)
    for value in bad:
        error = registry.validate("t", {"x": value})
        assert error and "'x'" in error, (json_type, value)


def test_tipo_union() -> None:
    registry = reg_with({"type": "object", "properties": {"x": {"type": ["string", "null"]}}})
    assert registry.validate("t", {"x": None}) is None
    assert registry.validate("t", {"x": "a"}) is None
    assert "texto o nulo" in registry.validate("t", {"x": 1})


def test_un_tipo_desconocido_no_se_valida() -> None:
    registry = reg_with({"type": "object", "properties": {"x": {"type": "weird"}}})
    assert registry.validate("t", {"x": object()}) is None


def test_enum_minimo_y_maximo() -> None:
    registry = reg_with(
        {
            "type": "object",
            "properties": {
                "mode": {"type": "string", "enum": ["a", "b"]},
                "n": {"type": "integer", "minimum": 1, "maximum": 10},
            },
        }
    )
    assert registry.validate("t", {"mode": "a", "n": 1}) is None
    assert registry.validate("t", {"mode": "b", "n": 10}) is None
    assert "uno de" in registry.validate("t", {"mode": "c"})
    assert "mayor o igual que 1" in registry.validate("t", {"n": 0})
    assert "menor o igual que 10" in registry.validate("t", {"n": 11})


def test_booleano_no_cuenta_como_numero_para_los_limites() -> None:
    registry = reg_with({"type": "object", "properties": {"n": {"minimum": 5}}})
    assert registry.validate("t", {"n": True}) is None  # sin tipo declarado: no es número


def test_parametros_obligatorios() -> None:
    registry = reg_with(
        {"type": "object", "properties": {"a": {}, "b": {}}, "required": ["a", "b"]}
    )
    assert registry.validate("t", {"a": 1, "b": 2}) is None
    assert "'a'" in registry.validate("t", {"b": 2})
    assert "'b'" in registry.validate("t", {"a": 1})
    assert "obligatorio" in registry.validate("t", {})


def test_un_obligatorio_con_valor_none_cuenta_como_presente() -> None:
    registry = reg_with({"type": "object", "properties": {"a": {}}, "required": ["a"]})
    assert registry.validate("t", {"a": None}) is None


def test_parametros_desconocidos_se_toleran() -> None:
    registry = reg_with({"type": "object", "properties": {"a": {"type": "string"}}})
    assert registry.validate("t", {"a": "x", "extra": 1}) is None


def test_validate_rechaza_herramienta_desconocida_y_args_que_no_son_dict() -> None:
    registry = reg_with({"type": "object", "properties": {}})
    assert "desconocida" in registry.validate("nope", {})
    assert "objeto JSON" in registry.validate("t", [1, 2])  # type: ignore[arg-type]


# --------------------------------------------------------------- ejecución
def test_run_ejecuta_y_devuelve_el_resultado() -> None:
    registry = ToolRegistry()
    registry.register(
        spec(
            "t",
            handler=lambda a: ToolResult(ok=True, data=a["x"] * 2),
            parameters={"type": "object", "properties": {"x": {"type": "integer"}}},
        )
    )
    assert registry.run("t", {"x": 4}) == ToolResult(ok=True, data=8)


def test_run_no_ejecuta_el_handler_si_la_validacion_falla() -> None:
    called: list[dict] = []
    registry = ToolRegistry()
    registry.register(
        spec(
            "t",
            handler=lambda a: called.append(a) or ToolResult(ok=True),
            parameters={
                "type": "object",
                "properties": {"x": {"type": "integer"}},
                "required": ["x"],
            },
        )
    )
    result = registry.run("t", {"x": "no"})
    assert not result.ok and called == []


def test_run_desconocida() -> None:
    result = ToolRegistry().run("nope", {})
    assert result.ok is False and "desconocida" in result.error


def test_run_convierte_excepciones_en_error() -> None:
    def boom(args):
        raise ValueError("kaboom")

    registry = ToolRegistry()
    registry.register(spec("t", handler=boom))
    result = registry.run("t", {})
    assert result.ok is False
    assert "ValueError" in result.error and "kaboom" in result.error


def test_run_aplica_timeout_sin_esperar_al_handler() -> None:
    release = threading.Event()
    registry = ToolRegistry()
    registry.register(spec("t", handler=lambda a: release.wait(10), timeout_s=0.05))
    try:
        result = registry.run("t", {})
    finally:
        release.set()
    assert result.ok is False and "tiempo límite" in result.error


@pytest.mark.parametrize(
    ("returned", "expected"),
    [
        (ToolResult(ok=True, data=1), ToolResult(ok=True, data=1)),
        ({"ok": False, "error": "mal"}, ToolResult(ok=False, error="mal")),
        ({"ok": True, "data": [1]}, ToolResult(ok=True, data=[1])),
        ({"error": "solo error"}, ToolResult(ok=False, error="solo error")),
        ({"data": 5}, ToolResult(ok=True, data=5)),
        ({"cualquier": "cosa"}, ToolResult(ok=True, data={"cualquier": "cosa"})),
        (None, ToolResult(ok=True, data=None)),
        ("texto", ToolResult(ok=True, data="texto")),
        (42, ToolResult(ok=True, data=42)),
    ],
)
def test_normalizacion_de_resultados(returned, expected) -> None:
    registry = ToolRegistry()
    registry.register(spec("t", handler=lambda a: returned))
    assert registry.run("t", {}) == expected


# ---------------------------------------------------------------- registro
def test_register_solo_acepta_toolspec() -> None:
    with pytest.raises(TypeError):
        ToolRegistry().register({"name": "x"})  # type: ignore[arg-type]


def test_un_duplicado_reemplaza_al_anterior() -> None:
    registry = ToolRegistry()
    registry.register(spec("t", handler=lambda a: ToolResult(ok=True, data="viejo")))
    registry.register(spec("t", handler=lambda a: ToolResult(ok=True, data="nuevo")))
    assert registry.names() == ["t"]
    assert registry.run("t", {}).data == "nuevo"


def test_schemas_respeta_orden_limite_y_omite_desconocidas() -> None:
    registry = ToolRegistry()
    registry.register_many([spec(n) for n in "abcde"])
    names = lambda items: [s["function"]["name"] for s in items]  # noqa: E731
    assert names(registry.schemas()) == list("abcde")
    assert names(registry.schemas(names=["c", "zzz", "a"])) == ["c", "a"]
    assert names(registry.schemas(limit=2)) == ["a", "b"]
    assert names(registry.schemas(names=["e", "d", "c"], limit=2)) == ["e", "d"]
    assert registry.schemas(limit=0) == []


def test_el_esquema_tiene_el_formato_de_openai() -> None:
    schema = spec(
        "t", parameters={"type": "object", "properties": {"x": {"type": "string"}}}
    ).schema()
    assert schema == {
        "type": "function",
        "function": {
            "name": "t",
            "description": "tool t",
            "parameters": {"type": "object", "properties": {"x": {"type": "string"}}},
        },
    }


def test_describe_usa_la_funcion_o_el_nivel_por_defecto() -> None:
    assert spec("t", level=1).describe({}) == ToolInfo(level=1)
    custom = spec("t", describe_fn=lambda a: ToolInfo(level=2, summary=a["p"]))
    assert custom.describe({"p": "hola"}) == ToolInfo(level=2, summary="hola")
    with pytest.raises(TypeError):
        spec("t", describe_fn=lambda a: {"level": 1}).describe({})


def test_registro_seguro_ante_hilos() -> None:
    registry = ToolRegistry()

    def work(i: int) -> None:
        for j in range(50):
            registry.register(spec(f"t{i}-{j}"))
            registry.names()

    threads = [threading.Thread(target=work, args=(i,)) for i in range(8)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(10)
    assert len(registry.names()) == 8 * 50


# ---------------------------------------------------------------- render
def test_render_for_llm_es_json_valido_si_cabe() -> None:
    out = ToolRegistry.render_for_llm(ToolResult(ok=True, data={"á": 1}))
    assert json.loads(out) == {
        "ok": True,
        "data": {"á": 1},
        "error": None,
        "outcome": "completed",
    }
    assert "á" in out  # sin escapes \u


def test_render_for_llm_trunca_sin_pasarse_del_limite() -> None:
    big = ToolResult(ok=True, data="x" * 5000)
    for limit in (20, 100, 2000):
        out = ToolRegistry.render_for_llm(big, max_chars=limit)
        assert len(out) <= limit
        payload = json.loads(out)  # F19: el recorte nunca rompe el JSON
        assert payload["truncated"] is True
        if limit >= 100:
            assert payload["original_chars"] > limit
    assert ToolRegistry.render_for_llm(big, max_chars=5) == "{}"
    assert ToolRegistry.render_for_llm(big, max_chars=0) == ""


def test_render_for_llm_respeta_el_limite_exacto() -> None:
    result = ToolResult(ok=True, data="x" * 40)
    full = ToolRegistry.render_for_llm(result, max_chars=10_000)
    exact = ToolRegistry.render_for_llm(result, max_chars=len(full))
    assert exact == full and "truncated" not in exact  # cabe justo: no se recorta
    shorter = ToolRegistry.render_for_llm(result, max_chars=len(full) - 1)
    assert len(shorter) <= len(full) - 1 and json.loads(shorter)["truncated"] is True


def test_render_for_llm_tolera_datos_no_serializables() -> None:
    class Raro:
        def __repr__(self) -> str:
            return "<raro>"

    out = ToolRegistry.render_for_llm(ToolResult(ok=True, data=Raro()))
    assert json.loads(out)["data"] == "<raro>"

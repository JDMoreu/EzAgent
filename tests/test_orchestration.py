"""Orquestación: pasos encadenados y guías de agentes."""

from __future__ import annotations

import pytest

from ezagent.agent.orchestration import (
    MAX_STEPS,
    ORCHESTRATION_HINT,
    chain_spec,
    describe_steps,
    load_guide,
    parse_steps,
)


def test_parse_steps_none_y_lista_vacia() -> None:
    assert parse_steps(None) == []
    assert parse_steps([]) == []


def test_parse_steps_normaliza_y_aplica_defaults() -> None:
    steps = parse_steps(
        [
            {"agent": " opencode ", "task": " implementa ", "model": " m ", "effort": "low"},
            {"agent": "codex"},
        ]
    )
    assert steps == [
        {"agent": "opencode", "task": "implementa", "model": "m", "effort": "low"},
        {"agent": "codex", "task": "{output}"},
    ]


def test_parse_steps_ignora_opcionales_vacios_o_que_no_son_texto() -> None:
    (step,) = parse_steps(
        [{"agent": "a", "model": "", "effort": 5, "cwd": "  ", "name": ["x"], "extra": "z"}]
    )
    assert step == {"agent": "a", "task": "{output}"}


@pytest.mark.parametrize(
    ("raw", "message"),
    [
        ("texto", "lista de pasos"),
        ({"agent": "a"}, "lista de pasos"),
        (["a"], "debe ser un objeto"),
        ([{"task": "x"}], "necesita «agent»"),
        ([{"agent": "  "}], "necesita «agent»"),
        ([{"agent": "ok"}, {"task": "sin agente"}], "paso 2"),
        ([{"agent": "a"}] * (MAX_STEPS + 1), "como máximo"),
    ],
)
def test_parse_steps_rechaza_entradas_invalidas(raw, message: str) -> None:
    with pytest.raises(ValueError, match=message):
        parse_steps(raw)


def test_parse_steps_acepta_exactamente_el_maximo() -> None:
    assert len(parse_steps([{"agent": "a"}] * MAX_STEPS)) == MAX_STEPS


def test_chain_spec_un_solo_paso_avisa_en_ese_paso() -> None:
    steps = parse_steps([{"agent": "claude", "task": "plan", "model": "opus"}])
    spec = chain_spec(
        "t1:1", steps, cwd="C:/x", space="w", notify=True, deliver=True, chain_id="c1"
    )
    assert spec["source"] == "t1:1" and spec["agent"] == "claude" and spec["task"] == "plan"
    assert spec["cwd"] == "C:/x" and spec["model"] == "opus" and spec["effort"] == ""
    assert spec["notify_on_done"] is True and spec["notify_final"] is True
    assert spec["deliver_answer"] is True and spec["deliver_final"] is True
    assert spec["chain_id"] == "c1" and "then" not in spec
    assert spec["background"] is False and spec["gui"] is False


def test_chain_spec_solo_el_ultimo_paso_avisa_y_entrega() -> None:
    steps = parse_steps([{"agent": "a"}, {"agent": "b"}, {"agent": "c"}])
    spec = chain_spec("s", steps, cwd=None, space=None, notify=True, deliver=True)
    assert spec["notify_on_done"] is False and spec["notify_final"] is True
    assert spec["deliver_answer"] is False and spec["deliver_final"] is True
    assert [s["agent"] for s in spec["then"]] == ["b", "c"]
    assert "chain_id" not in spec


def test_chain_spec_sin_aviso_no_avisa_nunca() -> None:
    spec = chain_spec(
        "s", parse_steps([{"agent": "a"}, {"agent": "b"}]), cwd=None, space=None, notify=False
    )
    assert spec["notify_on_done"] is False and spec["notify_final"] is False
    assert spec["deliver_final"] is False


def test_chain_spec_el_cwd_del_paso_manda_sobre_el_general() -> None:
    steps = parse_steps([{"agent": "a", "cwd": "C:/paso"}])
    assert chain_spec("s", steps, cwd="C:/general", space=None, notify=False)["cwd"] == "C:/paso"


def test_describe_steps() -> None:
    steps = parse_steps(
        [
            {"agent": "claude", "model": "opus", "effort": "high"},
            {"agent": "codex"},
            {"agent": "pi", "effort": "low"},
        ]
    )
    assert describe_steps(steps) == "claude (opus, high), codex, pi (low)"
    assert describe_steps([]) == ""


@pytest.mark.parametrize("key", ["claude", "codex", "mcode", "opencode", "pi", "qwen"])
def test_las_guias_de_agentes_existen_y_no_estan_vacias(key: str) -> None:
    guide = load_guide(key)
    assert guide and len(guide) > 100


@pytest.mark.parametrize(
    "key",
    [
        "",
        None,
        "../config",
        "..\\config",
        "claude/../codex",
        "CLAUDE",
        "a b",
        "claude.guide",
        "no-existe",
    ],
)
def test_load_guide_rechaza_claves_peligrosas_o_inexistentes(key) -> None:
    assert load_guide(key) is None


def test_el_hint_menciona_las_herramientas_y_la_regla_de_datos_no_confiables() -> None:
    for fragment in ("delegate_to_agent", "action=run", "then", "{output}", "dato no confiable"):
        assert fragment in ORCHESTRATION_HINT

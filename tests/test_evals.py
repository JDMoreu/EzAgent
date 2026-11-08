"""Set dorado: esquema de casos, puntuación y proveedor falso."""

from __future__ import annotations

from collections import Counter

import pytest

from evals.intents import build_cases, runner

TOOLS = {t["function"]["name"] for t in runner.load_tools()}


def test_cases_validos_y_composicion() -> None:
    cases = runner.load_cases()
    assert len(cases) >= 120
    assert len({c["id"] for c in cases}) == len(cases)
    for c in cases:
        assert c["lang"] in {"es", "en"} and c["route"] in {"command", "chat", "action"}
        tool = c["expect"]["tool"]
        assert tool is None or tool in TOOLS
        assert (c["route"] == "chat") == (tool is None)
    langs = Counter(c["lang"] for c in cases)
    assert langs["es"] >= 80 and langs["en"] >= 30
    assert {c["expect"]["tool"] for c in cases} == TOOLS | {None}


def test_cases_jsonl_esta_sincronizado_con_el_generador() -> None:
    assert runner.load_cases() == build_cases.build()


def test_oracle_puntua_100() -> None:
    report = runner.run(runner.load_cases(), runner.oracle, {"model": "fake"})
    assert report.pct(report.scores) == 100.0
    assert "Fallos (0)" in report.to_markdown()


def test_alias_de_whisper_se_resuelven() -> None:
    case = {"id": "x-es-001", "lang": "es", "text": "t", "route": "action",
            "expect": {"tool": "run_agent", "args": {"agent": "claude", "project": "ezagent"},
                       "contains": {"task": ["test"]}}}
    call = runner.Call("run_agent", {"agent": "Cloud Code", "project": "easy agent",
                                     "task": "Arregla los TESTS"}, 10.0)
    assert runner.score_case(case, call).ok


@pytest.mark.parametrize("call,fragment", [
    (runner.Call("agent_status", {}, 1.0), "herramienta esperada"),
    (runner.Call("run_agent", {"agent": "codex", "project": "ezagent", "task": "test"}, 1.0), "agent"),
    (runner.Call("run_agent", {"agent": "claude", "project": "ezagent", "task": "otra cosa"}, 1.0), "task"),
    (runner.Call(None, {}, 0.0, error="TimeoutError"), "TimeoutError"),
])
def test_fallos_se_explican(call: runner.Call, fragment: str) -> None:
    case = {"id": "run-es-001", "lang": "es", "text": "t", "route": "action",
            "expect": {"tool": "run_agent", "args": {"agent": "claude"}, "contains": {"task": ["test"]}}}
    score = runner.score_case(case, call)
    assert not score.ok and fragment in score.reason


def test_charla_exige_ninguna_herramienta() -> None:
    case = {"id": "chat-es-001", "lang": "es", "text": "gracias", "route": "chat", "expect": {"tool": None}}
    assert runner.score_case(case, runner.Call(None, {}, 5.0, text="De nada.")).ok
    assert not runner.score_case(case, runner.Call("remember", {}, 5.0)).ok


def test_prompt_cabe_en_el_presupuesto() -> None:
    system = runner.build_system("es")
    assert len(system) / 4 <= 1200  # ≈ tokens, presupuesto de docs/roadmap/04
    assert "Known agents" in system and "{" not in system


def test_esquemas_caben_en_el_presupuesto() -> None:
    import json
    compact = json.dumps(runner.load_tools(), separators=(",", ":"))
    assert len(compact) <= 2800 + 400  # margen: el doc 04 cuenta sin envoltorio "type/function"

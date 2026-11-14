"""Fixtures de orquestación multiagente (dominio `orchestration`, F1-04).

Fija `parse_steps`, `chain_spec`, `describe_steps`, `flow_task` y los manuales
de agente (`load_guide`) con el texto exacto que recibe el LLM.
"""

from __future__ import annotations

from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "orchestration"

PARSE_OK: list[Any] = [
    None,
    [],
    [{"agent": " opencode ", "task": " implementa ", "model": " m ", "effort": "low"}],
    [{"agent": "codex"}],
    [{"agent": "a", "model": "", "effort": 5, "cwd": "  ", "name": ["x"], "extra": "z"}],
    [{"agent": "a"}] * 5,
    [{"agent": "claude", "task": 0, "model": None, "name": ""}],
    [{"agent": 5, "task": "literal"}],
    [{"agent": "claude", "task": "plan", "model": "opus", "effort": "high", "cwd": "C:/w"}],
]

PARSE_BAD: list[Any] = [
    "texto",
    {"agent": "a"},
    ["a"],
    [{"task": "x"}],
    [{"agent": "  "}],
    [{"agent": "ok"}, {"task": "sin agente"}],
    [{"agent": "a"}] * 6,
    [None],
]

CHAIN_CASES: list[dict[str, Any]] = [
    {
        "id": "single",
        "source": "t1:1",
        "then": [{"agent": "claude", "task": "plan", "model": "opus"}],
        "cwd": "C:/x",
        "space": "w",
        "notify": True,
        "deliver": True,
        "chain_id": "c1",
        "flow": ["claude"],
    },
    {
        "id": "chain",
        "source": "t2:3",
        "then": [{"agent": "a"}, {"agent": "b"}, {"agent": "c"}],
        "notify": True,
        "deliver": True,
    },
    {
        "id": "no-notify",
        "source": "s",
        "then": [{"agent": "a"}, {"agent": "b"}],
        "notify": False,
    },
    {
        "id": "step-cwd-wins",
        "source": "s",
        "then": [{"agent": "a", "cwd": "C:/paso"}],
        "cwd": "C:/general",
        "notify": False,
    },
    {
        "id": "flow-filters-blanks",
        "source": "s",
        "then": [{"agent": "a", "name": "  "}],
        "notify": True,
        "deliver": False,
        "flow": ["claude", "", "  ", 7],
    },
    {
        "id": "deliver-without-notify",
        "source": "s",
        "then": [{"agent": "a"}],
        "notify": False,
        "deliver": True,
        "flow": [],
    },
]

DESCRIBE_CASES: list[list[dict[str, str]]] = [
    [],
    [{"agent": "claude", "task": "{output}", "model": "opus", "effort": "high"}],
    [
        {"agent": "claude", "task": "x", "model": "opus", "effort": "high"},
        {"agent": "codex", "task": "{output}"},
        {"agent": "pi", "task": "{output}", "effort": "low"},
    ],
    [{"agent": "claude", "task": "{output}", "model": ""}],
]

FLOW_NAMES: list[list[str]] = [[], ["claude"], ["claude", "opencode", "codex"]]

GUIDES: list[Any] = [
    "claude",
    "codex",
    "mcode",
    "opencode",
    "pi",
    "qwen",
    "",
    None,
    "../config",
    "..\\config",
    "claude/../codex",
    "CLAUDE",
    "a b",
    "claude.guide",
    "no-existe",
]


def _parse_cases(out: FixtureFile, orchestration: Any) -> None:
    for index, raw in enumerate(PARSE_OK, start=1):
        out.add(
            f"orchestration/parse/{index:03d}",
            "parse_steps",
            {"then": raw},
            {"steps": orchestration.parse_steps(raw)},
        )
    for index, raw in enumerate(PARSE_BAD, start=1):
        try:
            orchestration.parse_steps(raw)
        except ValueError as error:
            out.add_error(
                f"orchestration/parse-bad/{index:03d}", "parse_steps", {"then": raw}, str(error)
            )
        else:  # pragma: no cover - el oráculo debe registrar el fallo
            raise AssertionError(f"parse_steps aceptó una entrada inválida: {raw!r}")


def _chain_cases(out: FixtureFile, orchestration: Any) -> None:
    for case in CHAIN_CASES:
        steps = orchestration.parse_steps(case["then"])
        kwargs: dict[str, Any] = {
            "cwd": case.get("cwd"),
            "space": case.get("space"),
            "notify": case.get("notify", False),
            "deliver": case.get("deliver", False),
        }
        if "chain_id" in case:
            kwargs["chain_id"] = case["chain_id"]
        if "flow" in case:
            kwargs["flow"] = case["flow"]
        payload = {
            "source": case["source"],
            "then": case["then"],
            "cwd": case.get("cwd"),
            "space": case.get("space"),
            "notify": case.get("notify", False),
            "deliver": case.get("deliver", False),
            "chain_id": case.get("chain_id", ""),
            "flow": case.get("flow"),
        }
        out.add(
            f"orchestration/chain/{case['id']}",
            "chain_spec",
            payload,
            {"spec": orchestration.chain_spec(case["source"], steps, **kwargs)},
        )


def _text_cases(out: FixtureFile, orchestration: Any) -> None:
    for index, steps in enumerate(DESCRIBE_CASES, start=1):
        out.add(
            f"orchestration/describe/{index:03d}",
            "describe_steps",
            {"steps": steps},
            {"text": orchestration.describe_steps(steps)},
        )
    for index, names in enumerate(FLOW_NAMES, start=1):
        out.add(
            f"orchestration/flow/{index:03d}",
            "flow_task",
            {"names": names},
            {"text": orchestration.flow_task(names)},
        )


def generate(out: FixtureFile) -> None:
    from ezagent.agent import orchestration

    _parse_cases(out, orchestration)
    _chain_cases(out, orchestration)
    _text_cases(out, orchestration)

    for index, key in enumerate(GUIDES, start=1):
        out.add(
            f"orchestration/guide/{index:03d}",
            "load_guide",
            {"agent_key": key},
            {"guide": orchestration.load_guide(key)},
        )

    out.add(
        "orchestration/constants",
        "module_constants",
        {},
        {
            "max_steps": orchestration.MAX_STEPS,
            "flow_prefix": orchestration.FLOW_PREFIX,
            "orchestration_hint": orchestration.ORCHESTRATION_HINT,
        },
    )


if __name__ == "__main__":
    main_guard(DOMAIN, generate)

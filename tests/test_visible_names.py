"""Los agentes se nombran como los ve el usuario en Herdr, nunca por su dirección interna."""

from __future__ import annotations

import re
from types import SimpleNamespace

import pytest

from ezagent.agent import tools_herdr as th
from ezagent.agent.registry import ToolResult
from ezagent.agent.system_prompt import EXECUTION_HINT
from ezagent.voice.herdr import visible_name

_ADDRESS = re.compile(r"\bt[0-9a-z]{2,3}\b")


@pytest.mark.parametrize(
    ("item", "expected"),
    [
        ({"where": "EzAgent / Claude Code"}, "Claude Code de EzAgent"),
        ({"where": "EzAgent / 2"}, "pestaña 2 de EzAgent"),
        ({"name": "planificador", "space": "EzAgent"}, "planificador de EzAgent"),
        ({"agent": "claude-code", "cwd": r"C:\Users\x\EzAgent"}, "Claude Code en EzAgent"),
        ({"address": "t51"}, "el agente"),
    ],
)
def test_visible_name_uses_what_herdr_shows(item: dict, expected: str) -> None:
    assert visible_name(item) == expected


@pytest.mark.parametrize(
    "args",
    [
        {"action": "kill", "address": "t5y"},
        {"action": "send", "address": "t51", "task": "Iguala el espaciado"},
        {"action": "read", "address": "t51", "lines": 20},
        {"action": "watch", "address": "t51"},
        {"action": "focus", "address": "t51"},
        {"action": "keys", "address": "t51", "keys": ["enter"]},
        {"action": "configure", "address": "t51", "model": "sonnet"},
        {"action": "run", "agent": "claude", "task": "x", "input_from": "t5z"},
    ],
)
def test_confirmations_never_say_the_internal_address(args: dict) -> None:
    labels = {"t51": "Claude Code de EzAgent", "t5y": "OpenCode de EzAgent"}
    summary = th._delegate_summary(args, labels.get)
    assert not _ADDRESS.search(summary), summary
    unknown = th._delegate_summary(args, lambda address: "")
    assert not _ADDRESS.search(unknown), unknown


def test_kill_confirmation_names_the_tab() -> None:
    summary = th._delegate_summary(
        {"action": "kill", "address": "t5y"}, {"t5y": "OpenCode de EzAgent"}.get
    )
    assert summary == "Terminar OpenCode de EzAgent."


def test_every_result_with_an_address_carries_its_visible_name() -> None:
    herdr = SimpleNamespace(label_for={"t51": "Claude Code de EzAgent"}.get)
    known = th._with_labels(herdr, lambda args: ToolResult(ok=True, data={"address": "t51"}), {})
    assert known.data["label"] == "Claude Code de EzAgent"
    fresh = th._with_labels(
        herdr,
        lambda args: ToolResult(ok=True, data={"address": "t60", "name": "QA", "space": "EzAgent"}),
        {},
    )
    assert fresh.data["label"] == "QA de EzAgent"


def test_the_prompt_forbids_saying_addresses() -> None:
    assert "nunca la digas ni la escribas" in EXECUTION_HINT
    assert "Da su dirección real" not in EXECUTION_HINT


# ------------------------------------------------------------- avisos al terminar
def _app(labels: dict[str, str]):
    from ezagent.voice.app import VoiceApp

    app = VoiceApp.__new__(VoiceApp)
    app.herdr = SimpleNamespace(label_for=labels.get)
    return app


@pytest.mark.parametrize("reason", ["done", "blocked", "error", "lost"])
def test_the_done_notice_names_the_tab_and_never_reads_back_the_request(reason: str) -> None:
    request = "El input de texto del chat debe tener el mismo espaciado arriba y abajo"
    question = _app({"t51": "Claude Code de EzAgent"})._done_question(
        "t51", "claude-code", reason, request
    )
    assert "Claude Code de EzAgent" in question
    assert "espaciado" not in question and "«" not in question
    assert not _ADDRESS.search(question)


def test_without_a_tab_name_the_notice_uses_the_agent_name_capitalized() -> None:
    question = _app({})._done_question("t9", "codex", "lost", "una tarea cualquiera")
    assert question.startswith("Codex se cerró")
    assert _app({})._done_question("t9", "", "blocked", "x").startswith("El agente necesita")


def test_a_chain_notice_keeps_the_stage_names() -> None:
    from ezagent.agent.orchestration import FLOW_PREFIX

    flow = FLOW_PREFIX + "planificador → desarrollador"
    question = _app({})._done_question("t2", "opencode", "done", flow)
    assert "planificador → desarrollador" in question


def test_live_activity_rows_are_titled_by_tab_not_by_request() -> None:
    from ezagent.voice.activity import live_items

    app = SimpleNamespace(
        watched={"t51"},
        tasks=SimpleNamespace(
            get=lambda a: {"agent": "claude-code", "task": "Iguala el espaciado"}, chains=list
        ),
        watcher=SimpleNamespace(state_of=lambda a: "working"),
        herdr=SimpleNamespace(label_for={"t51": "Claude Code de EzAgent"}.get),
    )
    (item,) = live_items(app, now=1.0)
    assert item.title == "Claude Code de EzAgent" and item.agent == ""


# ----------------------------------------------------- sin nombres dobles
def test_the_model_only_sees_the_visible_name_not_the_raw_fields() -> None:
    herdr = SimpleNamespace(label_for=lambda address: "")
    raw = {
        "address": "t51",
        "agent": "claude-code",
        "where": "EzAgent / Claude Code",
        "name": "Claude Code",
        "title": "Claude Code",
        "labels": {"where": "EzAgent / Claude Code"},
    }
    listed = th._with_labels(herdr, lambda args: ToolResult(ok=True, data=[dict(raw)]), {})
    (item,) = listed.data
    assert item["label"] == "Claude Code de EzAgent"
    assert not {"where", "name", "title", "labels"} & set(item)
    single = th._with_labels(herdr, lambda args: ToolResult(ok=True, data=dict(raw)), {})
    assert single.data["label"] == "Claude Code de EzAgent" and "where" not in single.data


def test_the_prompt_context_has_no_raw_where_or_name(monkeypatch) -> None:
    from ezagent.voice.herdr import Herdr

    herdr = Herdr.__new__(Herdr)
    import threading

    herdr._context_lock = threading.Lock()
    herdr._agents_cache = None
    herdr._agents_at = None
    herdr._context_snapshot = None
    herdr._context_at = None
    herdr._context_pending = False
    agent = {
        "address": "t51",
        "agent": "claude-code",
        "state": "working",
        "cwd": "C:/x/EzAgent",
        "where": "EzAgent / Claude Code",
        "name": "Claude Code",
        "label": "Claude Code de EzAgent",
    }
    monkeypatch.setattr(herdr, "cached_agents", lambda max_age=1.5: [agent], raising=False)
    herdr._refresh_context()
    (terminal,) = herdr._context_snapshot["terminals"]
    assert terminal["label"] == "Claude Code de EzAgent"
    assert "where" not in terminal and "name" not in terminal
    assert herdr.label_for("t51") == "Claude Code de EzAgent"


def test_the_prompt_forbids_prefixing_the_agent_type() -> None:
    assert "nunca «Enviado a Claude Code en Claude Code de EzAgent»" in EXECUTION_HINT

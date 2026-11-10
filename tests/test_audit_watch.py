"""Regresiones de la auditoría: watcher, contexto de Herdr, sesión y condensador.

Cubren F04/F24 (final verificado y entrega fuera del polling), F50 (caché de
Herdr y frescura estable), F25 (plazos reales y lectura desconocida) y F44
(condensador con streaming).
"""

from __future__ import annotations

import json
import queue
import subprocess
import threading
import time
from types import SimpleNamespace
from typing import Any
from unittest.mock import Mock

import pytest

from ezagent.voice import agent_session as session_module
from ezagent.voice import herdr as herdr_module
from ezagent.voice import watcher as watcher_module
from ezagent.voice.agent_session import AgentSession
from ezagent.voice.condenser import Condenser
from ezagent.voice.config import VoiceConfig
from ezagent.voice.herdr import Herdr
from ezagent.voice.watcher import Watcher

# --------------------------------------------------------------------- watcher


def make_watcher(
    on_event: Any = None,
    has_targets: Any = None,
) -> Watcher:
    return Watcher(
        VoiceConfig(),
        on_event=on_event or (lambda *args: None),
        should_announce=lambda address: True,
        has_targets=has_targets,
    )


def drain(watcher: Watcher) -> list[tuple[str, str, str, str]]:
    found: list[tuple[str, str, str, str]] = []
    while True:
        try:
            event = watcher._events.get_nowait()
        except queue.Empty:
            return found
        if event is not None:
            found.append(event)


def terminal(address: str, state: str, app: str = "codex") -> dict[str, Any]:
    return {"address": address, "status": {"state": state, "app": app}}


def completed(
    *,
    terminals: list[dict[str, Any]] | None = None,
    returncode: int = 0,
    stdout: str | None = None,
) -> subprocess.CompletedProcess:
    if stdout is None:
        stdout = json.dumps({"terminals": terminals or []})
    return subprocess.CompletedProcess([], returncode, stdout=stdout, stderr="")


def run_always(monkeypatch: pytest.MonkeyPatch, *results: subprocess.CompletedProcess) -> None:
    pending = list(results)

    def run(*args: Any, **kwargs: Any) -> subprocess.CompletedProcess:
        return pending.pop(0) if len(pending) > 1 else pending[0]

    monkeypatch.setattr(watcher_module.subprocess, "run", run)


class StopAfter:
    """Sustituye a threading.Event para recorrer el bucle un número fijo de veces."""

    def __init__(self, polls: int) -> None:
        self.polls = polls
        self.delays: list[float] = []

    def is_set(self) -> bool:
        return len(self.delays) >= self.polls

    def wait(self, delay: float) -> bool:
        self.delays.append(delay)
        return len(self.delays) >= self.polls


def test_a_vanished_terminal_is_lost_never_done() -> None:
    watcher = make_watcher()
    watcher._compare([terminal("t1", "working")])
    watcher._compare([])
    assert drain(watcher) == [("t1", "codex", "lost", "lost")]


def test_done_requires_an_observed_working_transition() -> None:
    watcher = make_watcher()
    watcher._compare([terminal("t1", "idle")])
    watcher._compare([terminal("t1", "done")])
    assert drain(watcher) == []
    watcher._compare([terminal("t1", "working")])
    watcher._compare([terminal("t1", "done")])
    assert drain(watcher) == [("t1", "codex", "done", "done")]


def test_failed_hc_keeps_the_snapshot_and_reports_health(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    watcher = make_watcher()
    health: list[bool] = []
    watcher.on_health = health.append
    responses = [
        completed(terminals=[terminal("t1", "working")]),
        completed(returncode=1, stdout=""),
        completed(stdout=""),
        completed(stdout="no es json"),
        completed(terminals=[terminal("t1", "working")]),
        completed(terminals=[terminal("t1", "done")]),
    ]
    run_always(monkeypatch, *responses)
    watcher._stop = StopAfter(len(responses))  # type: ignore[assignment]
    watcher._loop()
    # Ninguna consulta fallida produjo "lost" ni "done"; el snapshot sobrevive.
    assert drain(watcher) == [("t1", "codex", "done", "done")]
    assert health == [False, True]


def test_backoff_grows_to_five_seconds(monkeypatch: pytest.MonkeyPatch) -> None:
    watcher = make_watcher()
    run_always(monkeypatch, completed(returncode=1))
    stop = StopAfter(4)
    watcher._stop = stop  # type: ignore[assignment]
    watcher._loop()
    assert stop.delays == [1.0, 2.0, 5.0, 5.0]


@pytest.mark.parametrize(
    ("targets", "expected"),
    [(True, watcher_module.POLL_S), (False, watcher_module.IDLE_POLL_S)],
)
def test_poll_interval_depends_on_watched_targets(
    monkeypatch: pytest.MonkeyPatch,
    targets: bool,
    expected: float,
) -> None:
    watcher = make_watcher(has_targets=lambda: targets)
    run_always(monkeypatch, completed())
    stop = StopAfter(2)
    watcher._stop = stop  # type: ignore[assignment]
    watcher._loop()
    assert stop.delays == [expected, expected]


def test_callbacks_run_outside_the_polling_thread(monkeypatch: pytest.MonkeyPatch) -> None:
    entered = threading.Event()
    release = threading.Event()
    seen: list[tuple[str, str]] = []

    def on_event(address: str, agent: str, state: str, reason: str) -> None:
        seen.append((address, reason))
        entered.set()
        release.wait(2.0)

    watcher = make_watcher(on_event=on_event, has_targets=lambda: False)
    snapshot = completed(terminals=[terminal("t1", "working"), terminal("t2", "working")])
    run_always(monkeypatch, snapshot)
    watcher.start()
    try:
        watcher._compare([terminal("t1", "blocked"), terminal("t2", "working")])
        assert entered.wait(1.0)
        started = time.monotonic()
        watcher._compare([terminal("t1", "blocked"), terminal("t2", "done")])
        assert time.monotonic() - started < 0.5
    finally:
        release.set()
        watcher.stop()
    assert ("t1", "blocked") in seen
    assert ("t2", "done") in seen


def test_stop_joins_both_threads(monkeypatch: pytest.MonkeyPatch) -> None:
    watcher = make_watcher(has_targets=lambda: False)
    run_always(monkeypatch, completed())
    watcher.start()
    watcher.stop()
    assert watcher._thread is not None and not watcher._thread.is_alive()
    assert watcher._delivery is not None and not watcher._delivery.is_alive()


# ----------------------------------------------------------------------- herdr


class FakeClock:
    def __init__(self) -> None:
        self.now = 1000.0

    def monotonic(self) -> float:
        return self.now

    def time(self) -> float:
        return self.now


class ImmediateThread:
    def __init__(self, *, target: Any, **kwargs: Any) -> None:
        self.target = target

    def start(self) -> None:
        self.target()


def make_herdr(monkeypatch: pytest.MonkeyPatch) -> Herdr:
    monkeypatch.setattr(Herdr, "_resolve_paths", lambda self: None)
    return Herdr(VoiceConfig())


def test_cached_agents_reuses_a_recent_snapshot(monkeypatch: pytest.MonkeyPatch) -> None:
    herdr = make_herdr(monkeypatch)
    clock = FakeClock()
    monkeypatch.setattr(herdr_module, "time", clock)
    calls: list[dict[str, Any]] = []
    agents = [{"address": "a", "agent": "codex", "state": "working"}]

    def list_agents(**kwargs: Any) -> list[dict[str, Any]]:
        calls.append(kwargs)
        return [dict(agents[0])]

    monkeypatch.setattr(herdr, "list_agents", list_agents)
    assert herdr.cached_agents() is None
    herdr._refresh_context()
    assert len(calls) == 1
    assert herdr.cached_agents() == agents
    herdr._refresh_context()  # caché reciente: no lanza otro `hc ls`
    assert len(calls) == 1
    clock.now += 2.0
    assert herdr.cached_agents() is None
    herdr._refresh_context()
    assert len(calls) == 2


def test_context_reports_freshness_instead_of_exact_age(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    herdr = make_herdr(monkeypatch)
    clock = FakeClock()
    monkeypatch.setattr(herdr_module, "time", clock)
    monkeypatch.setattr(herdr_module.threading, "Thread", ImmediateThread)
    monkeypatch.setattr(
        herdr,
        "list_agents",
        lambda **kwargs: [{"address": "a", "agent": "codex", "state": "working"}],
    )
    herdr._refresh_context()
    context = herdr.context_text()
    assert '"freshness": "reciente"' in context
    assert "age_seconds" not in context
    clock.now += 60.0
    context = herdr.context_text()
    assert '"freshness": "antiguo"' in context


# ------------------------------------------------------------------ agent session


class FakeSession(AgentSession):
    def __init__(
        self,
        *,
        screen: str = "",
        read_error: bool = False,
    ) -> None:
        self.cfg = VoiceConfig()
        self.screen = screen
        self.read_error = read_error
        self.state = ""
        self.gets = 0
        self.keys: list[list[str]] = []
        self._sleep = lambda seconds: None

    def get_agent(self, address: str) -> dict[str, Any]:
        self.gets += 1
        return {"agent": "codex", "state": self.state}

    def read_agent(self, address: str, lines: int | None = None) -> str:
        if self.read_error:
            raise OSError("terminal ilegible")
        return self.screen

    def send_agent(self, address: str, text: str, confirmed: bool = False) -> dict[str, Any]:
        return {"ok": True}

    def keys_agent(self, address: str, keys: list[str]) -> dict[str, Any]:
        self.keys.append(list(keys))
        return {"ok": True}


def test_unreadable_screen_never_confirms_delivery() -> None:
    session = FakeSession(read_error=True)
    assert session.confirm_submitted("t1", "haz algo") is False
    assert session.keys == []


def test_empty_screen_never_confirms_delivery() -> None:
    session = FakeSession(screen="")
    assert session.confirm_submitted("t1", "haz algo") is False
    assert session.keys == []


def test_prompt_free_screen_confirms_delivery() -> None:
    session = FakeSession(screen="PS C:\\Users\\JD>")
    assert session.confirm_submitted("t1", "haz algo") is True


def test_unsent_task_is_not_confirmed_after_one_enter(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(session_module, "CONFIRM_WINDOW_S", 0.0)
    session = FakeSession(screen="› haz algo importante")
    assert session.confirm_submitted("t1", "haz algo importante") is False
    assert session.keys == [["enter"]]


def test_wait_ready_deadline_includes_slow_reads(monkeypatch: pytest.MonkeyPatch) -> None:
    session = FakeSession()
    clock = [0.0]
    monkeypatch.setattr(session_module.time, "monotonic", lambda: clock[0])

    def slow_get(address: str) -> dict[str, Any]:
        session.gets += 1
        clock[0] += 40.0
        return {"agent": "codex", "state": "working"}

    session.get_agent = slow_get
    assert session.wait_ready("t1", timeout=30.0) is False
    assert session.gets == 1


# -------------------------------------------------------------------- condenser


def chunk(text: str) -> Any:
    return SimpleNamespace(choices=[SimpleNamespace(delta=SimpleNamespace(content=text))])


class FakeStream:
    def __init__(self, chunks: list[Any]) -> None:
        self.chunks = chunks
        self.closed = False

    def __iter__(self) -> Any:
        return iter(self.chunks)

    def close(self) -> None:
        self.closed = True


class StreamCompletions:
    def __init__(self, chunks: list[Any]) -> None:
        self.chunks = chunks
        self.requests: list[dict[str, Any]] = []
        self.streams: list[FakeStream] = []

    def create(self, **request: Any) -> FakeStream:
        self.requests.append(request)
        stream = FakeStream(self.chunks)
        self.streams.append(stream)
        return stream


class BrokenStream(FakeStream):
    def __iter__(self) -> Any:
        yield chunk("1")
        raise RuntimeError("stream cortado")


class BrokenCompletions(StreamCompletions):
    def create(self, **request: Any) -> FakeStream:
        self.requests.append(request)
        stream = BrokenStream([])
        self.streams.append(stream)
        return stream


class NoStreamCompletions:
    def __init__(self, content: str) -> None:
        self.content = content
        self.requests: list[dict[str, Any]] = []

    def create(self, **request: Any) -> Any:
        self.requests.append(request)
        if "stream" in request:
            raise TypeError("create() got an unexpected keyword argument 'stream'")
        return SimpleNamespace(
            choices=[SimpleNamespace(message=SimpleNamespace(content=self.content))]
        )


def make_client(completions: Any) -> SimpleNamespace:
    return SimpleNamespace(chat=SimpleNamespace(completions=completions))


def test_condenser_reads_and_closes_a_stream() -> None:
    completions = StreamCompletions([chunk("1"), chunk("2"), chunk("")])
    condenser = Condenser(make_client(completions), VoiceConfig())
    lines = [f"línea {number}" for number in range(1, 20)]
    assert condenser.conclusion_start(lines) == 12
    assert completions.requests[0]["stream"] is True
    assert completions.streams[0].closed is True


def test_condenser_falls_back_to_a_plain_completion() -> None:
    completions = NoStreamCompletions("3")
    condenser = Condenser(make_client(completions), VoiceConfig())
    lines = [f"línea {number}" for number in range(1, 10)]
    assert condenser.conclusion_start(lines) == 3
    assert [("stream" in request) for request in completions.requests] == [True, False]


def test_condenser_accepts_a_client_that_ignores_stream_true() -> None:
    reply = SimpleNamespace(choices=[SimpleNamespace(message=SimpleNamespace(content="4"))])
    completions = Mock()
    completions.create.return_value = reply
    condenser = Condenser(make_client(completions), VoiceConfig())
    lines = [f"línea {number}" for number in range(1, 10)]
    assert condenser.conclusion_start(lines) == 4


def test_condenser_closes_a_broken_stream_and_gives_up() -> None:
    completions = BrokenCompletions([])
    condenser = Condenser(make_client(completions), VoiceConfig())
    lines = [f"línea {number}" for number in range(1, 10)]
    assert condenser.conclusion_start(lines) is None
    assert completions.streams[0].closed is True

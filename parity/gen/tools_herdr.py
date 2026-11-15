"""Fixtures de las herramientas de Herdr (dominio `tools_herdr`, F1-15).

Cubre `delegate_to_agent` y `voice_control`: validación de argumentos,
resúmenes literales de confirmación, niveles de riesgo, `then`/cadenas,
`input_from`, avisos y el constructor de las dos herramientas. `Herdr` y sus
callbacks son dobles que registran cada llamada; el reloj y `uuid4` se
congelan. No se abre ninguna terminal ni se ejecuta ningún proceso.
"""

from __future__ import annotations

import contextlib
import uuid
from collections.abc import Callable, Iterator
from dataclasses import asdict
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "tools_herdr"

KNOWN_AGENTS = {"claude", "opencode", "codex", "mcode", "minimax", "pi", "qwen"}


def _capture(
    out: FixtureFile,
    case: str,
    fn_name: str,
    input_data: Any,
    action: Callable[[], Any],
) -> None:
    try:
        value = action()
    except Exception as exc:  # noqa: BLE001 - el oráculo registra el texto real
        out.add_error(case, fn_name, input_data, str(exc))
        return
    out.add(case, fn_name, input_data, value)


@contextlib.contextmanager
def _uuids() -> Iterator[None]:
    original = uuid.uuid4
    values = ["a1b2c3d4e5f60718", "b2c3d4e5f6071829", "c3d4e5f60718293a"]
    state = {"index": 0}

    class FakeUuid:
        def __init__(self, hex_value: str) -> None:
            self.hex = hex_value

    def fake() -> FakeUuid:
        value = FakeUuid(values[min(state["index"], len(values) - 1)])
        state["index"] += 1
        return value

    uuid.uuid4 = fake  # type: ignore[assignment]
    try:
        yield
    finally:
        uuid.uuid4 = original  # type: ignore[assignment]


def _raise_if(value: Any) -> Any:
    if isinstance(value, dict) and value.get("raise"):
        from ezagent.voice.herdr import HerdrError

        message = str(value.get("message") or "")
        if value["raise"] == "HerdrError":
            raise HerdrError(message)
        if value["raise"] == "HerdrNotFound":
            from ezagent.voice.herdr import HerdrNotFound

            raise HerdrNotFound(message)
        raise ValueError(message)
    return value


class FakeHerdr:
    def __init__(self, spec: dict[str, Any], calls: list[dict[str, Any]]) -> None:
        self.spec = spec
        self.calls = calls

    def _call(self, kind: str, **data: Any) -> None:
        self.calls.append({"kind": kind, **data})

    def resolve_agent(self, name: str) -> str | None:
        self._call("resolve_agent", name=name)
        mapping = self.spec.get("resolve")
        if mapping is None:
            return name if name in KNOWN_AGENTS else None
        return mapping.get(name)

    def label_for(self, address: str) -> str:
        self._call("label_for", address=address)
        _raise_if(self.spec.get("label_raise"))
        if self.spec.get("label_raises"):
            raise ValueError("nombre roto")
        return str(self.spec.get("labels", {}).get(address, ""))

    def list_agents(self) -> list[dict[str, Any]]:
        self._call("list_agents")
        return _raise_if(self.spec.get("list", []))

    def get_agent(self, address: str) -> dict[str, Any]:
        self._call("get_agent", address=address)
        return _raise_if(self.spec.get("get", {"agent": "codex", "state": "done"}))

    def read_agent(self, address: str, lines: int | None = None, limit: int = 6000) -> str:
        self._call("read_agent", address=address, lines=lines, limit=limit)
        if self.spec.get("read_error"):
            raise OSError("terminal ilegible")
        return str(self.spec.get("read", ""))

    def read_full(self, address: str, lines: int = 1500) -> str:
        self._call("read_full", address=address, lines=lines)
        if self.spec.get("read_full_error"):
            raise OSError("terminal ilegible")
        return str(self.spec.get("read_full", self.spec.get("read", "")))

    def send_agent(self, address: str, text: str, confirmed: bool = False) -> dict[str, Any]:
        self._call("send_agent", address=address, text=text, confirmed=confirmed)
        return _raise_if(
            self.spec.get("send", {"ok": True, "address": address, "message": "Mensaje enviado."})
        )

    def run_agent(self, agent: str, task: str, **kwargs: Any) -> dict[str, Any]:
        on_ready = kwargs.pop("on_ready", None)
        self._call(
            "run_agent",
            agent=agent,
            task=task,
            kwargs=kwargs,
            has_on_ready=on_ready is not None,
        )
        if self.spec.get("call_on_ready") and on_ready is not None:
            on_ready(self.spec.get("on_ready", {}))
        return _raise_if(self.spec.get("run", {"ok": True, "address": "t1", "terminal": "t1"}))

    def open_terminal(
        self,
        name: str,
        cwd: str | None = None,
        shell: str = "powershell.exe",
        space: str | None = None,
    ) -> dict[str, Any]:
        self._call("open_terminal", name=name, cwd=cwd, shell=shell, space=space)
        return _raise_if(
            self.spec.get("open_terminal", {"ok": True, "terminal": "t1", "address": "t1"})
        )

    def agents_catalog(self) -> list[dict[str, Any]]:
        self._call("agents_catalog")
        return _raise_if(self.spec.get("catalog", []))

    def configure_agent(
        self, address: str, model: str | None = None, effort: str | None = None
    ) -> dict[str, Any]:
        self._call("configure_agent", address=address, model=model, effort=effort)
        return _raise_if(
            self.spec.get(
                "configure", {"ok": True, "address": address, "agent": "codex", "applied": []}
            )
        )

    def keys_agent(self, address: str, keys: list[str]) -> dict[str, Any]:
        self._call("keys_agent", address=address, keys=list(keys))
        return _raise_if(self.spec.get("keys", {"ok": True, "address": address, "keys": keys}))

    def focus_agent(self, address: str) -> dict[str, Any]:
        self._call("focus_agent", address=address)
        return _raise_if(self.spec.get("focus", {"ok": True, "address": address, "focused": True}))

    def kill_agent(self, address: str, confirmed: bool = False) -> dict[str, Any]:
        self._call("kill_agent", address=address, confirmed=confirmed)
        return _raise_if(
            self.spec.get(
                "kill", {"ok": True, "address": address, "message": "Terminal terminado."}
            )
        )

    def save_context(self, text: str) -> str | None:
        self._call("save_context", text=text)
        value = self.spec.get("save_context")
        return None if value is None else str(value)


class FakeCallbacks:
    def __init__(self, spec: dict[str, Any], calls: list[dict[str, Any]]) -> None:
        self.spec = spec
        self.calls = calls
        self.async_launch = bool(spec.get("async_launch", False))

    def _call(self, kind: str, **data: Any) -> None:
        self.calls.append({"kind": kind, **data})

    def watch(self, address: str) -> None:
        self._call("watch", address=address)
        _raise_if(self.spec.get("watch_raise"))

    def request_close(self) -> None:
        self._call("request_close")
        _raise_if(self.spec.get("close_raise"))

    def mic_mute(self, muted: bool) -> None:
        self._call("mic_mute", muted=muted)
        _raise_if(self.spec.get("mic_raise"))

    def last_summary(self) -> str:
        self._call("last_summary")
        return str(self.spec.get("summary", ""))

    def mute(self, minutes: int) -> None:
        self._call("mute", minutes=minutes)
        _raise_if(self.spec.get("mute_raise"))

    def get_last_user_text(self) -> str:
        self._call("get_last_user_text")
        _raise_if(self.spec.get("user_text_raise"))
        return str(self.spec.get("user_text", ""))

    def remember_task(self, address: str, agent: str, task: str) -> None:
        self._call("remember_task", address=address, agent=agent, task=task)

    def enqueue_chain(self, spec: dict[str, Any]) -> None:
        self._call("enqueue_chain", spec=spec)

    def deliver_answer(self, address: str) -> None:
        self._call("deliver_answer", address=address)

    def prewarm_chain(self, spec: dict[str, Any]) -> None:
        self._call("prewarm_chain", spec=spec)

    def launch_failed(self, address: str, agent: str, error: str) -> None:
        self._call("launch_failed", address=address, agent=agent, error=error)


def _callbacks(spec: dict[str, Any], calls: list[dict[str, Any]]) -> Any:
    from ezagent.agent.tools_herdr import HerdrCallbacks

    fake = FakeCallbacks(spec, calls)
    return HerdrCallbacks(
        watch=fake.watch,
        request_close=fake.request_close,
        mic_mute=fake.mic_mute,
        last_summary=fake.last_summary,
        mute=fake.mute,
        get_last_user_text=fake.get_last_user_text,
        remember_task=fake.remember_task,
        enqueue_chain=fake.enqueue_chain,
        deliver_answer=fake.deliver_answer,
        prewarm_chain=fake.prewarm_chain,
        async_launch=fake.async_launch,
        launch_failed=fake.launch_failed,
    )


def _state() -> Any:
    from ezagent.agent.tools_herdr import _SessionState

    return _SessionState()


def _result_dict(result: Any) -> dict[str, Any]:
    return result.to_dict() if hasattr(result, "to_dict") else {"value": result}


def _delegate_case(
    out: FixtureFile,
    case: str,
    args: dict[str, Any],
    herdr_spec: dict[str, Any] | None = None,
    callbacks_spec: dict[str, Any] | None = None,
    state: Any | None = None,
) -> None:
    from ezagent.agent.tools_herdr import _handle_delegate

    calls: list[dict[str, Any]] = []
    herdr = FakeHerdr(herdr_spec or {}, calls)
    callbacks = _callbacks(callbacks_spec or {}, calls)
    session = state or _state()

    def action() -> dict[str, Any]:
        result = _handle_delegate(herdr, callbacks, session, args)
        return {
            "result": _result_dict(result),
            "calls": calls,
            "last_address": session.last_address,
        }

    input_data = {"args": args, "herdr": herdr_spec or {}, "callbacks": callbacks_spec or {}}
    if state is not None:
        input_data["last_address"] = state.last_address
    _capture(out, case, "_handle_delegate", input_data, action)


def _run_case(
    out: FixtureFile,
    case: str,
    args: dict[str, Any],
    herdr_spec: dict[str, Any] | None = None,
    callbacks_spec: dict[str, Any] | None = None,
    state: Any | None = None,
) -> None:
    from ezagent.agent.tools_herdr import _handle_run

    calls: list[dict[str, Any]] = []
    herdr = FakeHerdr(herdr_spec or {}, calls)
    callbacks = _callbacks(callbacks_spec or {}, calls)
    session = state or _state()

    def action() -> dict[str, Any]:
        with _uuids():
            result = _handle_run(herdr, callbacks, session, args)
        return {
            "result": _result_dict(result),
            "calls": calls,
            "last_address": session.last_address,
        }

    input_data = {"args": args, "herdr": herdr_spec or {}, "callbacks": callbacks_spec or {}}
    if state is not None:
        input_data["last_address"] = state.last_address
    _capture(out, case, "_handle_run", input_data, action)


def _voice_case(
    out: FixtureFile,
    case: str,
    args: dict[str, Any],
    callbacks_spec: dict[str, Any] | None = None,
    state: Any | None = None,
) -> None:
    from ezagent.agent.tools_herdr import _handle_voice

    calls: list[dict[str, Any]] = []
    callbacks = _callbacks(callbacks_spec or {}, calls)
    session = state or _state()

    def action() -> dict[str, Any]:
        result = _handle_voice(callbacks, session, args)
        return {
            "result": _result_dict(result),
            "calls": calls,
            "last_address": session.last_address,
        }

    input_data = {"args": args, "callbacks": callbacks_spec or {}}
    if state is not None:
        input_data["last_address"] = state.last_address
    _capture(out, case, "_handle_voice", input_data, action)


# ---------------------------------------------------------------- argumentos
def _argument_cases(out: FixtureFile) -> None:
    from ezagent.agent import tools_herdr as th

    action_cases = [
        {"action": "List"},
        {"action": "  RUN  "},
        {"action": ""},
        {"action": 5},
        {"action": None},
        {},
    ]
    for index, args in enumerate(action_cases, start=1):
        _capture(
            out,
            f"tools_herdr/args/action/{index:03d}",
            "_action",
            {"args": args},
            lambda item=args: {"action": th._action(item)},
        )

    checked_cases = [
        ({"action": "list"}, ("list", "get"), "t"),
        ({"action": "LIST"}, ("list",), "t"),
        ({"action": "nada"}, ("list", "get"), "t"),
        ({}, ("list",), "t"),
        ({"action": " "}, ("list",), "t"),
        ({"action": "a"}, ("a", "b", "c"), "delegate_to_agent"),
    ]
    for index, (args, valid, tool) in enumerate(checked_cases, start=1):
        _capture(
            out,
            f"tools_herdr/args/checked_action/{index:03d}",
            "_checked_action",
            {"args": args, "valid": list(valid), "tool": tool},
            lambda a=args, v=valid, t=tool: {"action": th._checked_action(a, v, t)},
        )

    required_cases = [
        ({"address": " t1 "}, "address", "get"),
        ({"address": ""}, "address", "get"),
        ({"address": "   "}, "address", "get"),
        ({"address": 5}, "address", "get"),
        ({"address": ["t1"]}, "address", "get"),
        ({}, "address", "get"),
        ({"task": True}, "task", "send"),
    ]
    for index, (args, key, action) in enumerate(required_cases, start=1):
        _capture(
            out,
            f"tools_herdr/args/required_text/{index:03d}",
            "_required_text",
            {"args": args, "key": key, "action": action},
            lambda a=args, k=key, t=action: {"text": th._required_text(a, k, t)},
        )

    optional_cases = [
        ({"x": " t1 "}, "x", "get"),
        ({"x": "   "}, "x", "get"),
        ({"x": ""}, "x", "get"),
        ({"x": None}, "x", "get"),
        ({}, "x", "get"),
        ({"x": 5}, "x", "get"),
        ({"x": False}, "x", "get"),
        ({"x": ["a"]}, "x", "get"),
    ]
    for index, (args, key, action) in enumerate(optional_cases, start=1):
        _capture(
            out,
            f"tools_herdr/args/optional_text/{index:03d}",
            "_optional_text",
            {"args": args, "key": key, "action": action},
            lambda a=args, k=key, t=action: {"text": th._optional_text(a, k, t)},
        )

    bool_cases = [
        ({"x": True}, "x", "run"),
        ({"x": False}, "x", "run"),
        ({"x": None}, "x", "run"),
        ({}, "x", "run"),
        ({"x": 1}, "x", "run"),
        ({"x": "true"}, "x", "run"),
        ({"x": 0}, "x", "run"),
    ]
    for index, (args, key, action) in enumerate(bool_cases, start=1):
        _capture(
            out,
            f"tools_herdr/args/optional_bool/{index:03d}",
            "_optional_bool",
            {"args": args, "key": key, "action": action},
            lambda a=args, k=key, t=action: {"value": th._optional_bool(a, k, t)},
        )

    lines_cases = [
        {"lines": 10},
        {"lines": 120},
        {"lines": 9},
        {"lines": 121},
        {"lines": 0},
        {"lines": -1},
        {"lines": True},
        {"lines": 20.0},
        {"lines": "20"},
        {},
        {"lines": None},
        {"lines": 20},
    ]
    for index, args in enumerate(lines_cases, start=1):
        _capture(
            out,
            f"tools_herdr/args/optional_lines/{index:03d}",
            "_optional_lines",
            {"args": args},
            lambda item=args: {"lines": th._optional_lines(item, "read")},
        )

    wrapped_cases = [
        {"ok": True, "data": {"x": 1}},
        {"ok": False, "error": "fallo"},
        {"ok": False, "error": ""},
        {"ok": False, "error": None},
        {"ok": False},
        {"ok": None, "x": 1},
        [1, 2],
        None,
        "texto",
    ]
    for index, value in enumerate(wrapped_cases, start=1):
        _capture(
            out,
            f"tools_herdr/args/wrapped/{index:03d}",
            "_wrapped",
            {"value": value},
            lambda item=value: {"result": th._wrapped(item).to_dict()},
        )

    user_cases = [
        ("lower", {}, "Hola"),
        ("raises", {"user_text_raise": {"raise": "ValueError", "message": "x"}}, ""),
        ("none", {"user_text": None}, ""),
        ("int", {"user_text": 5}, "5"),
    ]
    for index, (name, spec, _) in enumerate(user_cases, start=1):
        calls: list[dict[str, Any]] = []
        callbacks = _callbacks(spec, calls)
        _capture(
            out,
            f"tools_herdr/args/user_text/{index:03d}-{name}",
            "_user_text",
            {"callbacks": spec},
            lambda c=callbacks: {"text": th._user_text(c)},
        )

    words = [
        "avísame",
        "avisame",
        "avisa",
        "notifícame",
        "cuando termine",
        "vigila",
        "dime",
        "resumen",
        "otra cosa",
        "",
        "AVÍSAME",
    ]
    for index, text in enumerate(words, start=1):
        _capture(
            out,
            f"tools_herdr/args/wants/{index:03d}",
            "_wants_notify",
            {"text": text},
            lambda value=text: {
                "notify": th._wants_notify(value),
                "deliver": th._wants_deliver(value),
                "close": th._wants_close(value),
            },
        )


# ----------------------------------------------------------------- resúmenes
SUMMARY_CASES: list[tuple[str, dict[str, Any]]] = [
    ("list", {"action": "list"}),
    ("get", {"action": "get", "address": "t51"}),
    ("get-no-address", {"action": "get"}),
    ("read-lines", {"action": "read", "address": "t51", "lines": 20}),
    ("read-no-lines", {"action": "read", "address": "t51"}),
    ("read-no-address", {"action": "read"}),
    ("read-bool-lines", {"action": "read", "address": "t51", "lines": True}),
    ("send", {"action": "send", "address": "t51", "task": "haz algo"}),
    ("send-no-task", {"action": "send", "address": "t51"}),
    ("send-no-address", {"action": "send", "task": "x"}),
    ("agents", {"action": "agents"}),
    ("guide", {"action": "guide", "agent": "claude"}),
    ("guide-empty", {"action": "guide"}),
    ("keys", {"action": "keys", "address": "t51", "keys": ["enter", "esc"]}),
    ("keys-not-list", {"action": "keys", "address": "t51", "keys": "enter"}),
    ("configure-model", {"action": "configure", "address": "t51", "model": "sonnet"}),
    ("configure-effort", {"action": "configure", "address": "t51", "effort": "high"}),
    (
        "configure-both",
        {"action": "configure", "address": "t51", "model": "sonnet", "effort": "high"},
    ),
    ("configure-none", {"action": "configure", "address": "t51"}),
    ("configure-no-address", {"action": "configure", "model": "m"}),
    ("run-open", {"action": "run", "agent": "claude"}),
    ("run-task", {"action": "run", "agent": "claude", "task": "haz algo"}),
    (
        "run-details",
        {
            "action": "run",
            "agent": "claude",
            "task": "haz algo",
            "model": "sonnet",
            "effort": "high",
            "input_from": "t5z",
            "gui": True,
        },
    ),
    (
        "run-background",
        {"action": "run", "agent": "claude", "task": "x", "background": True},
    ),
    (
        "run-then",
        {
            "action": "run",
            "agent": "claude",
            "task": "x",
            "then": [{"agent": "codex", "model": "gpt-5.1"}, {"agent": "pi", "effort": "high"}],
        },
    ),
    ("run-then-invalid", {"action": "run", "agent": "claude", "task": "x", "then": "no"}),
    ("open-terminal", {"action": "open_terminal", "name": "nueva"}),
    ("open-terminal-no-name", {"action": "open_terminal"}),
    ("focus", {"action": "focus", "address": "t51"}),
    ("focus-no-address", {"action": "focus"}),
    ("kill", {"action": "kill", "address": "t51"}),
    ("kill-no-address", {"action": "kill"}),
    ("watch", {"action": "watch", "address": "t51"}),
    ("watch-no-address", {"action": "watch"}),
    ("unknown", {"action": "nada"}),
    ("upper", {"action": "  LIST  "}),
]


def _summary_cases(out: FixtureFile) -> None:
    from ezagent.agent import tools_herdr as th

    labels = {"t51": "Claude Code de EzAgent", "t5z": "OpenCode de EzAgent"}
    for index, (name, args) in enumerate(SUMMARY_CASES, start=1):
        for variant, resolver in (
            ("label", labels.get),
            ("empty", lambda address: ""),
            ("raise", lambda address: (_ for _ in ()).throw(ValueError("x"))),
            ("none", None),
        ):
            _capture(
                out,
                f"tools_herdr/summary/delegate/{index:03d}-{name}-{variant}",
                "_delegate_summary",
                {
                    "args": args,
                    "labels": variant,
                    "label_map": labels if variant == "label" else None,
                    "label_raises": variant == "raise",
                },
                lambda a=args, r=resolver: {"summary": th._delegate_summary(a, r)},
            )

    voice_cases = [
        ("close", {"action": "close"}),
        ("close-and-watch", {"action": "close_and_watch", "address": "t51"}),
        ("close-and-watch-no-address", {"action": "close_and_watch"}),
        ("mic-mute-true", {"action": "mic_mute", "muted": True}),
        ("mic-mute-false", {"action": "mic_mute", "muted": False}),
        ("mic-mute-none", {"action": "mic_mute"}),
        ("mic-mute-int", {"action": "mic_mute", "muted": 1}),
        ("repeat", {"action": "repeat_last"}),
        ("mute", {"action": "mute", "minutes": 15}),
        ("mute-bool", {"action": "mute", "minutes": True}),
        ("mute-str", {"action": "mute", "minutes": "15"}),
        ("mute-none", {"action": "mute"}),
        ("unknown", {"action": "nada"}),
        ("upper", {"action": " CLOSE "}),
    ]
    for index, (name, args) in enumerate(voice_cases, start=1):
        for variant, resolver in (("label", labels.get), ("none", None)):
            _capture(
                out,
                f"tools_herdr/summary/voice/{index:03d}-{name}-{variant}",
                "_voice_summary",
                {
                    "args": args,
                    "labels": variant,
                    "label_map": labels if variant == "label" else None,
                },
                lambda a=args, r=resolver: {"summary": th._voice_summary(a, r)},
            )

    info_cases = [
        {"action": "list"},
        {"action": "kill", "address": "t51"},
        {"action": "run", "agent": "claude", "task": "x"},
        {"action": "nada"},
        {},
        {"action": "read", "address": "t51", "lines": 20},
    ]
    for index, args in enumerate(info_cases, start=1):
        _capture(
            out,
            f"tools_herdr/summary/info/{index:03d}",
            "_delegate_info",
            {"args": args, "label_map": labels},
            lambda a=args: asdict(th._delegate_info(a, labels.get)),
        )
        _capture(
            out,
            f"tools_herdr/summary/voice_info/{index:03d}",
            "_voice_info",
            {"args": args, "label_map": labels},
            lambda a=args: asdict(th._voice_info(a, labels.get)),
        )

    shown_cases = [
        ("known", "t51", labels.get),
        ("unknown", "t99", labels.get),
        ("empty", "", labels.get),
        ("no-resolver", "t51", None),
        ("blank", "t51", lambda address: "   "),
        ("raise", "t51", lambda address: (_ for _ in ()).throw(ValueError("x"))),
    ]
    for index, (name, address, resolver) in enumerate(shown_cases, start=1):
        _capture(
            out,
            f"tools_herdr/summary/shown/{index:03d}-{name}",
            "_shown",
            {"address": address, "resolver": name, "label_map": labels},
            lambda a=address, r=resolver: {"shown": th._shown(a, r)},
        )


def _label_cases(out: FixtureFile) -> None:
    from ezagent.agent import tools_herdr as th
    from ezagent.agent.registry import ToolResult

    items = [
        {"address": "t51"},
        {
            "address": "t60",
            "name": "QA",
            "space": "EzAgent",
        },
        {
            "address": "t51",
            "label": "Claude Code de EzAgent",
            "where": "EzAgent / Claude Code",
            "name": "Claude Code",
            "title": "Claude Code",
            "labels": {"where": "EzAgent / Claude Code"},
        },
        {
            "agent": "claude-code",
            "cwd": "C:/x/EzAgent",
            "where": "EzAgent / Claude Code",
        },
        {"label": "sin address"},
        {},
    ]
    for index, item in enumerate(items, start=1):
        for variant, mapping in (
            ("known", {"t51": "Claude Code de EzAgent", "t60": "QA de EzAgent"}),
            ("empty", {}),
        ):
            calls: list[dict[str, Any]] = []
            herdr = FakeHerdr({"labels": mapping}, calls)
            _capture(
                out,
                f"tools_herdr/labels/labelled/{index:03d}-{variant}",
                "_labelled",
                {"item": item, "labels": mapping},
                lambda i=item, h=herdr: {"item": th._labelled(h, dict(i))},
            )

    data_cases = [
        ("dict", {"address": "t51"}),
        ("list", [{"address": "t51"}, {"name": "QA", "space": "EzAgent"}]),
        ("nested", {"items": [{"address": "t51"}]}),
        ("scalar", "texto"),
    ]
    for index, (name, data) in enumerate(data_cases, start=1):
        calls = []
        herdr = FakeHerdr({"labels": {"t51": "Claude de EzAgent"}}, calls)
        _capture(
            out,
            f"tools_herdr/labels/with_labels/{index:03d}-{name}",
            "_with_labels",
            {"data": data, "labels": {"t51": "Claude de EzAgent"}},
            lambda d=data, h=herdr: {
                "result": th._with_labels(h, lambda args: ToolResult(ok=True, data=d), {}).to_dict()
            },
        )


# -------------------------------------------------------------- delegate
def _delegate_cases(out: FixtureFile) -> None:
    cases = [
        ("list", {"action": "list"}, {"list": [{"address": "t1", "label": "uno"}]}),
        (
            "get",
            {"action": "get", "address": "t1"},
            {"get": {"address": "t1", "state": "working"}},
        ),
        ("read", {"action": "read", "address": "t1"}, {"read": "hola"}),
        (
            "read-lines",
            {"action": "read", "address": "t1", "lines": 20},
            {"read": "lineas"},
        ),
        (
            "send",
            {"action": "send", "address": "t1", "task": "hola"},
            {"send": {"ok": True, "address": "t1"}},
        ),
        (
            "open-terminal",
            {"action": "open_terminal", "name": "nueva"},
            {"open_terminal": {"ok": True, "terminal": "t9"}},
        ),
        (
            "open-terminal-shell",
            {
                "action": "open_terminal",
                "name": "nueva",
                "cwd": "C:/x",
                "shell": "cmd.exe",
                "space": "EzAgent",
            },
            {"open_terminal": {"ok": True, "terminal": "t9"}},
        ),
        ("agents", {"action": "agents"}, {"catalog": [{"agent": "claude"}]}),
        ("guide", {"action": "guide", "agent": "claude"}, {}),
        (
            "keys",
            {"action": "keys", "address": "t1", "keys": ["enter"]},
            {"keys": {"ok": True, "address": "t1", "keys": ["enter"]}},
        ),
        (
            "configure",
            {"action": "configure", "address": "t1", "model": "sonnet"},
            {"configure": {"ok": True, "address": "t1", "agent": "claude"}},
        ),
        (
            "focus",
            {"action": "focus", "address": "t1"},
            {"focus": {"ok": True, "address": "t1"}},
        ),
        (
            "kill",
            {"action": "kill", "address": "t1"},
            {"kill": {"ok": True, "address": "t1"}},
        ),
        ("watch", {"action": "watch", "address": "t1"}, {}),
        (
            "run",
            {"action": "run", "agent": "claude", "task": "x"},
            {"run": {"ok": True, "address": "t1", "terminal": "t1"}},
        ),
        (
            "run-model",
            {"action": "run", "agent": "claude", "task": "x", "model": "sonnet", "effort": "high"},
            {"run": {"ok": True, "address": "t1", "terminal": "t1"}},
        ),
    ]
    for index, (name, args, spec) in enumerate(cases, start=1):
        callbacks = {"user_text": "avísame"} if name == "watch" else {}
        _delegate_case(
            out,
            f"tools_herdr/delegate/ok/{index:03d}-{name}",
            args,
            spec,
            callbacks,
        )

    error_cases = [
        ({}, {}),
        ({"action": "nada"}, {}),
        ({"action": "get"}, {}),
        ({"action": "get", "address": "   "}, {}),
        ({"action": "get", "address": 5}, {}),
        ({"action": "read", "address": "t1", "lines": 9}, {}),
        ({"action": "send", "address": "t1"}, {}),
        ({"action": "send", "address": "t1", "task": "  "}, {}),
        ({"action": "open_terminal"}, {}),
        ({"action": "guide", "agent": "nada"}, {}),
        ({"action": "guide", "agent": ""}, {}),
        ({"action": "guide", "agent": "a.b"}, {}),
        ({"action": "keys", "address": "t1"}, {}),
        ({"action": "keys", "address": "t1", "keys": []}, {}),
        ({"action": "keys", "address": "t1", "keys": "enter"}, {}),
        ({"action": "configure", "address": "t1"}, {}),
        ({"action": "focus"}, {}),
        ({"action": "kill"}, {}),
        ({"action": "watch"}, {}),
        ({"action": "run"}, {}),
        ({"action": "run", "agent": "nada"}, {}),
        ({"action": "run", "agent": "  "}, {}),
        ({"action": "run", "agent": "claude", "then": "mal"}, {}),
        ({"action": "run", "agent": "claude", "then": [{"task": "x"}]}, {}),
        ({"action": "run", "agent": "claude", "then": [{"agent": "a"}] * 6}, {}),
        ({"action": "run", "agent": "claude", "notify_on_done": "sí"}, {}),
        ({"action": "run", "agent": "claude", "background": 1}, {}),
        ({"action": "run", "agent": "claude", "model": 4}, {}),
        ({"action": "run", "agent": "claude", "task": 4}, {}),
        (
            "herdr-error",
            {"action": "get", "address": "t1"},
            {"get": {"raise": "HerdrError", "message": "sin conexión"}},
        ),
        (
            "not-found",
            {"action": "get", "address": "t1"},
            {"get": {"raise": "HerdrNotFound", "message": "no existe"}},
        ),
        (
            "unexpected",
            {"action": "get", "address": "t1"},
            {"get": {"raise": "ValueError", "message": "roto"}},
        ),
        (
            "wrapped-fail",
            {"action": "send", "address": "t1", "task": "x"},
            {"send": {"ok": False, "error": "no pude"}},
        ),
        (
            "wrapped-fail-default",
            {"action": "send", "address": "t1", "task": "x"},
            {"send": {"ok": False}},
        ),
    ]
    for index, item in enumerate(error_cases, start=1):
        if len(item) == 3:
            name, args, spec = item
        else:
            args, spec = item
            name = f"error{index}"
        _delegate_case(
            out,
            f"tools_herdr/delegate/error/{index:03d}-{name}",
            args,
            spec,
        )

    # kill olvida el último agente; otro address no.
    state = _state()
    state.last_address = "t1"
    _delegate_case(
        out,
        "tools_herdr/delegate/state/001-kill-last",
        {"action": "kill", "address": "t1"},
        {"kill": {"ok": True, "address": "t1"}},
        state=state,
    )
    state = _state()
    state.last_address = "t2"
    _delegate_case(
        out,
        "tools_herdr/delegate/state/002-kill-other",
        {"action": "kill", "address": "t1"},
        {"kill": {"ok": True, "address": "t1"}},
        state=state,
    )
    state = _state()
    state.last_address = "t0"
    _delegate_case(
        out,
        "tools_herdr/delegate/state/003-watch",
        {"action": "watch", "address": "t1"},
        {},
        {"user_text": "revisa esto"},
        state=state,
    )


def _run_cases(out: FixtureFile) -> None:
    base = {"action": "run", "agent": "claude", "task": "haz algo"}
    cases = [
        (
            "open",
            {"action": "run", "agent": "claude"},
            {},
            {},
        ),
        ("task", base, {}, {}),
        ("task-notify-false", {**base, "notify_on_done": False}, {}, {}),
        ("task-user-notify", base, {}, {"user_text": "avísame"}),
        ("task-user-deliver", base, {}, {"user_text": "dime"}),
        ("task-deliver", {**base, "deliver_answer": True}, {}, {}),
        ("task-close", {**base, "close_listening": True}, {}, {}),
        ("task-user-close", base, {}, {"user_text": "ciérrate"}),
        ("gui", {**base, "gui": True}, {}, {}),
        ("background", {**base, "background": True}, {}, {}),
        ("model-effort", {**base, "model": "sonnet", "effort": "high"}, {}, {}),
        (
            "then",
            {
                **base,
                "then": [
                    {"agent": "codex", "task": "revisa", "model": "gpt-5.1"},
                    {"agent": "pi"},
                ],
            },
            {},
            {},
        ),
        ("then-bad", {**base, "then": "no"}, {}, {}),
        ("then-empty-agent", {**base, "then": [{"task": "x"}]}, {}, {}),
        ("then-too-many", {**base, "then": [{"agent": "a"}] * 6}, {}, {}),
        ("then-not-dict", {**base, "then": ["x"]}, {}, {}),
        ("unknown-agent", {**base, "agent": "nada"}, {}, {}),
        ("unknown-step", {**base, "then": [{"agent": "nada"}]}, {}, {}),
        (
            "input-done",
            {**base, "input_from": "t2"},
            {"get": {"agent": "codex", "state": "done"}, "read_full": "La respuesta."},
            {},
        ),
        (
            "input-done-long",
            {**base, "input_from": "t2", "task": "continúa: {output}"},
            {
                "get": {"agent": "codex", "state": "done"},
                "read_full": "x" * 7000,
                "save_context": "<tmp>/handoff/contexto.md",
            },
            {},
        ),
        (
            "input-working",
            {**base, "input_from": "t2"},
            {"get": {"agent": "codex", "state": "working"}},
            {},
        ),
        (
            "input-blocked",
            {**base, "input_from": "t2"},
            {"get": {"agent": "codex", "state": "blocked"}},
            {},
        ),
        (
            "input-error",
            {**base, "input_from": "t2"},
            {"get": {"agent": "codex", "state": "error"}},
            {},
        ),
        (
            "input-missing",
            {**base, "input_from": "t2"},
            {"get": {"raise": "HerdrError", "message": "no existe"}},
            {},
        ),
        (
            "input-read-error",
            {**base, "input_from": "t2"},
            {"get": {"agent": "codex", "state": "done"}, "read_full_error": True},
            {},
        ),
        ("run-error", base, {"run": {"ok": False, "error": "no abrió"}}, {}),
        ("run-no-address", base, {"run": {"ok": True, "terminal": ""}}, {}),
        (
            "async",
            base,
            {
                "call_on_ready": True,
                "on_ready": {"ok": True, "address": "t9", "task_sent": True},
                "run": {"ok": True, "launching": True, "address": "t9"},
            },
            {"async_launch": True},
        ),
        (
            "async-fail",
            base,
            {
                "call_on_ready": True,
                "on_ready": {"ok": False, "error": "arranque roto"},
                "run": {"ok": True, "launching": True, "address": "t9"},
            },
            {"async_launch": True},
        ),
        (
            "then-async",
            {**base, "then": [{"agent": "codex"}]},
            {
                "call_on_ready": True,
                "on_ready": {"ok": True, "address": "t9", "task_sent": True},
                "run": {"ok": True, "launching": True, "address": "t9"},
            },
            {"async_launch": True},
        ),
    ]
    for index, (name, args, herdr_spec, callbacks_spec) in enumerate(cases, start=1):
        _run_case(
            out,
            f"tools_herdr/run/{index:03d}-{name}",
            args,
            herdr_spec,
            callbacks_spec,
        )

    # `_read_output` sin read_full usa read_agent(200).
    from ezagent.agent import tools_herdr as th

    class NoFull:
        def read_agent(self, address: str, lines: int | None = None) -> str:
            return f"read_agent:{address}:{lines}"

    class Broken:
        def read_agent(self, address: str, lines: int | None = None) -> str:
            raise OSError("roto")

    for index, (name, obj) in enumerate([("fallback", NoFull()), ("error", Broken())], start=1):
        _capture(
            out,
            f"tools_herdr/run/read_output/{index:03d}-{name}",
            "_read_output",
            {
                "address": "t1",
                "behavior": name,
                "fallback_text": "read_agent:t1:200" if name == "fallback" else None,
            },
            lambda value=obj: {"text": th._read_output(value, "t1")},
        )


def _voice_cases(out: FixtureFile) -> None:
    state = _state()
    state.last_address = "t1"
    _voice_case(out, "tools_herdr/voice/001-close", {"action": "close"}, {}, state)
    state = _state()
    state.last_address = "t1"
    _voice_case(
        out,
        "tools_herdr/voice/002-close-notify",
        {"action": "close"},
        {"user_text": "avísame"},
        state,
    )
    _voice_case(
        out,
        "tools_herdr/voice/003-close-notify-no-address",
        {"action": "close"},
        {"user_text": "avísame"},
    )
    state = _state()
    state.last_address = "t0"
    _voice_case(
        out,
        "tools_herdr/voice/004-close-and-watch",
        {"action": "close_and_watch", "address": "t1"},
        {},
        state,
    )
    state = _state()
    state.last_address = "t0"
    _voice_case(
        out,
        "tools_herdr/voice/005-close-and-watch-fallback",
        {"action": "close_and_watch"},
        {},
        state,
    )
    _voice_case(
        out,
        "tools_herdr/voice/006-close-and-watch-missing",
        {"action": "close_and_watch"},
        {},
    )

    voice_basic = [
        ("mic-true", {"action": "mic_mute", "muted": True}),
        ("mic-false", {"action": "mic_mute", "muted": False}),
        ("mic-missing", {"action": "mic_mute"}),
        ("mic-int", {"action": "mic_mute", "muted": 1}),
        ("repeat", {"action": "repeat_last"}),
        ("mute", {"action": "mute", "minutes": 15}),
        ("mute-one", {"action": "mute", "minutes": 1}),
        ("mute-480", {"action": "mute", "minutes": 480}),
        ("mute-zero", {"action": "mute", "minutes": 0}),
        ("mute-481", {"action": "mute", "minutes": 481}),
        ("mute-bool", {"action": "mute", "minutes": True}),
        ("mute-float", {"action": "mute", "minutes": 1.5}),
        ("mute-str", {"action": "mute", "minutes": "5"}),
        ("mute-missing", {"action": "mute"}),
        ("unknown", {"action": "nada"}),
        ("missing", {}),
    ]
    for index, (name, args) in enumerate(voice_basic, start=1):
        _voice_case(
            out,
            f"tools_herdr/voice/{index + 6:03d}-{name}",
            args,
            {"summary": "el resumen"},
        )

    state = _state()
    state.last_address = "t1"
    _voice_case(
        out,
        "tools_herdr/voice/023-watch-raises",
        {"action": "close"},
        {"user_text": "avísame", "watch_raise": {"raise": "ValueError", "message": "roto"}},
        state,
    )
    _voice_case(
        out,
        "tools_herdr/voice/024-close-raises",
        {"action": "close"},
        {"close_raise": {"raise": "RuntimeError", "message": "roto"}},
    )
    _voice_case(
        out,
        "tools_herdr/voice/025-mic-raises",
        {"action": "mic_mute", "muted": True},
        {"mic_raise": {"raise": "RuntimeError", "message": "roto"}},
    )
    _voice_case(
        out,
        "tools_herdr/voice/026-mute-raises",
        {"action": "mute", "minutes": 5},
        {"mute_raise": {"raise": "RuntimeError", "message": "roto"}},
    )
    _voice_case(
        out,
        "tools_herdr/voice/027-user-raises",
        {"action": "close"},
        {"user_text_raise": {"raise": "ValueError", "message": "roto"}},
    )


# ------------------------------------------------------------------ builder
def _builder_cases(out: FixtureFile) -> None:
    from ezagent.agent import tools_herdr as th

    for index, label in enumerate(["none", "map", "raise"], start=1):
        calls: list[dict[str, Any]] = []
        labels = {"t1": "Claude de EzAgent"} if label == "map" else {}
        herdr = FakeHerdr(
            {
                "labels": labels,
                "label_raises": label == "raise",
                "list": [{"address": "t1"}],
            },
            calls,
        )
        callbacks = _callbacks({}, calls)

        def action(h: Any = herdr, c: Any = callbacks) -> dict[str, Any]:
            specs = th.build_herdr_tools(h, c)  # type: ignore[arg-type]
            described = []
            for spec in specs:
                for args in (
                    {"action": "run", "agent": "claude", "task": "x"},
                    {"action": "kill", "address": "t1"},
                    {"action": "close_and_watch", "address": "t1"},
                ):
                    info = spec.describe(args)
                    described.append(
                        {
                            "name": spec.name,
                            "args": args,
                            "info": asdict(info),
                        }
                    )
            return {
                "specs": [
                    {
                        "name": spec.name,
                        "description": spec.description,
                        "parameters": spec.parameters,
                        "level": spec.level,
                        "untrusted_output": spec.untrusted_output,
                        "timeout_s": spec.timeout_s,
                        "schema": spec.schema(),
                    }
                    for spec in specs
                ],
                "described": described,
                "handler": (
                    None if label == "raise" else specs[0].handler({"action": "list"}).to_dict()
                ),
            }

        _capture(
            out,
            f"tools_herdr/builder/{index:03d}-{label}",
            "build_herdr_tools",
            {
                "labels": labels,
                "label_raises": label == "raise",
                "handler": "none" if label == "raise" else "list",
            },
            action,
        )

    # El handler del builder conserva los labels.
    calls = []
    herdr = FakeHerdr(
        {"labels": {"t1": "Claude de EzAgent"}, "list": [{"address": "t1", "name": "Crudo"}]},
        calls,
    )
    callbacks = _callbacks({}, calls)
    _capture(
        out,
        "tools_herdr/builder/004-handler-labels",
        "build_herdr_tools",
        {"labels": "map", "handler": "list"},
        lambda: {
            "result": th.build_herdr_tools(herdr, callbacks)[0]  # type: ignore[arg-type]
            .handler({"action": "list"})
            .to_dict()
        },
    )

    # Un ToolResult inesperado se devuelve igual.
    _capture(
        out,
        "tools_herdr/builder/005-handler-error",
        "build_herdr_tools",
        {"handler": "delegate error"},
        lambda: {
            "result": th.build_herdr_tools(herdr, callbacks)[0]  # type: ignore[arg-type]
            .handler({"action": "nada"})
            .to_dict()
        },
    )


def generate(out: FixtureFile) -> None:
    _argument_cases(out)
    _summary_cases(out)
    _label_cases(out)
    _delegate_cases(out)
    _run_cases(out)
    _voice_cases(out)
    _builder_cases(out)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)

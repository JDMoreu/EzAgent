"""Pruebas del adaptador Herdr del núcleo agéntico (F7).

Usa un Herdr falso: no abre ventanas, no lanza procesos y no toca el disco.
Si el núcleo (`ezagent.agent.registry`) aún no existe, el módulo se omite.
"""

from __future__ import annotations

from typing import Any

import pytest

pytest.importorskip("ezagent.agent.registry")

from ezagent.agent.registry import ToolResult  # noqa: E402
from ezagent.agent.tools_herdr import (  # noqa: E402
    TASK_PREFIX,
    HerdrCallbacks,
    build_herdr_tools,
)
from ezagent.voice.herdr import HerdrError  # noqa: E402

DELEGATE_KEYS = {
    "action",
    "address",
    "agent",
    "task",
    "cwd",
    "name",
    "background",
    "gui",
    "notify_on_done",
    "close_listening",
    "space",
    "shell",
    "lines",
    "model",
    "effort",
    "input_from",
    "then",
    "keys",
    "deliver_answer",
}
VOICE_KEYS = {"action", "address", "muted", "minutes"}


class FakeHerdr:
    """Doble de Herdr que solo registra las llamadas."""

    def __init__(self) -> None:
        self.calls: list[tuple[str, tuple[Any, ...], dict[str, Any]]] = []
        self.agents: list[dict[str, Any]] = [
            {"address": "t1", "agent": "claude", "state": "idle"}
        ]
        self.agent: dict[str, Any] = {
            "address": "t1",
            "agent": "claude",
            "state": "idle",
            "cwd": "C:/tmp",
        }
        self.buffer = "salida reciente del agente"
        self.run_result: dict[str, Any] = {
            "ok": True,
            "address": "t7",
            "terminal": "t7",
            "agent": "claude",
            "visible": True,
            "background": False,
            "task_sent": True,
        }
        self.send_result: dict[str, Any] = {
            "ok": True,
            "address": "t1",
            "message": "Mensaje enviado.",
        }
        self.open_result: dict[str, Any] = {
            "ok": True,
            "terminal": "t9",
            "address": "t9",
            "visible": True,
        }
        self.fail_with: Exception | None = None

    def _record(
        self, tool: str, *args: Any, **kwargs: Any
    ) -> None:
        self.calls.append((tool, args, kwargs))
        if self.fail_with is not None:
            raise self.fail_with

    def calls_of(
        self, tool: str
    ) -> list[tuple[str, tuple[Any, ...], dict[str, Any]]]:
        return [call for call in self.calls if call[0] == tool]

    # ------------------------------------------------------------- lectura
    def list_agents(self) -> list[dict[str, Any]]:
        self._record("list_agents")
        return self.agents

    def get_agent(self, address: str) -> dict[str, Any]:
        self._record("get_agent", address)
        return {**self.agent, "address": address}

    def read_agent(self, address: str, lines: int | None = None) -> str:
        self._record("read_agent", address, lines)
        return self.buffer

    # ------------------------------------------------------------ acciones
    def send_agent(
        self, address: str, text: str, confirmed: bool = False
    ) -> dict[str, Any]:
        self._record("send_agent", address, text, confirmed=confirmed)
        return self.send_result

    def open_terminal(
        self,
        name: str,
        cwd: str | None = None,
        shell: str = "powershell.exe",
        space: str | None = None,
    ) -> dict[str, Any]:
        self._record(
            "open_terminal", name, cwd=cwd, shell=shell, space=space
        )
        return self.open_result

    def run_agent(
        self,
        agent: str,
        task: str,
        cwd: str | None = None,
        name: str | None = None,
        confirmed: bool = False,
        background: bool = False,
        gui: bool = False,
        space: str | None = None,
    ) -> dict[str, Any]:
        self._record(
            "run_agent",
            agent,
            task,
            cwd=cwd,
            name=name,
            confirmed=confirmed,
            background=background,
            gui=gui,
            space=space,
        )
        return self.run_result

    def focus_agent(self, address: str) -> dict[str, Any]:
        self._record("focus_agent", address)
        return {"ok": True, "address": address, "focused": True}

    def kill_agent(
        self, address: str, confirmed: bool = False
    ) -> dict[str, Any]:
        self._record("kill_agent", address, confirmed=confirmed)
        return {
            "ok": True,
            "address": address,
            "message": "Terminal terminado.",
        }


class FakeCallbacks:
    """Doble de los callbacks de la aplicación de voz."""

    def __init__(self, user_text: str = "") -> None:
        self.watched: list[str] = []
        self.closed = 0
        self.mic: list[bool] = []
        self.muted: list[int] = []
        self.summary = "Último resumen hablado."
        self.user_text = user_text

    def watch(self, address: str) -> None:
        self.watched.append(address)

    def request_close(self) -> None:
        self.closed += 1

    def mic_mute(self, muted: bool) -> None:
        self.mic.append(muted)

    def last_summary(self) -> str:
        return self.summary

    def mute(self, minutes: int) -> None:
        self.muted.append(minutes)

    def get_last_user_text(self) -> str:
        return self.user_text

    def as_callbacks(self) -> HerdrCallbacks:
        return HerdrCallbacks(
            watch=self.watch,
            request_close=self.request_close,
            mic_mute=self.mic_mute,
            last_summary=self.last_summary,
            mute=self.mute,
            get_last_user_text=self.get_last_user_text,
        )


@pytest.fixture
def env() -> tuple[FakeHerdr, FakeCallbacks, dict[str, Any]]:
    herdr = FakeHerdr()
    callbacks = FakeCallbacks()
    specs = {
        spec.name: spec
        for spec in build_herdr_tools(herdr, callbacks.as_callbacks())
    }
    return herdr, callbacks, specs


def _call(spec: Any, args: dict[str, Any]) -> ToolResult:
    result = spec.handler(args)
    assert isinstance(result, ToolResult)
    return result


# ------------------------------------------------------------------ builder
def test_builds_exactly_two_tools(env: tuple) -> None:
    _, _, tools = env
    assert set(tools) == {"delegate_to_agent", "voice_control"}


def test_delegate_schema_has_requested_parameters(env: tuple) -> None:
    _, _, tools = env
    properties = tools["delegate_to_agent"].parameters["properties"]
    assert set(properties) == DELEGATE_KEYS
    assert tools["delegate_to_agent"].parameters["required"] == ["action"]
    assert set(properties["action"]["enum"]) == {
        "list",
        "get",
        "read",
        "send",
        "run",
        "open_terminal",
        "focus",
        "kill",
        "watch",
        "agents",
        "configure",
        "keys",
        "guide",
    }
    assert properties["lines"]["minimum"] == 10
    assert properties["lines"]["maximum"] == 120


def test_voice_schema_has_requested_parameters(env: tuple) -> None:
    _, _, tools = env
    properties = tools["voice_control"].parameters["properties"]
    assert set(properties) == VOICE_KEYS
    assert tools["voice_control"].parameters["required"] == ["action"]
    assert set(properties["action"]["enum"]) == {
        "close",
        "close_and_watch",
        "mic_mute",
        "repeat_last",
        "mute",
    }
    assert properties["minutes"]["minimum"] == 1
    assert properties["minutes"]["maximum"] == 480


def test_delegate_levels_match_contract(env: tuple) -> None:
    _, _, tools = env
    spec = tools["delegate_to_agent"]
    expected = {
        "list": 0,
        "get": 0,
        "read": 0,
        "focus": 0,
        "watch": 0,
        "send": 1,
        "run": 1,
        "open_terminal": 1,
        "agents": 0,
        "configure": 1,
        "keys": 1,
        "guide": 0,
        "kill": 2,
    }
    for action, level in expected.items():
        info = spec.describe({"action": action})
        assert info.level == level, action
        assert info.always_confirm == (action == "kill"), action


def test_voice_control_levels_are_one(env: tuple) -> None:
    _, _, tools = env
    spec = tools["voice_control"]
    for action in ("close", "close_and_watch", "mic_mute", "repeat_last", "mute"):
        info = spec.describe({"action": action})
        assert info.level == 1, action
        assert info.always_confirm is False


def test_unknown_action_describe_is_conservative(env: tuple) -> None:
    _, _, tools = env
    delegate = tools["delegate_to_agent"].describe({"action": "destroy"})
    assert delegate.level == 2
    assert delegate.always_confirm is True
    assert "desconocida" in delegate.summary.lower()
    assert tools["voice_control"].describe({}).level == 1


def test_read_marks_the_tool_as_untrusted(env: tuple) -> None:
    _, _, tools = env
    assert tools["delegate_to_agent"].untrusted_output is True
    assert tools["voice_control"].untrusted_output is False


# ----------------------------------------------------------------- summaries
def test_summaries_include_action_agent_address_and_task(env: tuple) -> None:
    _, _, tools = env
    spec = tools["delegate_to_agent"]

    run = spec.describe(
        {"action": "run", "agent": "claude", "task": "arregla el login"}
    )
    assert "Lanzar" in run.summary
    assert "claude" in run.summary
    assert "arregla el login" in run.summary
    assert "visible" in run.summary

    background = spec.describe(
        {
            "action": "run",
            "agent": "codex",
            "task": "investiga",
            "background": True,
        }
    )
    assert "segundo plano" in background.summary

    send = spec.describe(
        {"action": "send", "address": "t3", "task": "revisa los tests"}
    )
    assert "t3" in send.summary
    assert "revisa los tests" in send.summary

    kill = spec.describe({"action": "kill", "address": "t5"})
    assert "Terminar" in kill.summary
    assert "t5" in kill.summary

    terminal = spec.describe(
        {"action": "open_terminal", "name": "PowerShell"}
    )
    assert "PowerShell" in terminal.summary

    read = spec.describe({"action": "read", "address": "t2", "lines": 40})
    assert "40" in read.summary
    assert "t2" in read.summary

    watch = spec.describe({"action": "watch", "address": "t8"})
    assert "t8" in watch.summary
    assert "avisar" in watch.summary


def test_voice_summaries_are_generated_from_arguments(env: tuple) -> None:
    _, _, tools = env
    spec = tools["voice_control"]

    close_and_watch = spec.describe(
        {"action": "close_and_watch", "address": "t4"}
    )
    assert "t4" in close_and_watch.summary
    assert "Cerrar" in close_and_watch.summary

    assert "Silenciar" in spec.describe(
        {"action": "mic_mute", "muted": True}
    ).summary
    assert "Reactivar" in spec.describe(
        {"action": "mic_mute", "muted": False}
    ).summary
    assert "30 minutos" in spec.describe(
        {"action": "mute", "minutes": 30}
    ).summary
    assert "resumen" in spec.describe(
        {"action": "repeat_last"}
    ).summary.lower()


# --------------------------------------------------------------- delegate
def test_task_prefix_is_exact() -> None:
    assert TASK_PREFIX == "Tarea del usuario (por voz): "


def test_list_returns_agents_and_calls_herdr(env: tuple) -> None:
    herdr, callbacks, tools = env
    result = _call(tools["delegate_to_agent"], {"action": "list"})
    assert result.ok is True
    assert result.data == herdr.agents
    assert len(herdr.calls_of("list_agents")) == 1
    assert callbacks.watched == []
    assert callbacks.closed == 0


def test_get_uses_address(env: tuple) -> None:
    herdr, _, tools = env
    result = _call(
        tools["delegate_to_agent"],
        {"action": "get", "address": "t2"},
    )
    assert result.ok is True
    assert result.data["address"] == "t2"
    assert herdr.calls_of("get_agent")[0][1] == ("t2",)


def test_read_passes_lines_and_returns_text(env: tuple) -> None:
    herdr, _, tools = env
    spec = tools["delegate_to_agent"]
    result = _call(
        spec, {"action": "read", "address": "t1", "lines": 40}
    )
    assert result.ok is True
    assert result.data == {
        "address": "t1",
        "text": "salida reciente del agente",
    }
    assert herdr.calls_of("read_agent")[0][1] == ("t1", 40)
    # La herramienta completa queda marcada como no confiable.
    assert spec.untrusted_output is True


def test_read_uses_default_lines_when_omitted(env: tuple) -> None:
    herdr, _, tools = env
    _call(tools["delegate_to_agent"], {"action": "read", "address": "t1"})
    assert herdr.calls_of("read_agent")[0][1] == ("t1", None)


@pytest.mark.parametrize("lines", [5, 121, "40", True, 4.5])
def test_read_rejects_invalid_lines(env: tuple, lines: Any) -> None:
    herdr, _, tools = env
    result = _call(
        tools["delegate_to_agent"],
        {"action": "read", "address": "t1", "lines": lines},
    )
    assert result.ok is False
    assert "lines" in (result.error or "")
    assert herdr.calls_of("read_agent") == []


def test_send_adds_voice_prefix_and_confirms(env: tuple) -> None:
    herdr, _, tools = env
    result = _call(
        tools["delegate_to_agent"],
        {"action": "send", "address": "t1", "task": "arregla el bug"},
    )
    assert result.ok is True
    name, args, kwargs = herdr.calls[-1]
    assert name == "send_agent"
    assert args == ("t1", "Tarea del usuario (por voz): arregla el bug")
    assert kwargs == {"confirmed": True}


def test_send_wraps_herdr_failure(env: tuple) -> None:
    herdr, _, tools = env
    herdr.send_result = {"ok": False, "error": "El terminal no existe."}
    result = _call(
        tools["delegate_to_agent"],
        {"action": "send", "address": "t99", "task": "hola"},
    )
    assert result.ok is False
    assert "no existe" in (result.error or "")


def test_run_is_confirmed_updates_state_and_notifies(env: tuple) -> None:
    herdr, callbacks, tools = env
    result = _call(
        tools["delegate_to_agent"],
        {
            "action": "run",
            "agent": "claude",
            "task": "haz la tarea",
            "notify_on_done": True,
            "close_listening": True,
        },
    )
    assert result.ok is True
    assert result.data["address"] == "t7"

    name, args, kwargs = herdr.calls_of("run_agent")[-1]
    assert name == "run_agent"
    assert args == ("claude", "haz la tarea")
    assert kwargs["confirmed"] is True
    assert kwargs["background"] is False
    assert kwargs["gui"] is False

    assert callbacks.watched == ["t7"]
    assert callbacks.closed == 1

    # El estado interno quedó en t7: close_and_watch sin address lo usa.
    voice = _call(tools["voice_control"], {"action": "close_and_watch"})
    assert voice.ok is True
    assert callbacks.watched[-1] == "t7"
    assert callbacks.closed == 2


def test_run_detects_notify_and_close_from_user_text(env: tuple) -> None:
    herdr, callbacks, tools = env
    callbacks.user_text = "avísame cuando termine y ciérrate"
    result = _call(
        tools["delegate_to_agent"],
        {"action": "run", "agent": "codex", "task": "investiga"},
    )
    assert result.ok is True
    assert callbacks.watched == ["t7"]
    assert callbacks.closed == 1


def test_run_without_user_intent_does_not_notify(env: tuple) -> None:
    _, callbacks, tools = env
    result = _call(
        tools["delegate_to_agent"],
        {"action": "run", "agent": "claude", "task": "tarea"},
    )
    assert result.ok is True
    assert callbacks.watched == []
    assert callbacks.closed == 0


def test_run_failure_does_not_update_state_or_notify(env: tuple) -> None:
    herdr, callbacks, tools = env
    herdr.run_result = {
        "ok": False,
        "error": "Agente desconocido: 'foo'.",
    }
    result = _call(
        tools["delegate_to_agent"],
        {
            "action": "run",
            "agent": "foo",
            "task": "tarea",
            "notify_on_done": True,
            "close_listening": True,
        },
    )
    assert result.ok is False
    assert "desconocido" in (result.error or "").lower()
    assert callbacks.watched == []
    assert callbacks.closed == 0
    # No hay último agente: close_and_watch debe fallar sin cerrar.
    voice = _call(tools["voice_control"], {"action": "close_and_watch"})
    assert voice.ok is False
    assert callbacks.closed == 0


def test_open_terminal_defaults_and_overrides(env: tuple) -> None:
    herdr, _, tools = env
    spec = tools["delegate_to_agent"]

    result = _call(spec, {"action": "open_terminal", "name": "shell"})
    assert result.ok is True
    _, _, kwargs = herdr.calls_of("open_terminal")[-1]
    assert kwargs == {"cwd": None, "shell": "powershell.exe", "space": None}

    _call(
        spec,
        {
            "action": "open_terminal",
            "name": "dev",
            "cwd": "C:/work",
            "shell": "pwsh.exe",
            "space": "principal",
        },
    )
    _, _, kwargs = herdr.calls_of("open_terminal")[-1]
    assert kwargs == {
        "cwd": "C:/work",
        "shell": "pwsh.exe",
        "space": "principal",
    }


def test_focus_calls_herdr(env: tuple) -> None:
    herdr, _, tools = env
    result = _call(
        tools["delegate_to_agent"],
        {"action": "focus", "address": "t2"},
    )
    assert result.ok is True
    assert herdr.calls_of("focus_agent")[0][1] == ("t2",)


def test_kill_is_confirmed_and_forgets_last_agent(env: tuple) -> None:
    herdr, callbacks, tools = env
    spec = tools["delegate_to_agent"]

    _call(spec, {"action": "run", "agent": "claude", "task": "tarea"})
    result = _call(spec, {"action": "kill", "address": "t7"})

    assert result.ok is True
    _, args, kwargs = herdr.calls_of("kill_agent")[-1]
    assert args == ("t7",)
    assert kwargs == {"confirmed": True}

    # Ya no queda último agente al que vigilar.
    voice = _call(tools["voice_control"], {"action": "close_and_watch"})
    assert voice.ok is False
    assert callbacks.closed == 0


def test_watch_updates_last_agent_without_closing(env: tuple) -> None:
    herdr, callbacks, tools = env
    result = _call(
        tools["delegate_to_agent"],
        {"action": "watch", "address": "t4"},
    )
    assert result.ok is True
    assert result.data["watching"] is True
    assert callbacks.watched == ["t4"]
    assert callbacks.closed == 0
    assert herdr.calls == []

    # close con «avísame» vigila el último agente y cierra.
    callbacks.user_text = "avísame cuando acabe"
    voice = _call(tools["voice_control"], {"action": "close"})
    assert voice.ok is True
    assert callbacks.watched == ["t4", "t4"]
    assert callbacks.closed == 1


def test_invalid_delegate_action_returns_spanish_error(env: tuple) -> None:
    herdr, _, tools = env
    result = _call(
        tools["delegate_to_agent"], {"action": "destroy"}
    )
    assert result.ok is False
    assert "Acción desconocida" in (result.error or "")
    assert "destroy" in (result.error or "")
    assert herdr.calls == []


def test_delegate_requires_action(env: tuple) -> None:
    _, _, tools = env
    result = _call(tools["delegate_to_agent"], {})
    assert result.ok is False
    assert "Falta la acción" in (result.error or "")


@pytest.mark.parametrize(
    ("action", "args"),
    [
        ("get", {}),
        ("read", {}),
        ("send", {"address": "t1"}),
        ("send", {"task": "hola"}),
        ("run", {"task": "hola"}),
        ("open_terminal", {}),
        ("focus", {}),
        ("kill", {}),
        ("watch", {}),
    ],
)
def test_delegate_validates_required_parameters(
    env: tuple, action: str, args: dict[str, Any]
) -> None:
    herdr, _, tools = env
    result = _call(
        tools["delegate_to_agent"], {"action": action, **args}
    )
    assert result.ok is False
    assert "falta el parámetro" in (result.error or "").lower()
    assert herdr.calls == []


def test_delegate_rejects_wrong_boolean_type(env: tuple) -> None:
    herdr, _, tools = env
    result = _call(
        tools["delegate_to_agent"],
        {
            "action": "run",
            "agent": "claude",
            "task": "tarea",
            "background": "sí",
        },
    )
    assert result.ok is False
    assert "background" in (result.error or "")
    assert herdr.calls == []


def test_delegate_wraps_herdr_error(env: tuple) -> None:
    herdr, _, tools = env
    herdr.fail_with = HerdrError("hc no está disponible")
    result = _call(tools["delegate_to_agent"], {"action": "list"})
    assert result.ok is False
    assert "hc no está disponible" in (result.error or "")


def test_delegate_wraps_unexpected_exception(env: tuple) -> None:
    herdr, _, tools = env
    herdr.fail_with = RuntimeError("boom")
    result = _call(
        tools["delegate_to_agent"],
        {"action": "focus", "address": "t1"},
    )
    assert result.ok is False
    assert "Error inesperado" in (result.error or "")
    assert "boom" in (result.error or "")


# --------------------------------------------------------------------- voz
def test_voice_close_without_notify_just_closes(env: tuple) -> None:
    _, callbacks, tools = env
    result = _call(tools["voice_control"], {"action": "close"})
    assert result.ok is True
    assert callbacks.watched == []
    assert callbacks.closed == 1


def test_voice_close_with_notify_watches_last_agent(env: tuple) -> None:
    _, callbacks, tools = env
    _call(
        tools["delegate_to_agent"],
        {"action": "watch", "address": "t6"},
    )
    callbacks.user_text = "ciérrate y avísame"
    result = _call(tools["voice_control"], {"action": "close"})
    assert result.ok is True
    assert callbacks.watched == ["t6", "t6"]
    assert callbacks.closed == 1


def test_voice_close_and_watch_with_address(env: tuple) -> None:
    _, callbacks, tools = env
    result = _call(
        tools["voice_control"],
        {"action": "close_and_watch", "address": "t8"},
    )
    assert result.ok is True
    assert result.data["address"] == "t8"
    assert callbacks.watched == ["t8"]
    assert callbacks.closed == 1


def test_voice_close_and_watch_without_agent_fails_cleanly(
    env: tuple,
) -> None:
    _, callbacks, tools = env
    result = _call(tools["voice_control"], {"action": "close_and_watch"})
    assert result.ok is False
    assert "agente" in (result.error or "").lower()
    assert callbacks.watched == []
    assert callbacks.closed == 0


def test_voice_close_and_watch_reuses_last_agent(env: tuple) -> None:
    _, callbacks, tools = env
    _call(
        tools["delegate_to_agent"],
        {"action": "run", "agent": "claude", "task": "tarea"},
    )
    result = _call(tools["voice_control"], {"action": "close_and_watch"})
    assert result.ok is True
    assert result.data["address"] == "t7"
    assert callbacks.watched == ["t7"]
    assert callbacks.closed == 1


def test_voice_mic_mute_calls_callback(env: tuple) -> None:
    _, callbacks, tools = env
    first = _call(
        tools["voice_control"], {"action": "mic_mute", "muted": True}
    )
    second = _call(
        tools["voice_control"], {"action": "mic_mute", "muted": False}
    )
    assert first.ok is True and second.ok is True
    assert callbacks.mic == [True, False]
    assert first.data["message"] == "Micrófono silenciado."
    assert second.data["message"] == "Micrófono activo."


def test_voice_repeat_last_returns_summary(env: tuple) -> None:
    _, callbacks, tools = env
    result = _call(tools["voice_control"], {"action": "repeat_last"})
    assert result.ok is True
    assert result.data == {"text": callbacks.summary}


def test_voice_mute_calls_callback(env: tuple) -> None:
    _, callbacks, tools = env
    result = _call(
        tools["voice_control"], {"action": "mute", "minutes": 30}
    )
    assert result.ok is True
    assert callbacks.muted == [30]
    assert "30" in result.data["message"]


@pytest.mark.parametrize(
    ("args", "expected"),
    [
        ({"action": "mic_mute"}, "muted"),
        ({"action": "mic_mute", "muted": "sí"}, "muted"),
        ({"action": "mute"}, "minutes"),
        ({"action": "mute", "minutes": 0}, "minutes"),
        ({"action": "mute", "minutes": 481}, "minutes"),
        ({"action": "mute", "minutes": True}, "minutes"),
    ],
)
def test_voice_validates_parameters(
    env: tuple, args: dict[str, Any], expected: str
) -> None:
    _, callbacks, tools = env
    result = _call(tools["voice_control"], args)
    assert result.ok is False
    assert expected in (result.error or "")
    assert callbacks.mic == []
    assert callbacks.muted == []


def test_voice_invalid_action_returns_spanish_error(env: tuple) -> None:
    _, callbacks, tools = env
    result = _call(tools["voice_control"], {"action": "explode"})
    assert result.ok is False
    assert "Acción desconocida" in (result.error or "")
    assert callbacks.closed == 0


def test_voice_control_does_not_call_herdr(env: tuple) -> None:
    herdr, _, tools = env
    _call(
        tools["voice_control"],
        {"action": "close_and_watch", "address": "t3"},
    )
    _call(tools["voice_control"], {"action": "mic_mute", "muted": True})
    _call(tools["voice_control"], {"action": "repeat_last"})
    _call(tools["voice_control"], {"action": "mute", "minutes": 5})
    assert herdr.calls == []

"""Integración del núcleo agéntico con el mensajero y la isla.

No usa red, micrófono, Tk ni ventanas Win32: el cliente OpenAI, Herdr y la
confirmación se sustituyen por dobles en memoria.
"""

from __future__ import annotations

import json
from types import SimpleNamespace

import pytest

from ezagent.agent.runtime import (
    AgentRuntime,
    RuntimeCallbacks,
)
from ezagent.voice.config import VoiceConfig
from ezagent.voice.llm import ACK_FILLERS, Messenger


class _FakeHerdr:
    def list_agents(self):
        return []


class _FakeStream:
    def __init__(self, chunks):
        self._chunks = list(chunks)

    def __iter__(self):
        return iter(self._chunks)

    def close(self):
        return None


class _FakeCompletions:
    def __init__(self, responses):
        self._responses = list(responses)
        self.requests: list[dict] = []

    def create(self, **kwargs):
        self.requests.append(kwargs)
        return _FakeStream(self._responses.pop(0))


class _FakeClient:
    def __init__(self, responses):
        self.chat = SimpleNamespace(completions=_FakeCompletions(responses))


def _chunk(content: str = "", tool_calls=None):
    delta = SimpleNamespace(content=content, tool_calls=tool_calls or [])
    return SimpleNamespace(choices=[SimpleNamespace(delta=delta)])


def _tool_chunk(name: str, arguments: dict, call_id: str = "call_1"):
    call = SimpleNamespace(
        index=0,
        id=call_id,
        function=SimpleNamespace(
            name=name, arguments=json.dumps(arguments, ensure_ascii=False)
        ),
    )
    return _chunk(tool_calls=[call])


class _FakeConfirm:
    def __init__(self, answer: bool = True):
        self.answer = answer
        self.calls: list[tuple[str, bool, int | None]] = []
        self._pending = False

    @property
    def pending(self) -> bool:
        return self._pending

    def ask(self, summary, *, strong=False, require_number=None):
        self.calls.append((summary, strong, require_number))
        return self.answer

    def submit_text(self, text):
        return None

    def submit_click(self, yes):
        return None

    def cancel(self):
        return None


@pytest.fixture()
def runtime(tmp_path):
    config_path = tmp_path / "permissions.toml"
    rt = AgentRuntime(
        VoiceConfig(),
        _FakeHerdr(),
        RuntimeCallbacks(),
        config_path=config_path,
    )
    rt.config.paths.read_roots = [str(tmp_path)]
    rt.config.paths.write_roots = [str(tmp_path)]
    # tmp_path vive bajo AppData; en producción AppData está en deny. Para la
    # prueba aislada se limpia la lista y se mantiene el workspace temporal.
    rt.config.paths.deny = []
    rt.config.paths.deny_globs = []
    rt.config.workspace = tmp_path
    rt._rebuild_tools()
    yield rt
    rt.close()


def test_runtime_expone_14_herramientas_y_selecciona_por_intencion(runtime):
    assert len(runtime.registry.names()) >= 18
    assert len(runtime.tools_for("hola")) <= 14
    assert "voice_control" in {
        item["function"]["name"] for item in runtime.tools_for("hola")
    }
    time_tools = runtime.tools_for("¿qué hora es?")
    assert len(time_tools) <= 14
    assert "get_datetime" in {
        item["function"]["name"] for item in time_tools
    }
    copy_tools = runtime.tools_for("copia este archivo")
    assert "copy_path" in {
        item["function"]["name"] for item in copy_tools
    }
    open_tools = runtime.tools_for("abre el archivo")
    assert "open_path" in {
        item["function"]["name"] for item in open_tools
    }


def test_runtime_ejecuta_una_herramienta_y_audita(runtime):
    result = runtime.execute("get_datetime", {})
    assert result.ok
    line = (runtime.config.audit_path).read_text(encoding="utf-8").splitlines()[-1]
    entry = json.loads(line)
    assert entry["tool"] == "get_datetime"
    assert entry["decision"] == "allow"
    assert entry["result_ok"] is True


def test_st23_el_runtime_limita_las_herramientas_por_turno(runtime):
    runtime.begin_turn("hola")
    runtime.config.general.max_tool_calls_per_turn = 2  # begin_turn recarga la config
    assert runtime.execute("get_datetime", {}).ok
    assert runtime.execute("get_datetime", {}).ok
    blocked = runtime.execute("get_datetime", {})
    assert not blocked.ok
    assert "límite" in (blocked.error or "").lower()
    runtime.begin_turn("otro turno")
    runtime.config.general.max_tool_calls_per_turn = 2
    assert runtime.execute("get_datetime", {}).ok


def test_taint_obliga_a_confirmar_una_escritura(runtime, tmp_path):
    source = tmp_path / "nota.txt"
    source.write_text("hola", encoding="utf-8")
    read = runtime.execute("read_file", {"path": str(source)})
    assert read.ok
    assert runtime.tainted is True

    confirm = _FakeConfirm(answer=False)
    runtime.confirm = confirm
    target = tmp_path / "salida.txt"
    result = runtime.execute(
        "write_file", {"path": str(target), "content": "x"}
    )
    assert not result.ok
    assert confirm.calls
    assert "canceló" in (result.error or "").lower()


def test_delegate_no_acepta_una_carpeta_fuera_de_las_raices(runtime, tmp_path):
    result = runtime.execute(
        "delegate_to_agent",
        {
            "action": "run",
            "agent": "claude",
            "task": "x",
            "cwd": str(tmp_path.parent),
        },
    )
    assert not result.ok
    assert "no permitida" in (result.error or "").lower()


def test_delegate_valida_tambien_la_carpeta_por_defecto(runtime, tmp_path):
    runtime.voice_config.default_cwd = str(tmp_path.parent)
    result = runtime.execute(
        "delegate_to_agent",
        {"action": "run", "agent": "claude", "task": "x"},
    )
    assert not result.ok
    assert "no permitida" in (result.error or "").lower()


def test_confirmacion_fuerte_incluye_el_numero(runtime, tmp_path):
    target = tmp_path / "carpeta"
    target.mkdir()
    for index in range(6):
        (target / f"{index}.txt").write_text("x", encoding="utf-8")
    confirm = _FakeConfirm(answer=True)
    runtime.confirm = confirm
    result = runtime.execute("delete_path", {"path": str(target)})
    assert result.ok
    assert confirm.calls
    summary, strong, number = confirm.calls[0]
    assert strong is True
    assert number >= 6
    assert "6" in summary or "siete" in summary or "7" in summary


def test_remember_propone_y_guarda_tras_confirmar(runtime):
    confirm = _FakeConfirm(answer=True)
    runtime.confirm = confirm
    result = runtime.execute(
        "remember",
        {"kind": "memory", "text": "prefiero respuestas cortas"},
    )
    assert result.ok
    assert confirm.calls
    assert "prefiero respuestas cortas" in runtime.memory.memory_block()
    recall = runtime.execute("recall", {"query": "respuestas cortas"})
    assert recall.ok


def test_mensajero_usa_el_runtime_y_conserva_el_streaming(runtime):
    first = [_tool_chunk("get_datetime", {})]
    second = [_chunk(" Son las diez.")]
    messenger = Messenger(
        VoiceConfig(llm_model="fake"),
        _FakeHerdr(),
        last_summary=lambda: "",
        watch=lambda address: None,
        mute=lambda minutes: None,
        request_close=lambda: None,
        mic_mute=lambda muted: None,
        runtime=runtime,
    )
    messenger.client = _FakeClient([first, second])
    answer = "".join(messenger.ask_stream("¿qué hora es?")).strip()
    # El modelo pidió la herramienta sin decir nada: el código acusa recibo.
    assert answer.startswith(ACK_FILLERS[0])
    assert answer.endswith("Son las diez.")
    assert len(messenger.client.chat.completions.requests) == 2
    first_request = messenger.client.chat.completions.requests[0]
    assert len(first_request["tools"]) <= 14
    names = {
        item["function"]["name"] for item in first_request["tools"]
    }
    assert "get_datetime" in names
    assert any(
        message.get("role") == "tool"
        for message in messenger.messages
    )
    assert runtime.memory.count_messages() >= 2


def test_mensajero_recorta_historial_por_grupos():
    messenger = Messenger.__new__(Messenger)
    messenger._last_user_text = ""
    messenger._last_agent_address = None
    messenger.runtime = None
    system = {"role": "system", "content": "sys"}
    messages = [system]
    for turn in range(10):
        messages.append({"role": "user", "content": f"u{turn}"})
        messages.append(
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [{"id": f"c{turn}"}],
            }
        )
        messages.append(
            {"role": "tool", "tool_call_id": f"c{turn}", "content": "{}"}
        )
        messages.append({"role": "assistant", "content": f"a{turn}"})
    messenger.messages = messages
    messenger._trim_history()
    assert messenger.messages[0] == system
    assert messenger.messages[1]["role"] == "user"
    assert len(messenger.messages) <= 14


def _messenger(runtime) -> Messenger:
    return Messenger(
        VoiceConfig(llm_model="fake"),
        _FakeHerdr(),
        last_summary=lambda: "",
        watch=lambda address: None,
        mute=lambda minutes: None,
        request_close=lambda: None,
        mic_mute=lambda muted: None,
        runtime=runtime,
    )


def test_sin_acuse_del_codigo_si_el_modelo_ya_lo_dijo(runtime):
    first = [_chunk("Va, lo reviso."), _tool_chunk("get_datetime", {})]
    second = [_chunk("Son las diez.")]
    messenger = _messenger(runtime)
    messenger.client = _FakeClient([first, second])
    answer = "".join(messenger.ask_stream("¿qué hora es?"))
    assert not any(filler in answer for filler in ACK_FILLERS)
    # El acuse del modelo y la respuesta no quedan pegados.
    assert "Va, lo reviso. Son las diez." in answer
    assert len(messenger.last_rounds) == 2
    assert "get_datetime" in messenger.last_rounds[0]


def test_sin_acuse_para_controles_instantaneos(runtime):
    first = [_tool_chunk("voice_control", {"action": "mute", "minutes": 5})]
    second = [_chunk("Listo.")]
    messenger = _messenger(runtime)
    messenger.client = _FakeClient([first, second])
    answer = "".join(messenger.ask_stream("silencia los avisos"))
    assert not any(filler in answer for filler in ACK_FILLERS)


def test_respuesta_directa_no_lleva_acuse(runtime):
    messenger = _messenger(runtime)
    messenger.client = _FakeClient([[_chunk("Porque orbita la Tierra.")]])
    answer = "".join(messenger.ask_stream("¿por qué la luna es un satélite?"))
    assert answer == "Porque orbita la Tierra."
    assert len(messenger.last_rounds) == 1


def test_errores_del_proveedor_se_dicen_claro():
    import httpx
    import openai

    from ezagent.voice.llm import friendly_llm_error

    request = httpx.Request("POST", "https://api.groq.com/openai/v1/chat/completions")
    limited = openai.RateLimitError(
        "Rate limit reached",
        response=httpx.Response(429, headers={"retry-after": "50"}, request=request),
        body=None,
    )
    assert "Espera unos 50 segundos" in friendly_llm_error(limited, "Groq", "m")
    missing = openai.NotFoundError(
        "model does not exist", response=httpx.Response(404, request=request), body=None
    )
    assert friendly_llm_error(missing, "Groq", "qwen3.8-27b") == (
        "Groq no reconoce el modelo qwen3.8-27b. Revisa el nombre en ajustes."
    )
    # Errores del cliente Anthropic (RuntimeError con el código en el texto).
    anthropic = RuntimeError("La API compatible con Anthropic respondió 401: {}")
    assert "clave" in friendly_llm_error(anthropic, "Minimax", "m")
    assert friendly_llm_error(ValueError("raro"), "Groq", "m") == "Groq: raro"
    too_big = openai.APIStatusError(
        "Request too large ... org_123", response=httpx.Response(413, request=request), body=None
    )
    spoken = friendly_llm_error(too_big, "Groq", "m")
    assert "tu plan de Groq" in spoken and "org_" not in spoken

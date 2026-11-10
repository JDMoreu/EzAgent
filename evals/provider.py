"""Proveedor de promptfoo que ejecuta el código REAL de EzAgent.

Un eval solo vale si prueba lo que el usuario recibe: el system prompt que
arma ``AgentRuntime`` y las herramientas que selecciona ``tools_for``. Por eso
este proveedor no copia el prompt a un YAML: construye un ``Messenger`` real y
deja que él arme la petición.

Modos (``config.mode`` en el YAML):

- ``live``: llama al LLM configurado (``.env`` / ``llm.json`` vía ``resolve_llm``)
  y devuelve el primer paso del modelo: su texto y las herramientas que pide.
  Las herramientas NO se ejecutan; se registran y se responde un resultado
  simulado. El bucle se corta antes de la segunda llamada al modelo.
- ``context``: no llama a ningún modelo; devuelve el prompt y las herramientas
  que verá el modelo. Sirve para el contrato offline, sin clave de API.

Privacidad: todo corre con un perfil de usuario temporal (HOME, APPDATA…), así
que la memoria, las skills y el ``system.md`` reales del usuario nunca viajan
al proveedor del LLM.

Salida (JSON en texto)::

    {"text": "...", "tool_calls": [{"name": "...", "arguments": {...},
     "arguments_valid": true}], "available_tools": ["..."], "model": "..."}
"""

from __future__ import annotations

import atexit
import json
import os
import shutil
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace
from typing import Any

REPO = Path(__file__).resolve().parents[1]
_SANDBOX = Path(tempfile.mkdtemp(prefix="ezagent-evals-"))
atexit.register(shutil.rmtree, _SANDBOX, ignore_errors=True)

# Perfil falso ANTES de importar ezagent: nada del usuario real entra al prompt.
_HOME = _SANDBOX / "home"
_HOME.mkdir()
for _name in ("USERPROFILE", "HOME"):
    os.environ[_name] = str(_HOME)
for _name in ("APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME"):
    os.environ[_name] = str(_SANDBOX / _name.lower())
os.environ.pop("EZ_SYSTEM_PROMPT", None)

sys.path.insert(0, str(REPO / "src"))

from ezagent.agent.registry import ToolResult  # noqa: E402
from ezagent.agent.runtime import AgentRuntime, RuntimeCallbacks  # noqa: E402
from ezagent.voice.config import VoiceConfig  # noqa: E402
from ezagent.voice.herdr import Herdr  # noqa: E402
from ezagent.voice.llm import Messenger  # noqa: E402


class _StopAfterFirstStep(Exception):
    """Corta el bucle del Messenger antes de la segunda llamada al modelo."""


class _RecordingRuntime:
    """Runtime real, salvo ``execute``: registra la llamada y simula el resultado."""

    def __init__(self, inner: AgentRuntime) -> None:
        self._inner = inner
        self.calls: list[tuple[str, str]] = []

    def __getattr__(self, name: str) -> Any:
        return getattr(self._inner, name)

    def execute(self, name: str, raw_arguments: Any) -> ToolResult:
        self.calls.append((name, raw_arguments if isinstance(raw_arguments, str) else json.dumps(raw_arguments)))
        return ToolResult(ok=True, data="(resultado simulado por el eval)")


def _build_runtime(case_dir: Path) -> tuple[VoiceConfig, Herdr, AgentRuntime]:
    voice = VoiceConfig.load()
    herdr = Herdr(voice)
    runtime = AgentRuntime(voice, herdr, RuntimeCallbacks(), config_path=case_dir / "permissions.toml")
    runtime.config.ensure_dirs()
    return voice, herdr, runtime


def _provider_options(options: dict[str, Any] | None) -> dict[str, Any]:
    return dict((options or {}).get("config") or {})


def _configure(voice: VoiceConfig, settings: dict[str, Any]) -> None:
    voice.llm_temperature = float(settings.get("temperature", 0.0))
    if "max_tokens" in settings:
        voice.llm_max_tokens = int(settings["max_tokens"])
    if settings.get("model"):
        voice.llm_model = str(settings["model"])


def _make_messenger(voice: VoiceConfig, herdr: Herdr, runtime: Any) -> Messenger:
    return Messenger(
        voice, herdr,
        last_summary=lambda: "", watch=lambda address: None, mute=lambda minutes: None,
        request_close=lambda: None, mic_mute=lambda muted: None, runtime=runtime,
    )


def _history(raw: Any) -> list[dict[str, Any]]:
    if isinstance(raw, str):
        raw = json.loads(raw)
    return [dict(item) for item in (raw or [])]


def _guard_second_call(messenger: Messenger) -> None:
    original = messenger.client.chat.completions.create
    state = {"calls": 0}

    def guarded(**request: Any) -> Any:
        state["calls"] += 1
        if state["calls"] > 1:
            raise _StopAfterFirstStep
        return original(**request)

    messenger.client = SimpleNamespace(chat=SimpleNamespace(completions=SimpleNamespace(create=guarded)))


def _first_step(messages: list[dict[str, Any]], user: str) -> dict[str, Any]:
    """Primer mensaje del asistente después de la frase del usuario."""
    for index in range(len(messages) - 1, -1, -1):
        if messages[index].get("role") == "user" and messages[index].get("content") == user:
            for message in messages[index + 1 :]:
                if message.get("role") == "assistant":
                    return message
            break
    return {"content": "", "tool_calls": []}


def _parse_calls(message: dict[str, Any]) -> list[dict[str, Any]]:
    calls = []
    for call in message.get("tool_calls") or []:
        function = call.get("function") or {}
        raw = function.get("arguments") or ""
        try:
            arguments = json.loads(raw or "{}")
            valid = isinstance(arguments, dict)
        except ValueError:
            arguments, valid = raw, False
        calls.append({"name": function.get("name", ""), "arguments": arguments, "arguments_valid": valid})
    return calls


def _run_live(user: str, history: list[dict[str, Any]], settings: dict[str, Any]) -> dict[str, Any]:
    case_dir = Path(tempfile.mkdtemp(dir=_SANDBOX))
    voice, herdr, runtime = _build_runtime(case_dir)
    try:
        _configure(voice, settings)
        recorder = _RecordingRuntime(runtime)
        messenger = _make_messenger(voice, herdr, recorder)
        messenger.messages.extend(history)
        _guard_second_call(messenger)
        try:
            for _ in messenger.ask_stream(user):
                pass
        except _StopAfterFirstStep:
            pass
        step = _first_step(messenger.messages, user)
        available = [t["function"]["name"] for t in runtime.tools_for(user)]
        return {
            "text": (step.get("content") or "").strip(),
            "tool_calls": _parse_calls(step),
            "available_tools": available,
            "model": voice.llm_model,
        }
    finally:
        runtime.close()
        shutil.rmtree(case_dir, ignore_errors=True)


def _run_context(user: str) -> dict[str, Any]:
    case_dir = Path(tempfile.mkdtemp(dir=_SANDBOX))
    _, _, runtime = _build_runtime(case_dir)
    try:
        tools = runtime.tools_for(user)
        return {
            "system_prompt": runtime.system_prompt(),
            "tools": tools,
            "available_tools": [t["function"]["name"] for t in tools],
        }
    finally:
        runtime.close()
        shutil.rmtree(case_dir, ignore_errors=True)


def call_api(prompt: str, options: dict[str, Any], context: dict[str, Any]) -> dict[str, Any]:
    """Punto de entrada de promptfoo para los casos de prueba."""
    settings = _provider_options(options)
    variables = (context or {}).get("vars") or {}
    user = str(variables.get("user") or prompt or "")
    try:
        if settings.get("mode") == "context":
            result = _run_context(user)
        else:
            result = _run_live(user, _history(variables.get("history")), settings)
    except Exception as exc:  # noqa: BLE001 - promptfoo debe ver el fallo, no morir
        return {"error": f"{type(exc).__name__}: {exc}"}
    return {"output": json.dumps(result, ensure_ascii=False)}


def call_grader(prompt: str, options: dict[str, Any], context: dict[str, Any]) -> dict[str, Any]:
    """Modelo juez para ``llm-rubric``: el mismo proveedor que usa la app."""
    from ezagent.llm_providers import resolve_llm

    settings = _provider_options(options)
    try:
        resolved = resolve_llm(model=settings.get("model") or None)
        if resolved.api_style == "anthropic":
            from ezagent.voice.anthropic_client import AnthropicChatClient

            client: Any = AnthropicChatClient(
                api_key=resolved.api_key or "sin-clave", base_url=resolved.base_url)
            stream = client.chat.completions.create(
                model=resolved.model, messages=[{"role": "user", "content": prompt}],
                max_tokens=600, temperature=0, stream=True)
            text = "".join(c.choices[0].delta.content or "" for c in stream)
        else:
            import uuid

            from openai import OpenAI

            # Mismas cabeceras que el Messenger real: OpenCode rechaza (400) las
            # peticiones sin ``x-opencode-session``.
            headers = {"User-Agent": "EzAgent-Voice/0.1"}
            if resolved.provider == "opencode":
                headers["x-opencode-session"] = uuid.uuid4().hex
            client = OpenAI(api_key=resolved.api_key or "sin-clave", base_url=resolved.base_url,
                            default_headers=headers, max_retries=2)
            response = client.chat.completions.create(
                model=resolved.model, messages=[{"role": "user", "content": prompt}],
                max_tokens=600, temperature=0)
            text = response.choices[0].message.content or ""
    except Exception as exc:  # noqa: BLE001
        return {"error": f"{type(exc).__name__}: {exc}"}
    return {"output": text}

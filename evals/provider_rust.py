"""Proveedor de promptfoo que ejecuta el binario Rust de EzAgent.

Mismo contrato que ``provider.py`` (la salida también es un JSON en texto), pero
el trabajo lo hace ``ezagent-voice eval``: el ejecutable arma el prompt de
sistema y las herramientas REALES con el código Rust y, en modo ``live``, hace
una sola llamada al modelo configurado sin ejecutar ninguna herramienta.

La petición viaja por stdin y la respuesta sale por stdout::

    {"user": "...", "history": [...], "config": {"temperature": 0, ...}}

    {"text": "...", "tool_calls": [{"name": "...", "arguments": {...},
     "arguments_valid": true}], "available_tools": ["..."], "model": "..."}

El binario se toma de ``EZAGENT_RUST_BIN``; si no está, de
``rust/target/{debug,release}/ezagent-voice[.exe]`` y, por último, del PATH.
"""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parents[1]
BINARY_ENV = "EZAGENT_RUST_BIN"


def _binary() -> str:
    """Ruta del ejecutable Rust, o su nombre si se espera en el PATH."""
    override = os.environ.get(BINARY_ENV)
    if override:
        return override
    for profile in ("debug", "release"):
        for name in ("ezagent-voice.exe", "ezagent-voice"):
            candidate = REPO / "rust" / "target" / profile / name
            if candidate.is_file():
                return str(candidate)
    return "ezagent-voice"


def _provider_options(options: dict[str, Any] | None) -> dict[str, Any]:
    return dict((options or {}).get("config") or {})


def _request(prompt: str, options: dict[str, Any], context: dict[str, Any]) -> dict[str, Any]:
    """Lo que ``provider.py`` recibe como ``prompt``, ``options`` y ``context``."""
    settings = _provider_options(options)
    variables = (context or {}).get("vars") or {}
    request: dict[str, Any] = {
        "user": str(variables.get("user") or prompt or ""),
        "config": settings,
    }
    if "history" in variables:
        request["history"] = variables["history"]
    return request


def _mode(settings: dict[str, Any]) -> str:
    """Mismo criterio que ``call_api``: ``context`` solo si se pide."""
    return "context" if settings.get("mode") == "context" else "live"


def call_api(prompt: str, options: dict[str, Any], context: dict[str, Any]) -> dict[str, Any]:
    """Punto de entrada de promptfoo para los casos de prueba."""
    settings = _provider_options(options)
    request = _request(prompt, options, context)
    command = [_binary(), "eval", "--mode", _mode(settings)]
    try:
        done = subprocess.run(  # noqa: S603 - binario declarado, argumentos en lista
            command,
            input=json.dumps(request, ensure_ascii=False),
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=dict(os.environ),
            check=False,
        )
    except OSError as exc:
        return {"error": f"{type(exc).__name__}: {exc}"}
    if done.returncode != 0:
        return {"error": _failure(done)}
    try:
        result = json.loads(done.stdout)
    except ValueError as exc:
        return {"error": f"JSONDecodeError: {exc}"}
    if isinstance(result, dict) and "error" in result:
        return {"error": str(result["error"])}
    return {"output": json.dumps(result, ensure_ascii=False)}


def _failure(done: subprocess.CompletedProcess[str]) -> str:
    """El binario ya devuelve ``{"error": "Tipo: mensaje"}``; se copia tal cual."""
    detail = (done.stdout or done.stderr or "").strip()
    try:
        data = json.loads(detail)
    except ValueError:
        return f"RuntimeError: {detail}"
    if isinstance(data, dict) and data.get("error"):
        return str(data["error"])
    return f"RuntimeError: {detail}"

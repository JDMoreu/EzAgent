"""Cliente mínimo de la API Messages (Anthropic) con forma de cliente OpenAI.

El ``Messenger`` habla en formato Chat Completions de OpenAI: mensajes con
``tool_calls``, herramientas ``{"type": "function", ...}`` y trozos en streaming
con ``choices[0].delta``. Esta clase traduce la petición al formato Messages,
lee el streaming SSE y devuelve trozos con esa misma forma, así el bucle de
herramientas no cambia. Sirve para Claude y para cualquier servicio
«compatible con Anthropic».

Solo usa ``httpx`` (ya es dependencia). La clave va en ``x-api-key`` y nunca se
registra.
"""

from __future__ import annotations

import json
from collections.abc import Iterator
from threading import Lock
from types import SimpleNamespace
from typing import Any

import httpx

ANTHROPIC_VERSION = "2023-06-01"
STOP_REASONS = {"end_turn": "stop", "max_tokens": "length", "tool_use": "tool_calls"}


def messages_url(base_url: str) -> str:
    """``…/v1`` -> ``…/v1/messages``; una raíz sin versión -> ``…/v1/messages``."""
    base = str(base_url or "").strip().rstrip("/")
    if base.endswith("/v1"):
        return base + "/messages"
    return base + "/v1/messages"


# ------------------------------------------------------------ conversión
def _append(out: list[dict[str, Any]], role: str, blocks: list[dict[str, Any]]) -> None:
    """Añade bloques uniendo turnos seguidos del mismo rol."""
    if not blocks:
        return
    if out and out[-1]["role"] == role:
        out[-1]["content"].extend(blocks)
    else:
        out.append({"role": role, "content": list(blocks)})


def _text_block(text: Any) -> list[dict[str, Any]]:
    value = str(text or "")
    return [{"type": "text", "text": value}] if value.strip() else []


def _tool_input(arguments: Any) -> dict[str, Any]:
    if isinstance(arguments, dict):
        return arguments
    try:
        value = json.loads(arguments or "{}")
    except (TypeError, ValueError):
        return {}
    return value if isinstance(value, dict) else {}


def convert_messages(
    messages: list[dict[str, Any]],
) -> tuple[str, list[dict[str, Any]]]:
    """Mensajes OpenAI -> (system, mensajes Anthropic)."""
    system_parts: list[str] = []
    out: list[dict[str, Any]] = []
    for message in messages:
        role = message.get("role")
        content = message.get("content")
        if role == "system":
            if not out:
                system_parts.append(str(content or ""))
            else:
                # DECISION: Messages solo admite un system al principio; los
                # avisos a mitad de conversación van como texto del usuario.
                _append(out, "user", _text_block(f"[Aviso del sistema]\n{content}"))
        elif role == "user":
            _append(out, "user", _text_block(content))
        elif role == "assistant":
            blocks = _text_block(content)
            for call in message.get("tool_calls") or []:
                function = call.get("function") or {}
                blocks.append(
                    {
                        "type": "tool_use",
                        "id": call.get("id") or "",
                        "name": function.get("name") or "",
                        "input": _tool_input(function.get("arguments")),
                    }
                )
            _append(out, "assistant", blocks)
        elif role == "tool":
            _append(
                out,
                "user",
                [
                    {
                        "type": "tool_result",
                        "tool_use_id": message.get("tool_call_id") or "",
                        "content": str(content or ""),
                    }
                ],
            )
    return "\n\n".join(part for part in system_parts if part.strip()), out


def convert_tools(tools: list[dict[str, Any]] | None) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for tool in tools or []:
        function = tool.get("function") if tool.get("type") == "function" else tool
        if not isinstance(function, dict) or not function.get("name"):
            continue
        result.append(
            {
                "name": function["name"],
                "description": function.get("description") or "",
                "input_schema": function.get("parameters") or {"type": "object", "properties": {}},
            }
        )
    return result


# ------------------------------------------------------------- streaming
def _chunk(
    content: str | None = None,
    tool_calls: list[Any] | None = None,
    *,
    finish_reason: str | None = None,
    usage: dict[str, Any] | None = None,
) -> Any:
    delta = SimpleNamespace(content=content, tool_calls=tool_calls)
    chunk = SimpleNamespace(choices=[SimpleNamespace(delta=delta, finish_reason=finish_reason)])
    if usage is not None:
        chunk.usage = usage
    return chunk


def _tool_delta(index: int, *, call_id: str | None, name: str | None, arguments: str) -> Any:
    function = SimpleNamespace(name=name, arguments=arguments)
    return _chunk(tool_calls=[SimpleNamespace(index=index, id=call_id, function=function)])


def _sse_event(data: str) -> dict[str, Any] | None:
    text = data.strip()
    if not text or text == "[DONE]":
        return None
    try:
        event = json.loads(text)
    except ValueError:
        return None
    return event if isinstance(event, dict) else None


def _sse_events(lines: list[str]) -> Iterator[dict[str, Any]]:
    """Un bloque SSE: ``data:`` multilínea o, si no es JSON, varias líneas sueltas."""
    if not lines:
        return
    joined = _sse_event("\n".join(lines))
    if joined is not None:
        yield joined
        return
    # DECISION: hay servidores que mandan varios eventos sin línea en blanco;
    # se intentan por separado antes de descartar el bloque entero.
    for line in lines:
        event = _sse_event(line)
        if event is not None:
            yield event


def iter_sse(lines: Iterator[str]) -> Iterator[dict[str, Any]]:
    """Eventos ``data:`` de un flujo SSE; un evento termina en la línea en blanco."""
    buffer: list[str] = []
    for raw in lines:
        line = raw.strip()
        if not line:
            yield from _sse_events(buffer)
            buffer = []
        elif line.startswith("data:"):
            buffer.append(line[5:].strip())
        # ``event:``, ``id:`` y comentarios (``:``) no llevan datos.
    yield from _sse_events(buffer)


def translate_events(events: Iterator[dict[str, Any]]) -> Iterator[Any]:
    """Eventos de Messages -> trozos con forma de Chat Completions."""
    tool_index: dict[int, int] = {}  # índice de bloque -> índice de herramienta
    got_args: set[int] = set()
    for event in events:
        kind = event.get("type")
        if kind == "error":
            error = event.get("error") or {}
            raise RuntimeError(
                f"Anthropic: {error.get('type', 'error')}: {error.get('message', '')}"
            )
        block_index = int(event.get("index") or 0)
        if kind == "content_block_start":
            block = event.get("content_block") or {}
            if block.get("type") == "tool_use":
                position = len(tool_index)
                tool_index[block_index] = position
                yield _tool_delta(
                    position, call_id=block.get("id"), name=block.get("name"), arguments=""
                )
            elif block.get("type") == "text" and block.get("text"):
                yield _chunk(content=block["text"])
        elif kind == "content_block_delta":
            delta = event.get("delta") or {}
            if delta.get("type") == "text_delta" and delta.get("text"):
                yield _chunk(content=delta["text"])
            elif delta.get("type") == "input_json_delta" and block_index in tool_index:
                partial = str(delta.get("partial_json") or "")
                if partial:
                    got_args.add(block_index)
                    yield _tool_delta(
                        tool_index[block_index], call_id=None, name=None, arguments=partial
                    )
        elif kind == "content_block_stop":
            if block_index in tool_index and block_index not in got_args:
                # Herramienta sin argumentos: el bucle espera un JSON válido.
                yield _tool_delta(tool_index[block_index], call_id=None, name=None, arguments="{}")
        elif kind == "message_delta":
            # El bucle necesita saber por qué terminó (límite, herramientas, fin normal).
            stop = (event.get("delta") or {}).get("stop_reason")
            usage = event.get("usage")
            if stop or usage:
                reason = STOP_REASONS.get(str(stop), "stop") if stop else None
                yield _chunk(finish_reason=reason, usage=usage)
        elif kind == "message_stop":
            return


class _Stream:
    """Iterable de trozos con ``close()``, como el stream del SDK de OpenAI."""

    def __init__(self, client: httpx.Client, request: httpx.Request) -> None:
        self._response = client.send(request, stream=True)
        if self._response.status_code >= 400:
            body = self._response.read().decode("utf-8", "replace")[:300]
            self._response.close()
            raise RuntimeError(
                f"La API compatible con Anthropic respondió {self._response.status_code}: {body}"
            )

    def __iter__(self) -> Iterator[Any]:
        return translate_events(iter_sse(self._response.iter_lines()))

    def close(self) -> None:
        # Solo la respuesta: el cliente HTTP es compartido entre peticiones.
        self._response.close()


class _Completions:
    def __init__(self, owner: AnthropicChatClient) -> None:
        self._owner = owner

    def create(self, **request: Any) -> _Stream:
        return self._owner.stream(request)


class AnthropicChatClient:
    """Imita ``OpenAI().chat.completions.create(stream=True, ...)``."""

    def __init__(
        self,
        *,
        api_key: str,
        base_url: str,
        default_headers: dict[str, str] | None = None,
        timeout: float = 60.0,
        transport: httpx.BaseTransport | None = None,
        prompt_cache: bool = False,
    ) -> None:
        self._url = messages_url(base_url)
        self._transport = transport  # solo para pruebas
        self._headers = {
            **(default_headers or {}),
            "x-api-key": api_key,
            "anthropic-version": ANTHROPIC_VERSION,
            "content-type": "application/json",
        }
        self._timeout = timeout
        self._prompt_cache = bool(prompt_cache)
        self._client: httpx.Client | None = None
        self._lock = Lock()
        self.chat = SimpleNamespace(completions=_Completions(self))

    def build_body(self, request: dict[str, Any]) -> dict[str, Any]:
        system, messages = convert_messages(list(request.get("messages") or []))
        body: dict[str, Any] = {
            "model": request.get("model"),
            "max_tokens": int(request.get("max_tokens") or 1024),
            "messages": messages,
            "stream": True,
        }
        if system:
            if self._prompt_cache:
                body["system"] = [
                    {
                        "type": "text",
                        "text": system,
                        "cache_control": {"type": "ephemeral"},
                    }
                ]
            else:
                body["system"] = system
        if request.get("temperature") is not None:
            body["temperature"] = float(request["temperature"])
        tools = convert_tools(request.get("tools"))
        if tools:
            if self._prompt_cache:
                tools[-1]["cache_control"] = {"type": "ephemeral"}
            body["tools"] = tools
            body["tool_choice"] = {"type": "auto"}
        return body

    def _http(self) -> httpx.Client:
        """Cliente HTTP compartido, creado en la primera petición."""
        with self._lock:
            if self._client is None:
                self._client = httpx.Client(timeout=self._timeout, transport=self._transport)
            return self._client

    def close(self) -> None:
        """Cierra el cliente compartido; una petición posterior lo recrea."""
        with self._lock:
            client, self._client = self._client, None
        if client is not None:
            client.close()

    def stream(self, request: dict[str, Any]) -> _Stream:
        client = self._http()
        prepared = client.build_request(
            "POST", self._url, headers=self._headers, json=self.build_body(request)
        )
        return _Stream(client, prepared)

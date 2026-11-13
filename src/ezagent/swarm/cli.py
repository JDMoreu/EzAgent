"""``ezs``: el comando con el que un agente habla con su enjambre.

Es deliberadamente mínimo y solo usa la biblioteca estándar para arrancar
rápido: envía sus argumentos al servicio local de EzAgent e imprime la
respuesta. Quién es el agente lo dice su token (``EZS_TOKEN``), que EzAgent
pone en el entorno de su terminal al lanzarlo.

Códigos de salida: 0 bien, 1 el tablero rechazó el comando, 2 sin conexión o
sin token.
"""

from __future__ import annotations

import json
import os
import socket
import sys
import time

RETRIES = 3
SHORT_TIMEOUT = 10.0
WAIT_TIMEOUT = 130.0  # por encima de la espera máxima del servidor (110 s)
_LOCAL_PREFIX = "http://127.0.0.1:"
_CRLF = "\r\n"


def endpoint_path() -> str:
    custom = os.environ.get("EZS_ENDPOINT")
    if custom:
        return custom
    seat = os.environ.get("EZS_SEAT", "").strip()
    if seat:  # <base>/seats/<asiento> → <base>/endpoint.json
        base = os.path.dirname(os.path.dirname(os.path.abspath(seat)))
        return os.path.join(base, "endpoint.json")
    base = os.environ.get("APPDATA") or os.environ.get("XDG_CONFIG_HOME")
    root = base or os.path.join(os.path.expanduser("~"), ".config")
    return os.path.join(root, "EzAgent", "swarm", "endpoint.json")


def _read_port() -> int:
    """Puerto local publicado por EzAgent, o 0 si no está escuchando."""
    try:
        with open(endpoint_path(), encoding="utf-8") as handle:
            data = json.load(handle)
    except (OSError, ValueError):
        return 0
    url = str(data.get("url") or "") if isinstance(data, dict) else ""
    if not url.startswith(_LOCAL_PREFIX):
        return 0
    port = url[len(_LOCAL_PREFIX) :].split("/", 1)[0]
    return int(port) if port.isdigit() else 0


def _post(port: int, token: str, argv: list[str], timeout: float) -> tuple[bool, str]:
    """Una petición HTTP/1.0 a mano: evita cargar ``http.client`` (~60 ms)."""
    body = json.dumps({"argv": argv}).encode("utf-8")
    lines = [
        "POST /v1 HTTP/1.0",
        f"Host: 127.0.0.1:{port}",
        f"Authorization: Bearer {token}",
        "Content-Type: application/json",
        f"Content-Length: {len(body)}",
        "",
        "",
    ]
    chunks: list[bytes] = []
    with socket.create_connection(("127.0.0.1", port), timeout=timeout) as sock:
        sock.sendall(_CRLF.join(lines).encode("ascii") + body)
        while chunk := sock.recv(65536):
            chunks.append(chunk)
    payload = b"".join(chunks).partition((_CRLF + _CRLF).encode("ascii"))[2]
    data = json.loads(payload.decode("utf-8"))
    return bool(data.get("ok")), str(data.get("text") or "")


def _token() -> str:
    """Token del miembro: ``EZS_TOKEN`` o el archivo que indica ``EZS_SEAT``."""
    token = os.environ.get("EZS_TOKEN", "").strip()
    seat = os.environ.get("EZS_SEAT", "").strip()
    if not token and seat:
        try:
            with open(seat, encoding="utf-8") as handle:
                token = handle.read(200).strip()
        except OSError:
            token = ""
    return token


def _out(text: str, *, error: bool = False) -> None:
    stream = sys.stderr if error else sys.stdout
    stream.write(text.rstrip("\n") + "\n")


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if callable(reconfigure):  # las consolas de Windows no siempre son UTF-8
            reconfigure(encoding="utf-8", errors="replace")
    token = _token()
    if not token or not token.isascii() or any(char.isspace() for char in token):
        _out(
            "Esta terminal no pertenece a un enjambre de EzAgent (falta EZS_SEAT). "
            "No lo investigues: termina tu turno.",
            error=True,
        )
        return 2
    timeout = WAIT_TIMEOUT if args[:1] == ["wait"] else SHORT_TIMEOUT
    for attempt in range(RETRIES):
        port = _read_port()
        try:
            if not port:
                raise ConnectionError("sin dirección")
            ok, text = _post(port, token, args, timeout)
        except TimeoutError:
            # No se reintenta: el comando pudo haberse aplicado.
            _out("El tablero tardó demasiado. Comprueba con: ezs board", error=True)
            return 2
        except (OSError, ValueError):
            if attempt + 1 < RETRIES:
                time.sleep(0.4 * (attempt + 1))  # EzAgent puede estar reiniciándose
            continue
        _out(text, error=not ok)
        return 0 if ok else 1
    _out(
        "EzAgent no responde. No lo investigues: sigue con tu tarea y reintenta en un minuto.",
        error=True,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())

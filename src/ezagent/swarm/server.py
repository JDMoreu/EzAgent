"""Servidor local del enjambre: HTTP mínimo sobre la biblioteca estándar.

Solo escucha en ``127.0.0.1`` y cada petición exige el token del miembro
(AGENTS.md §5). Hay un único punto, ``POST /v1``, con ``{"argv": [...]}``: el
análisis del comando vive en ``SwarmService``, así el cliente ``ezs`` es tonto
y no hay dos sitios que mantener.

El puerto es efímero. Se publica en un archivo (``endpoint.json``) que el
cliente lee en cada llamada: si EzAgent se reinicia con otro puerto, los
agentes que ya estaban abiertos lo encuentran sin relanzarse.
"""

from __future__ import annotations

import json
import os
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

from loguru import logger

from .service import Reply, SwarmService

HOST = "127.0.0.1"
MAX_REQUEST_BYTES = 16_384
MAX_ARGS = 64
_LOCAL_HOSTS = frozenset({"127.0.0.1", "localhost"})


def _handler_for(service: SwarmService) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        server_version = "EzSwarm/1"
        timeout = 15  # lectura de la petición; un cliente colgado no retiene el hilo

        def log_message(self, format: str, *args: Any) -> None:  # noqa: A002
            return  # sin registro por petición: ni tokens ni cuerpos llegan al log

        def _send(self, status: int, reply: Reply) -> None:
            body = json.dumps({"ok": reply.ok, "text": reply.text}, ensure_ascii=False).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def _argv(self) -> list[str] | None:
            try:
                length = int(self.headers.get("Content-Length") or 0)
            except ValueError:
                return None
            if not 0 < length <= MAX_REQUEST_BYTES:
                return None
            try:
                data = json.loads(self.rfile.read(length).decode("utf-8"))
            except (ValueError, UnicodeDecodeError):
                return None
            argv = data.get("argv") if isinstance(data, dict) else None
            if not isinstance(argv, list) or len(argv) > MAX_ARGS:
                return None
            return [str(item) for item in argv]

        def do_POST(self) -> None:  # noqa: N802 - nombre impuesto por http.server
            host = (self.headers.get("Host") or "").rsplit(":", 1)[0]
            if self.path != "/v1" or host not in _LOCAL_HOSTS:
                self._send(404, Reply(False, "No encontrado."))
                return
            scheme, _, token = (self.headers.get("Authorization") or "").partition(" ")
            if scheme != "Bearer" or not token.strip():
                self._send(401, Reply(False, "Falta el token del enjambre."))
                return
            argv = self._argv()
            if argv is None:
                self._send(400, Reply(False, "Petición no válida o demasiado larga."))
                return
            reply = service.handle(token.strip(), argv)
            self._send(200, reply)

        def do_GET(self) -> None:  # noqa: N802
            self._send(404, Reply(False, "No encontrado."))

    return Handler


class SwarmServer:
    """Arranca y detiene el servidor local, y publica dónde escucha."""

    def __init__(self, service: SwarmService, endpoint_path: Path) -> None:
        self.service = service
        self.endpoint_path = Path(endpoint_path)
        self._httpd: ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None
        self._lock = threading.Lock()

    @property
    def url(self) -> str:
        httpd = self._httpd
        return f"http://{HOST}:{httpd.server_address[1]}/v1" if httpd else ""

    def start(self) -> str:
        """Empieza a escuchar en un puerto libre de 127.0.0.1; devuelve la URL."""
        with self._lock:
            if self._httpd is None:
                httpd = ThreadingHTTPServer((HOST, 0), _handler_for(self.service))
                httpd.daemon_threads = True
                self.service.open()
                self._httpd = httpd
                self._thread = threading.Thread(
                    target=httpd.serve_forever, name="ezagent-swarm", daemon=True
                )
                self._thread.start()
                self._publish(self.url)
                logger.info("Servicio del enjambre en {}", self.url)
            return self.url

    def stop(self) -> None:
        with self._lock:
            httpd, thread = self._httpd, self._thread
            self._httpd = self._thread = None
        if httpd is None:
            return
        self.service.close()
        httpd.shutdown()
        httpd.server_close()
        if thread is not None:
            thread.join(timeout=5)
        self._publish("")

    def _publish(self, url: str) -> None:
        """Escribe ``endpoint.json`` de forma atómica (vacío al detenerse)."""
        try:
            self.endpoint_path.parent.mkdir(parents=True, exist_ok=True)
            tmp = self.endpoint_path.with_name(self.endpoint_path.name + ".tmp")
            tmp.write_text(json.dumps({"url": url, "pid": os.getpid()}), encoding="utf-8")
            os.replace(tmp, self.endpoint_path)
        except OSError as exc:
            logger.warning("No pude publicar la dirección del enjambre: {}", exc)


__all__ = ["HOST", "SwarmServer"]

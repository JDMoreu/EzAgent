"""Fixtures de herramientas web y búsqueda (dominio `tools_web`, F1-14).

Cubre `src/ezagent/agent/tools_web.py` y `src/ezagent/agent/search.py` sin red:

- `httpx.MockTransport` sirve páginas de prueba (redirecciones, cuerpos,
  codificaciones y errores).
- `socket.getaddrinfo` se sustituye por un DNS falso determinista.
- El proveedor `ddgs` se sustituye por motores de prueba; nunca se importa.
- `time.monotonic` de `tools_web` se puede congelar para probar el tiempo total.

Las IP y los nombres de host son de documentación (example.com, `.test`), así
que no hay rutas de la máquina en los fixtures.
"""

from __future__ import annotations

import contextlib
import os
import sys
import types
from dataclasses import asdict
from pathlib import Path
from typing import Any

from _common import FixtureFile, main_guard

DOMAIN = "tools_web"

PUBLIC = "93.184.216.34"
PUBLIC_V6 = "2606:2800:220:1:248:1893:25c8:1946"


def _config(home: Path, max_fetch_chars: int = 8000, block_private_ips: bool = True) -> Any:
    from ezagent.agent.config import (
        AgentConfig,
        ConfirmConfig,
        GeneralConfig,
        PathsConfig,
        SkillsConfig,
        ToolsDConfig,
        WebConfig,
    )

    work = home / "web-cases"
    return AgentConfig(
        general=GeneralConfig(),
        paths=PathsConfig(read_roots=[], write_roots=[], deny=[], deny_globs=[]),
        confirm=ConfirmConfig(accept=[], reject=[]),
        web=WebConfig(max_fetch_chars=max_fetch_chars, block_private_ips=block_private_ips),
        skills=SkillsConfig(),
        tools_d=ToolsDConfig(allowed_executables=[]),
        app_dir=work / "app",
        workspace=work / "workspace",
        permissions_path=work / "app" / "permissions.toml",
        system_md_path=work / "app" / "system.md",
        memory_dir=work / "app" / "memory",
        history_path=work / "app" / "memory" / "history.db",
        audit_path=work / "app" / "logs" / "audit.jsonl",
        journal_dir=work / "app" / "journal",
        skills_dir=work / "app" / "skills",
        global_skills_dir=work / "app" / "global-skills",
        tools_d_dir=work / "app" / "tools.d",
    )


@contextlib.contextmanager
def _dns(mapping: dict[str, Any]):
    from ezagent.agent import tools_web as module

    original = module.socket.getaddrinfo

    def fake(host: str, port: int, *args: Any, **kwargs: Any) -> Any:
        value = mapping.get(host)
        if isinstance(value, Exception):
            raise value
        if value is None:
            import socket

            raise socket.gaierror(-2, "nombre desconocido")
        infos = []
        for address in value:
            import socket

            family = socket.AF_INET6 if ":" in address else socket.AF_INET
            infos.append((family, socket.SOCK_STREAM, 6, "", (address, port)))
        return infos

    module.socket.getaddrinfo = fake
    try:
        yield
    finally:
        module.socket.getaddrinfo = original


class _Timeline:
    def __init__(self, values: list[float]) -> None:
        self.values = values
        self.index = 0

    def __call__(self) -> float:
        value = self.values[min(self.index, len(self.values) - 1)]
        self.index += 1
        return value


@contextlib.contextmanager
def _clock(values: list[float]):
    from ezagent.agent import tools_web as module

    original = module.time
    module.time = types.SimpleNamespace(monotonic=_Timeline(values))
    try:
        yield
    finally:
        module.time = original


@contextlib.contextmanager
def _broken_web_dependency(which: str):
    original = dict(sys.modules)
    try:
        if which == "bs4":
            sys.modules["bs4"] = None  # type: ignore[assignment]
        else:
            fake = types.ModuleType("bs4")

            class FeatureNotFound(Exception):
                pass

            def soup(*args: Any, **kwargs: Any) -> Any:
                raise FeatureNotFound("Couldn't find a tree builder with the features you requested: lxml")

            fake.FeatureNotFound = FeatureNotFound  # type: ignore[attr-defined]
            fake.BeautifulSoup = soup  # type: ignore[attr-defined]
            sys.modules["bs4"] = fake
        yield
    finally:
        sys.modules.clear()
        sys.modules.update(original)


def _error(error: BaseException) -> dict[str, Any]:
    return {"type": type(error).__name__, "message": str(error)}


def _jsonable_mapping(mapping: dict[str, Any]) -> dict[str, Any]:
    return {
        key: ({"error": _error(value)} if isinstance(value, Exception) else value)
        for key, value in mapping.items()
    }


def _fetch(
    out: FixtureFile,
    home: Path,
    label: str,
    url: Any,
    *,
    handler: Any,
    max_chars: Any = 8000,
    mapping: dict[str, Any] | None = None,
    block_private_ips: bool = True,
    clock: list[float] | None = None,
) -> None:
    import httpx

    from ezagent.agent.tools_web import run_fetch_page

    config = _config(home, block_private_ips=block_private_ips)
    requests: list[dict[str, Any]] = []

    def serve(request: httpx.Request) -> httpx.Response:
        requests.append(
            {
                "method": request.method,
                "url": str(request.url),
                "host": request.headers.get("host"),
                "user_agent": request.headers.get("user-agent"),
                "accept": request.headers.get("accept"),
                "sni": request.extensions.get("sni_hostname"),
            }
        )
        return handler(request)

    transport = httpx.MockTransport(serve)
    with _dns(mapping or {}), _clock(clock or [1000.0, 1000.0, 1000.0]):
        result = run_fetch_page(config, url, max_chars, transport=transport)
    out.add(
        f"tools_web/fetch/{label}",
        "run_fetch_page",
        {
            "url": url,
            "max_chars": max_chars,
            "block_private_ips": block_private_ips,
            "dns": _jsonable_mapping(mapping or {}),
            "requests": [{"method": item["method"], "url": item["url"]} for item in requests],
        },
        {"result": result.to_dict(), "requests": requests},
    )


def _page(status: int = 200, body: str | bytes = "", headers: dict[str, str] | None = None) -> Any:
    import httpx

    content = body.encode("utf-8") if isinstance(body, str) else body
    return httpx.Response(status, headers=headers or {"content-type": "text/html; charset=utf-8"}, content=content)


def _fetch_cases(out: FixtureFile, home: Path) -> None:
    import httpx

    mapping = {"example.test": [PUBLIC], "other.test": [PUBLIC], "doble.test": ["10.0.0.5"]}
    base_url = "http://example.test/"

    _fetch(out, home, "html", base_url, handler=lambda request: _page(200, "<html><body><h1>Hola</h1><p>Mundo</p></body></html>"), mapping=mapping)
    _fetch(out, home, "empty", base_url, handler=lambda request: _page(200, ""), mapping=mapping)
    _fetch(
        out,
        home,
        "scripts",
        base_url,
        handler=lambda request: _page(
            200,
            "<div>Antes<script>alert('x')</script><style>p{}</style><noscript>no</noscript>Después</div>",
        ),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "entities",
        base_url,
        handler=lambda request: _page(200, "<p>a &amp; b &lt; c &nbsp; d</p>"),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "plain",
        base_url,
        handler=lambda request: _page(200, "línea 1\nlínea 2\n", {"content-type": "text/plain"}),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "latin1",
        base_url,
        handler=lambda request: _page(
            200,
            b"<p>ca\xf1\xf3n</p>",
            {"content-type": "text/html; charset=iso-8859-1"},
        ),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "bad-charset",
        base_url,
        handler=lambda request: _page(
            200,
            b"<p>x</p>",
            {"content-type": "text/html; charset=no-existe"},
        ),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "invalid-utf8",
        base_url,
        handler=lambda request: _page(
            200,
            b"<p>a\xffb</p>",
            {"content-type": "text/html; charset=utf-8"},
        ),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "redirect-relative",
        base_url + "inicio",
        handler=lambda request: (
            httpx.Response(302, headers={"location": "/final"})
            if request.url.path == "/inicio"
            else _page(200, "<p>final</p>")
        ),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "redirect-absolute",
        base_url,
        handler=lambda request: (
            httpx.Response(301, headers={"location": "https://other.test/x"})
            if request.url.host == "example.test"
            else _page(200, "<p>otro</p>")
        ),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "redirect-too-many",
        base_url,
        handler=lambda request: httpx.Response(302, headers={"location": "/otra"}),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "redirect-no-location",
        base_url,
        handler=lambda request: httpx.Response(302),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "redirect-javascript",
        base_url,
        handler=lambda request: httpx.Response(302, headers={"location": "javascript:alert(1)"}),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "redirect-ftp",
        base_url,
        handler=lambda request: httpx.Response(302, headers={"location": "ftp://example.test/x"}),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "redirect-private",
        base_url,
        handler=lambda request: httpx.Response(302, headers={"location": "http://127.0.0.1/x"}),
        mapping=mapping,
    )
    _fetch(out, home, "redirect-localhost", base_url, handler=lambda request: httpx.Response(302, headers={"location": "http://localhost/x"}), mapping=mapping)
    _fetch(out, home, "status-404", base_url, handler=lambda request: _page(404, "no"), mapping=mapping)
    _fetch(out, home, "status-500", base_url, handler=lambda request: _page(500, "error"), mapping=mapping)
    _fetch(out, home, "status-204", base_url, handler=lambda request: _page(204, ""), mapping=mapping)
    _fetch(
        out,
        home,
        "too-large",
        base_url,
        handler=lambda request: _page(200, b"z" * (2 * 1024 * 1024 + 1)),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "timeout",
        base_url,
        handler=lambda request: (_ for _ in ()).throw(httpx.ReadTimeout("timed out")),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "oserror",
        base_url,
        handler=lambda request: (_ for _ in ()).throw(OSError("sin ruta")),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "unexpected",
        base_url,
        handler=lambda request: (_ for _ in ()).throw(ValueError("roto")),
        mapping=mapping,
    )
    _fetch(out, home, "budget", base_url, handler=lambda request: _page(200, "x"), mapping=mapping, clock=[0.0, 9.0])
    _fetch(
        out,
        home,
        "truncate",
        base_url,
        handler=lambda request: _page(200, "<p>" + "z" * 50 + "</p>"),
        mapping=mapping,
        max_chars=10,
    )
    _fetch(
        out,
        home,
        "max-default",
        base_url,
        handler=lambda request: _page(200, "<p>" + "z" * 50 + "</p>"),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "pinned-host-port",
        "http://example.test:8080/ruta?x=1#frag",
        handler=lambda request: _page(200, "<p>puerto</p>"),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "pinned-https",
        "https://example.test/ruta",
        handler=lambda request: _page(200, "<p>seguro</p>"),
        mapping=mapping,
    )
    _fetch(
        out,
        home,
        "pinned-idn",
        "http://ñandú.test/ruta",
        handler=lambda request: _page(200, "<p>idn</p>"),
        mapping={"ñandú.test": [PUBLIC]},
    )
    _fetch(
        out,
        home,
        "pinned-trailing-dot",
        "http://EXAMPLE.TEST./ruta",
        handler=lambda request: _page(200, "<p>punto</p>"),
        mapping={"example.test": [PUBLIC]},
    )
    _fetch(
        out,
        home,
        "literal-public",
        "http://93.184.216.34/ruta",
        handler=lambda request: _page(200, "<p>literal</p>"),
        mapping={},
    )
    _fetch(
        out,
        home,
        "allow-private",
        "http://127.0.0.1/ruta",
        handler=lambda request: _page(200, "<p>local</p>"),
        mapping={},
        block_private_ips=False,
    )
    _fetch(
        out,
        home,
        "allow-localhost-hostname",
        "http://localhost/ruta",
        handler=lambda request: _page(200, "<p>local</p>"),
        mapping={"localhost": ["127.0.0.1"]},
        block_private_ips=False,
    )
    # Errores de URL antes de tocar el transporte.
    for label, url in [
        ("url-non-text", 5),
        ("url-empty", ""),
        ("url-blank", "   "),
        ("url-space", "http://example.test/a b"),
        ("url-control", "http://example.test/a\nb"),
        ("url-scheme", "ftp://example.test/x"),
        ("url-credentials", "http://user:pass@example.test/x"),
        ("url-no-host", "http:///x"),
        ("url-bad-port", "http://example.test:abc/x"),
        ("url-localhost", "http://localhost/x"),
        ("url-localhost-sub", "http://app.localhost/x"),
        ("url-private-v4", "http://127.0.0.1/x"),
        ("url-private-10", "http://10.0.0.1/x"),
        ("url-private-v6", "http://[::1]/x"),
        ("url-mapped", "http://[::ffff:10.0.0.1]/x"),
    ]:
        _fetch(out, home, label, url, handler=lambda request: _page(200, "x"), mapping=mapping)
    _fetch(out, home, "url-dns-private", "http://doble.test/x", handler=lambda request: _page(200, "x"), mapping=mapping)
    _fetch(
        out,
        home,
        "url-dns-empty",
        "http://vacio.test/x",
        handler=lambda request: _page(200, "x"),
        mapping={"vacio.test": []},
    )
    _fetch(
        out,
        home,
        "url-dns-fail",
        "http://roto.test/x",
        handler=lambda request: _page(200, "x"),
        mapping={"roto.test": __import__("socket").gaierror(-2, "no resuelve")},
    )
    _fetch(out, home, "max-bool", base_url, handler=lambda request: _page(200, "x"), mapping=mapping, max_chars=True)
    _fetch(out, home, "max-zero", base_url, handler=lambda request: _page(200, "x"), mapping=mapping, max_chars=0)
    _fetch(out, home, "max-too-big", base_url, handler=lambda request: _page(200, "x"), mapping=mapping, max_chars=8001)
    _fetch(out, home, "max-text", base_url, handler=lambda request: _page(200, "x"), mapping=mapping, max_chars="10")
    _fetch(out, home, "max-none", base_url, handler=lambda request: _page(200, "x"), mapping=mapping, max_chars=None)


def _target_cases(out: FixtureFile, home: Path) -> None:
    from ezagent.agent.tools_web import _pinned_request_target, _validate_public_target

    mapping = {
        "example.test": [PUBLIC, PUBLIC_V6],
        "v6.test": [PUBLIC_V6],
        "doble.test": ["10.0.0.5"],
        "local.test": ["127.0.0.1"],
        "ñandú.test": [PUBLIC],
        "vacio.test": [],
        "roto.test": __import__("socket").gaierror(-2, "no resuelve"),
    }
    targets = [
        ("public", "http://example.test/ruta", True),
        ("root", "http://example.test", True),
        ("https-upper", "HTTPS://EXAMPLE.TEST/x", True),
        ("spaced-edges", "  http://example.test/x  ", True),
        ("public-port", "http://example.test:8080/ruta?x=1#f", True),
        ("https-default", "https://example.test/ruta", True),
        ("literal", "http://93.184.216.34/ruta", True),
        ("literal-v6", "http://[2606:2800:220:1:248:1893:25c8:1946]/x", True),
        ("private", "http://10.0.0.5/x", True),
        ("private-dns", "http://doble.test/x", True),
        ("local-dns", "http://local.test/x", True),
        ("v6-only", "https://v6.test/x", True),
        ("idn", "http://ñandú.test/x", True),
        ("trailing-dot", "http://EXAMPLE.TEST./x", True),
        ("empty-dns", "http://vacio.test/x", True),
        ("broken-dns", "http://roto.test/x", True),
        ("disabled", "http://127.0.0.1/x", False),
        ("scheme", "ftp://example.test/x", True),
        ("credentials", "http://u:p@example.test/x", True),
        ("no-host", "http:///x", True),
        ("bad-port", "http://example.test:abc/x", True),
        ("space", "http://example.test/a b", True),
        ("blank", "", True),
        ("non-text", 5, True),
        ("localhost", "http://localhost/x", True),
        ("localhost-sub", "http://x.localhost/x", True),
        ("mapped", "http://[::ffff:10.0.0.1]/x", True),
    ]
    with _dns(mapping):
        for label, url, block in targets:
            config = _config(home, block_private_ips=block)
            try:
                validated, host, port, addresses = _validate_public_target(url, config)
                value: dict[str, Any] = {
                    "url": validated,
                    "host": host,
                    "port": port,
                    "addresses": [str(item) for item in addresses],
                }
                error = None
            except Exception as exc:
                value = None
                error = _error(exc)
            if error is None:
                out.add(
                    f"tools_web/validate_public_target/{label}",
                    "validate_public_target",
                    {"url": url, "block_private_ips": block},
                    value,
                )
            else:
                out.add_error(
                    f"tools_web/validate_public_target/{label}",
                    "validate_public_target",
                    {"url": url, "block_private_ips": block},
                    error["message"],
                )
            try:
                pinned = _pinned_request_target(url, config)
                pinned_value: Any = {
                    "connect_url": pinned[0],
                    "headers": pinned[1],
                    "extensions": pinned[2],
                }
            except Exception as exc:
                pinned_value = None
                error_pinned = _error(exc)
            else:
                error_pinned = None
            if error_pinned is None:
                out.add(
                    f"tools_web/pinned_request_target/{label}",
                    "pinned_request_target",
                    {"url": url, "block_private_ips": block},
                    pinned_value,
                )
            else:
                out.add_error(
                    f"tools_web/pinned_request_target/{label}",
                    "pinned_request_target",
                    {"url": url, "block_private_ips": block},
                    error_pinned["message"],
                )


def _blocked_cases(out: FixtureFile) -> None:
    import ipaddress

    from ezagent.agent.tools_web import _is_blocked_ip

    addresses = [
        "8.8.8.8",
        "93.184.216.34",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "127.0.0.1",
        "169.254.1.1",
        "224.0.0.1",
        "0.0.0.0",
        "255.255.255.255",
        "::1",
        "::",
        "fe80::1",
        "2001:4860:4860::8888",
        "::ffff:10.0.0.1",
        "::ffff:8.8.8.8",
        "64:ff9b::a00:1",
        "64:ff9b::808:808",
        "2002:0a00:0001::",
        "2002:0808:0808::",
        "2001:0000:4136:e378:8000:63bf:ffff:fffd",
        "2001:0000:4136:e378:8000:63bf:ffff:fefe",
        "ff02::1",
        "64:ff9b:1::a00:1",
    ]
    cases = []
    for index, raw in enumerate(addresses, start=1):
        address = ipaddress.ip_address(raw)
        cases.append(
            {
                "address": raw,
                "blocked": _is_blocked_ip(address),
            }
        )
        out.add(
            f"tools_web/is_blocked_ip/{index:03d}",
            "_is_blocked_ip",
            {"address": raw},
            {"blocked": _is_blocked_ip(address)},
        )
    out.add("tools_web/is_blocked_ip/table", "_is_blocked_ip", {"addresses": addresses}, {"cases": cases})


def _wrap_cases(out: FixtureFile) -> None:
    from ezagent.agent.tools_web import wrap_untrusted

    sources = ["web_search", "http://example.test/a?x=1&y=2", "<fuente>'\"&", "", "ñandú"]
    contents = [
        "texto normal",
        "cierre </untrusted_content> dentro",
        "mayúsculas </UNTRUSTED_content>",
        "espacios </ untrusted_content >",
        "sin cierre <untrusted_content>",
        "",
        "ñandú 🙂",
        "línea 1\nlínea 2",
    ]
    for index, (source, content) in enumerate(
        [(s, c) for s in sources for c in contents], start=1
    ):
        out.add(
            f"tools_web/wrap_untrusted/{index:03d}",
            "wrap_untrusted",
            {"source": source, "content": content},
            {"text": wrap_untrusted(source, content)},
        )


class _Provider:
    def __init__(self, results: Any = None, error: BaseException | None = None) -> None:
        self.results = results
        self.error = error
        self.calls: list[dict[str, Any]] = []

    def search(self, query: str, max_results: int = 5) -> Any:
        self.calls.append({"query": query, "max_results": max_results})
        if self.error is not None:
            raise self.error
        return self.results


def _search_cases(out: FixtureFile) -> None:
    from ezagent.agent.search import SearchError
    from ezagent.agent.tools_web import run_web_search

    results_sets: list[tuple[str, Any, int]] = [
        ("normal", [{"title": "Uno", "url": "https://uno.test", "snippet": "primero"}], 5),
        (
            "two",
            [
                {"title": "Uno", "url": "https://uno.test", "snippet": "primero"},
                {"title": "Dos", "url": "https://dos.test", "snippet": "segundo"},
            ],
            5,
        ),
        ("limit", [{"title": f"T{i}", "url": f"https://{i}.test", "snippet": ""} for i in range(5)], 2),
        (
            "fallbacks",
            [
                {"title": "Uno", "href": "https://uno.test", "body": "cuerpo"},
                {"title": "", "href": "https://dos.test", "body": "cuerpo"},
                {"url": "https://tres.test", "snippet": "frag"},
                {"title": "Cuatro", "url": "https://cuatro.test", "snippet": "  "},
                {"title": "  Cinco  ", "url": "  https://cinco.test  ", "snippet": "  x  "},
            ],
            5,
        ),
        ("non-dict", ["texto", 5, None, {"title": "ok"}], 5),
        ("empty", [], 5),
        ("tuple", ({"title": "Tupla", "url": "https://t.test"},), 5),
        ("bad-shape", "texto", 5),
        ("bad-shape-number", 5, 5),
    ]
    for label, results, limit in results_sets:
        provider = _Provider(results)
        result = run_web_search(provider, "consulta ñ", limit)
        out.add(
            f"tools_web/run_web_search/{label}",
            "run_web_search",
            {"query": "consulta ñ", "max_results": limit, "results": results},
            {"result": result.to_dict(), "calls": provider.calls},
        )

    errors: list[tuple[str, BaseException]] = [
        ("search-error", SearchError("proveedor caído", kind="provider")),
        ("search-timeout", SearchError("tardó", kind="timeout")),
        ("search-unavailable", SearchError("falta", kind="unavailable")),
        ("generic", RuntimeError("boom")),
    ]
    for label, error in errors:
        provider = _Provider(None, error)
        result = run_web_search(provider, "x", 3)
        out.add(
            f"tools_web/run_web_search/{label}",
            "run_web_search",
            {"query": "x", "max_results": 3},
            {"result": result.to_dict(), "calls": provider.calls},
        )

    for index, (query, limit) in enumerate(
        [
            ("", 5),
            ("   ", 5),
            ("x" * 501, 5),
            (5, 5),
            (None, 5),
            ("x", True),
            ("x", 0),
            ("x", 11),
            ("x", "5"),
            ("x", None),
            ("x", 1.5),
        ],
        start=1,
    ):
        provider = _Provider([])
        result = run_web_search(provider, query, limit)  # type: ignore[arg-type]
        out.add(
            f"tools_web/run_web_search/invalid{index:02d}",
            "run_web_search",
            {"query": query, "max_results": limit},
            {"result": result.to_dict(), "calls": provider.calls},
        )


class _Engine:
    def __init__(self, results: Any = None, error: BaseException | None = None) -> None:
        self.results = results
        self.error = error
        self.calls: list[dict[str, Any]] = []

    def text(self, query: str, **kwargs: Any) -> Any:
        self.calls.append({"query": query, **kwargs})
        if self.error is not None:
            raise self.error
        return self.results


def _ddgs_cases(out: FixtureFile) -> None:
    from ezagent.agent.search import DdgsSearchProvider

    raw_sets: list[tuple[str, Any, int]] = [
        (
            "normal",
            [
                {"title": "Uno", "href": "https://uno.test", "body": "cuerpo"},
                {"title": "Dos", "url": "https://dos.test", "snippet": "frag"},
                {"title": "Tres", "href": "https://tres.test", "description": "desc"},
            ],
            5,
        ),
        (
            "limit",
            [{"title": f"T{i}", "href": f"https://{i}.test", "body": "x"} for i in range(5)],
            2,
        ),
        (
            "skip",
            [{"title": "", "href": ""}, {"body": "sin titulo ni url"}, "texto", {"title": "Ok"}],
            5,
        ),
        ("none", None, 5),
        ("empty", [], 5),
        ("bad-shape", "texto", 5),
    ]
    for label, raw, limit in raw_sets:
        engine = _Engine(raw)
        provider = DdgsSearchProvider()
        provider._engine = engine
        results = provider.search("consulta ñ", limit)
        out.add(
            f"tools_web/DdgsSearchProvider/{label}",
            "DdgsSearchProvider.search",
            {"query": "consulta ñ", "max_results": limit, "raw": raw},
            {"results": results, "calls": engine.calls, "engine_reused": provider._engine is engine},
        )

    errors: list[tuple[str, BaseException, str]] = [
        ("no-results", Exception("No results found."), "list"),
        ("timeout", TimeoutError("timed out"), "error"),
        ("rate-limit", Exception("Too Many Requests 429"), "error"),
        ("provider", Exception("otra cosa"), "error"),
        ("oserror", OSError("red caída"), "error"),
    ]
    for label, error, _kind in errors:
        engine = _Engine(None, error)
        provider = DdgsSearchProvider()
        provider._engine = engine
        try:
            results = provider.search("x", 3)
        except Exception as exc:
            out.add_error(
                f"tools_web/DdgsSearchProvider/error/{label}",
                "DdgsSearchProvider.search",
                {"query": "x", "max_results": 3, "raw": None},
                str(exc),
            )
            # Rama interna: un motor que falló se descarta para el siguiente intento.
            out.add(
                f"tools_web/DdgsSearchProvider/state/{label}",
                "DdgsSearchProvider.search",
                {"query": "x", "max_results": 3, "after_error": True},
                {"engine_reset": provider._engine is None, "calls": engine.calls},
            )
        else:
            out.add(
                f"tools_web/DdgsSearchProvider/results/{label}",
                "DdgsSearchProvider.search",
                {"query": "x", "max_results": 3, "raw": None},
                {"results": results, "calls": engine.calls},
            )

    for index, (query, limit) in enumerate(
        [("", 5), ("  ", 5), (5, 5), (None, 5), ("x", True), ("x", 0), ("x", 11), ("x", "5")],
        start=1,
    ):
        provider = DdgsSearchProvider()
        try:
            value = {"results": provider.search(query, limit)}  # type: ignore[arg-type]
        except Exception as exc:
            out.add_error(
                f"tools_web/DdgsSearchProvider/invalid{index:02d}",
                "DdgsSearchProvider.search",
                {"query": query, "max_results": limit},
                str(exc),
            )
        else:
            out.add(
                f"tools_web/DdgsSearchProvider/invalid{index:02d}",
                "DdgsSearchProvider.search",
                {"query": query, "max_results": limit},
                value,
            )

    engine = _Engine([{"title": "Uno", "href": "https://uno.test"}])
    provider = DdgsSearchProvider(region="es-es", backend="html", timeout_s=2.5)
    provider._engine = engine
    provider.search("x", 1)
    first = provider._engine
    provider.search("y", 2)
    out.add(
        "tools_web/DdgsSearchProvider/kwargs",
        "DdgsSearchProvider.search",
        {"region": "es-es", "backend": "html", "timeout_s": 2.5},
        {"calls": engine.calls, "reused": provider._engine is first},
    )

    class _NoTimeoutEngine:
        def __init__(self) -> None:
            self.kwargs: list[dict[str, Any]] = []

        def text(self, query: str, **kwargs: Any) -> list[dict[str, str]]:
            self.kwargs.append(kwargs)
            return []

    class _AcceptTimeout:
        def __init__(self, timeout: float = 0) -> None:
            self.timeout = timeout

        def text(self, query: str, **kwargs: Any) -> list[dict[str, str]]:
            return []

    class _RejectTimeout:
        def __init__(self, *args: Any, **kwargs: Any) -> None:
            if args or kwargs:
                raise TypeError("timeout no soportado")

        def text(self, query: str, **kwargs: Any) -> list[dict[str, str]]:
            return []

    provider = DdgsSearchProvider(timeout_s=1.5)
    out.add(
        "tools_web/DdgsSearchProvider/new-engine",
        "DdgsSearchProvider._new_engine",
        {"timeout_s": 1.5},
        {
            "accept": {k: v for k, v in vars(provider._new_engine(_AcceptTimeout)).items()},
            "reject": type(provider._new_engine(_RejectTimeout)).__name__,
        },
    )

    # Carga real del paquete ddgs: módulo falso, ausente y submódulo ausente.
    fake = types.ModuleType("ddgs")
    fake.DDGS = _AcceptTimeout  # type: ignore[attr-defined]
    original = dict(sys.modules)
    try:
        sys.modules["ddgs"] = fake
        out.add(
            "tools_web/DdgsSearchProvider/load-ok",
            "DdgsSearchProvider._load_ddgs",
            {"module": "ddgs falso"},
            {"class": DdgsSearchProvider._load_ddgs().__name__},
        )
    finally:
        sys.modules.clear()
        sys.modules.update(original)

    class _Finder:
        def __init__(self, name: str) -> None:
            self.name = name

        def find_spec(self, fullname: str, path: Any = None, target: Any = None) -> Any:
            if fullname == "ddgs" or fullname.startswith("ddgs."):
                raise ModuleNotFoundError(f"No module named '{fullname}'", name=self.name)
            return None

    for label, missing in [("missing", "ddgs"), ("other-error", "otro")]:
        finder = _Finder(missing)
        original = dict(sys.modules)
        original_meta = list(sys.meta_path)
        sys.modules.pop("ddgs", None)
        sys.meta_path.insert(0, finder)
        try:
            try:
                value = {"class": DdgsSearchProvider._load_ddgs().__name__}
                failure = None
            except Exception as exc:
                value = None
                failure = str(exc)
        finally:
            sys.meta_path[:] = original_meta
            sys.modules.clear()
            sys.modules.update(original)
        if failure is None:
            out.add(
                f"tools_web/DdgsSearchProvider/{label}",
                "DdgsSearchProvider._load_ddgs",
                {"missing": missing},
                value,
            )
        else:
            out.add_error(
                f"tools_web/DdgsSearchProvider/{label}",
                "DdgsSearchProvider._load_ddgs",
                {"missing": missing},
                failure,
            )


def _extract_cases(out: FixtureFile) -> None:
    from ezagent.agent.tools_web import _extract_text

    pages = [
        ("simple", "<html><body><h1>Título</h1><p>Texto</p></body></html>"),
        ("spaces", "<p>  a  </p>\n\n<p> b </p>"),
        ("entities", "<p>a &amp; b &lt; c &nbsp; d</p>"),
        (
            "scripts",
            "<div>uno<script>x()</script>dos<style>p{}</style>tres<noscript>no</noscript></div>",
        ),
        ("empty", ""),
        ("unicode", "<p>ñandú 🙂</p>"),
    ]
    for index, (label, page) in enumerate(pages, start=1):
        out.add(
            f"tools_web/extract/{index:02d}-{label}",
            "_extract_text",
            {"html": page},
            {"text": _extract_text(page)},
        )
    for label, which in [("no-bs4", "bs4"), ("no-lxml", "lxml")]:
        with _broken_web_dependency(which):
            try:
                value = {"text": _extract_text("<p>x</p>")}
            except Exception as exc:
                out.add_error(
                    f"tools_web/extract/{label}",
                    "_extract_text",
                    {"missing": which},
                    str(exc),
                )
            else:
                out.add(
                    f"tools_web/extract/{label}",
                    "_extract_text",
                    {"missing": which},
                    value,
                )


def _tool_cases(out: FixtureFile, home: Path) -> None:
    import httpx

    from ezagent.agent.tools_web import build_web_tools

    provider = _Provider(
        [
            {"title": "Uno", "url": "https://uno.test", "snippet": "primero"},
            {"title": "Dos", "url": "https://dos.test", "snippet": "segundo"},
        ]
    )
    config = _config(home, max_fetch_chars=120)
    tools = {tool.name: tool for tool in build_web_tools(config, provider=provider)}
    search = tools["web_search"]
    fetch = tools["fetch_page"]

    results = []
    infos = []
    calls = []
    for args in [
        {"query": "consulta"},
        {"query": "  consulta  ", "max_results": 1},
        {"query": ""},
        {"query": "x", "max_results": 20},
        {"query": "x", "max_results": True},
        {"max_results": 3},
    ]:
        results.append(search.handler(args).to_dict())
        infos.append(asdict(search.describe(args)))
        calls.append(provider.calls[-1] if provider.calls else None)
    out.add(
        "tools_web/build_web_tools/web_search",
        "build_web_tools",
        {"tool": "web_search", "args": "varios", "provider": "doble"},
        {"results": results, "infos": infos, "calls": calls},
    )

    requests: list[str] = []
    mapping = {"example.test": [PUBLIC]}

    def serve(request: httpx.Request) -> httpx.Response:
        requests.append(str(request.url))
        return _page(200, "<p>" + "z" * 200 + "</p>")

    original_transport = httpx.HTTPTransport
    httpx.HTTPTransport = lambda *args, **kwargs: httpx.MockTransport(serve)  # type: ignore[assignment]
    with _dns(mapping):
        try:
            fetch_results = []
            fetch_infos = []
            for args in [
                {"url": "http://example.test/x"},
                {"url": "http://example.test/x", "max_chars": 10},
                {"url": "http://example.test/x", "max_chars": 121},
                {"url": "http://example.test/x", "max_chars": 0},
                {"url": "http://127.0.0.1/x"},
                {"url": ""},
            ]:
                fetch_results.append(fetch.handler(args).to_dict())
                fetch_infos.append(asdict(fetch.describe(args)))
        finally:
            httpx.HTTPTransport = original_transport  # type: ignore[assignment]
    out.add(
        "tools_web/build_web_tools/fetch_page",
        "build_web_tools",
        {"tool": "fetch_page", "args": "varios", "max_fetch_chars": 120},
        {"results": fetch_results, "infos": fetch_infos, "requests": requests},
    )

    describe_cases = []
    for args in [
        {"query": "  consulta larga " + "x" * 100},
        {"query": 5},
        {},
        {"url": "  http://example.test/" + "x" * 200},
        {"url": 5},
        {},
    ]:
        describe_cases.append(
            {
                "args": args,
                "search": asdict(search.describe(args)),
                "fetch": asdict(fetch.describe(args)),
            }
        )
    out.add(
        "tools_web/build_web_tools/describe",
        "build_web_tools",
        {"args": "varios"},
        {"cases": describe_cases},
    )


def _specs(out: FixtureFile, home: Path) -> None:
    from ezagent.agent.tools_web import build_web_tools

    config = _config(home)
    tools = []
    for tool in build_web_tools(config, provider=_Provider([])):
        tools.append(
            {
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
                "level": tool.level,
                "untrusted_output": tool.untrusted_output,
                "timeout_s": tool.timeout_s,
                "schema": tool.schema(),
                "info": asdict(tool.describe({})),
            }
        )
    out.add(
        "tools_web/build_web_tools/specs",
        "build_web_tools",
        {"config": "default"},
        {"tools": tools},
    )


def _constants(out: FixtureFile) -> None:
    from ezagent.agent import search, tools_web

    out.add(
        "tools_web/constants",
        "module_constants",
        {},
        {
            "default_max_chars": tools_web.DEFAULT_MAX_CHARS,
            "max_search_results": tools_web.MAX_SEARCH_RESULTS,
            "max_query_chars": tools_web.MAX_QUERY_CHARS,
            "fetch_timeout_s": tools_web.FETCH_TIMEOUT_S,
            "max_redirects": tools_web.MAX_REDIRECTS,
            "max_fetch_bytes": tools_web.MAX_FETCH_BYTES,
            "user_agent": tools_web.USER_AGENT,
            "search_error_kind_default": search.SearchError("x").kind,
        },
    )


def generate(out: FixtureFile) -> None:
    home = Path(os.environ["HOME"])
    _constants(out)
    _specs(out, home)
    _wrap_cases(out)
    _blocked_cases(out)
    _target_cases(out, home)
    _search_cases(out)
    _ddgs_cases(out)
    _extract_cases(out)
    _fetch_cases(out, home)
    _tool_cases(out, home)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)

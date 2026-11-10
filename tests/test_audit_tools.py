"""Regresiones de la auditoría: timeout honesto, JSON, esquemas, selección,
caché de skills, rutas recargadas, IP fijada y errores de búsqueda."""

from __future__ import annotations

import ipaddress
import json
import os
import threading
import time
from pathlib import Path
from types import SimpleNamespace

import httpx
import pytest

from ezagent.agent import tools_web as tw
from ezagent.agent.registry import ToolRegistry, ToolResult, ToolSpec
from ezagent.agent.runtime import (
    BASE_TOOLS,
    AgentRuntime,
    RuntimeCallbacks,
    select_tool_names,
)
from ezagent.agent.search import DdgsSearchProvider, SearchError, SearchUnavailableError
from ezagent.agent.skills import SkillLibrary

PUBLIC_IP = ipaddress.ip_address("93.184.216.34")


# ------------------------------------------------------------------ F03
def test_timeout_es_unknown_outcome_y_no_invita_a_repetir() -> None:
    release = threading.Event()
    registry = ToolRegistry()
    registry.register(
        ToolSpec(
            name="lenta",
            description="d",
            parameters={"type": "object", "properties": {}},
            level=0,
            handler=lambda args: release.wait(10),
            timeout_s=0.05,
        )
    )
    try:
        result = registry.run("lenta", {})
    finally:
        release.set()
    assert result.ok is False and result.outcome == "unknown_outcome"
    assert "no se sabe" in result.error.lower()
    assert "no la repitas" in result.error.lower()
    assert result.to_dict()["outcome"] == "unknown_outcome"


def test_error_kind_solo_aparece_cuando_se_conoc_e_y_el_timeout_no_es_fallo_normal() -> None:
    assert ToolResult(ok=True).to_dict()["outcome"] == "completed"
    assert "error_kind" not in ToolResult(ok=True).to_dict()
    assert ToolResult(ok=False, error_kind="timeout").to_dict()["error_kind"] == "timeout"

    def boom(args):
        raise ValueError("kaboom")

    registry = ToolRegistry()
    registry.register(
        ToolSpec(
            name="explota",
            description="d",
            parameters={"type": "object", "properties": {}},
            level=0,
            handler=boom,
        )
    )
    assert registry.run("explota", {}).outcome == "failed"


# ------------------------------------------------------------------ F19
def test_el_recorte_siempre_devuelve_json_valido() -> None:
    big = ToolResult(
        ok=True,
        data={"texto": "x" * 5000, "items": [{"nota": "y" * 200} for _ in range(20)]},
    )
    for limit in (2000, 500, 120, 20):
        out = ToolRegistry.render_for_llm(big, max_chars=limit)
        parsed = json.loads(out)
        assert isinstance(parsed, dict)
        assert len(out) <= limit

    parsed = json.loads(ToolRegistry.render_for_llm(big, max_chars=500))
    assert parsed["truncated"] is True
    assert parsed["original_chars"] > 500
    assert parsed["ok"] is True
    assert "…" in json.dumps(parsed, ensure_ascii=False)
    assert ToolRegistry.render_for_llm(big, max_chars=0) == ""


def test_si_cabe_exacto_no_se_marca_como_truncado() -> None:
    result = ToolResult(ok=True, data="x" * 40)
    full = ToolRegistry.render_for_llm(result, max_chars=10_000)
    assert ToolRegistry.render_for_llm(result, max_chars=len(full)) == full
    assert json.loads(full).get("truncated") is None


# ------------------------------------------------------------------ F20
NESTED_SCHEMA = {
    "type": "object",
    "properties": {
        "filtro": {
            "type": "object",
            "properties": {
                "nombre": {"type": "string", "minLength": 2, "maxLength": 5},
                "tags": {
                    "type": "array",
                    "items": {"type": "string", "minLength": 1},
                    "minItems": 1,
                    "maxItems": 2,
                },
            },
            "required": ["nombre"],
            "additionalProperties": False,
        },
        "modo": {"type": "string", "enum": ["a", "b"]},
        "n": {"type": "integer", "minimum": 1, "maximum": 3},
    },
    "required": ["filtro"],
    "additionalProperties": False,
}

VALID_ARGS = {"filtro": {"nombre": "ab", "tags": ["x"]}, "modo": "a", "n": 2}


def _nested_registry() -> ToolRegistry:
    registry = ToolRegistry()
    registry.register(
        ToolSpec(
            name="t",
            description="d",
            parameters=NESTED_SCHEMA,
            level=0,
            handler=lambda args: ToolResult(ok=True, data=args),
        )
    )
    return registry


def test_la_validacion_anidada_acepta_un_caso_valido() -> None:
    assert _nested_registry().validate("t", VALID_ARGS) is None


@pytest.mark.parametrize(
    ("args", "where", "message"),
    [
        ({}, "filtro", "obligatorio"),
        ({"filtro": {"tags": ["x"]}}, "filtro.nombre", "obligatorio"),
        ({"filtro": {"nombre": "a"}}, "filtro.nombre", "al menos 2"),
        ({"filtro": {"nombre": "abcdef"}}, "filtro.nombre", "como máximo 5"),
        ({"filtro": {"nombre": "ok", "tags": []}}, "filtro.tags", "al menos 1"),
        ({"filtro": {"nombre": "ok", "tags": ["a", "b", "c"]}}, "filtro.tags", "como máximo 2"),
        ({"filtro": {"nombre": "ok", "tags": ["a", 2]}}, "filtro.tags[1]", "texto"),
        ({"filtro": {"nombre": "ok", "extra": 1}}, "filtro.extra", "no está permitido"),
        ({"filtro": {"nombre": "ok"}, "extra": 1}, "extra", "no está permitido"),
        ({"filtro": {"nombre": "ok"}, "modo": "c"}, "modo", "uno de"),
        ({"filtro": {"nombre": "ok"}, "n": 0}, "n", "mayor o igual"),
        ({"filtro": {"nombre": "ok"}, "n": 4}, "n", "menor o igual"),
    ],
)
def test_la_validacion_anidada_rechaza(args, where: str, message: str) -> None:
    error = _nested_registry().validate("t", args)
    assert error is not None
    assert where in error and message in error


# ------------------------------------------------------------- F16/F41
def test_la_base_es_cinco_y_la_frase_del_pdf_conserva_web_y_script() -> None:
    assert len(BASE_TOOLS) == 5
    names = select_tool_names("hoy genera el PDF del informe", scripts_loaded=True)
    assert set(BASE_TOOLS) <= set(names)
    assert {
        "fetch_page",
        "get_datetime",
        "write_file",
        "create_dir",
        "run_skill_script",
    } <= set(names)
    assert len(names) <= 14


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("¿qué hora es?", {"get_datetime"}),
        ("escribe el resumen", {"write_file", "create_dir"}),
        ("copia el informe", {"copy_path"}),
        ("abre el documento", {"open_path"}),
        ("borra la carpeta", {"delete_path"}),
        ("lista la carpeta", {"list_dir"}),
        ("usa la skill pdf", {"load_skill", "run_skill_script"}),
        ("recuerda esto en memoria", {"remember", "recall", "forget", "compact_memory"}),
    ],
)
def test_las_intenciones_suman_sin_quitar_la_base(text: str, expected: set[str]) -> None:
    names = select_tool_names(text)
    assert expected <= set(names)
    assert set(BASE_TOOLS) <= set(names)


def test_el_tope_de_catorce_y_la_memoria_opcional_se_respetan() -> None:
    text = "hoy escribe, copia, abre, borra y lista; usa la skill y recuerda en memoria"
    names = select_tool_names(text, scripts_loaded=True)
    assert len(names) == len(set(names)) <= 14
    without_memory = select_tool_names("recuerda en memoria", has_memory=False)
    assert "recall" not in without_memory and "remember" not in without_memory


@pytest.mark.parametrize(
    "text",
    [
        "convierte este informe a PDF",
        "haz una hoja de Excel con estos datos",
        "pásalo a Word",
    ],
)
def test_los_formatos_de_oficina_ofrecen_load_skill(text: str) -> None:
    assert "load_skill" in select_tool_names(text)


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("qué archivos tengo en Descargas", "list_dir"),
        ("qué hay en la carpeta", "list_dir"),
        ("muéstrame la carpeta", "list_dir"),
        ("mueve el archivo a la carpeta X", "move_path"),
        ("renombra el informe", "move_path"),
        ("deshaz lo último", "undo_last"),
        ("revierte el cambio", "undo_last"),
    ],
)
def test_las_intenciones_nuevas_ofrecen_su_herramienta(text: str, expected: str) -> None:
    names = select_tool_names(text)
    assert expected in names
    assert set(BASE_TOOLS) <= set(names) and len(names) <= 14


def test_un_seguimiento_conserva_un_turno_las_herramientas_extra(tmp_path: Path) -> None:
    root = tmp_path / "work"
    root.mkdir()
    runtime = _make_runtime(_write_permissions(tmp_path, root))
    try:
        runtime.begin_turn("crea una carpeta")
        first = {schema["function"]["name"] for schema in runtime.tools_for("crea una carpeta")}
        assert "create_dir" in first

        runtime.begin_turn("en Documentos")
        second = {schema["function"]["name"] for schema in runtime.tools_for("en Documentos")}
        assert "create_dir" in second, (
            "el seguimiento no debe perder la herramienta del turno anterior"
        )

        runtime.begin_turn("¿qué hora es?")
        third = {schema["function"]["name"] for schema in runtime.tools_for("¿qué hora es?")}
        assert "create_dir" not in third, "la conservación dura un solo turno"
    finally:
        runtime.close()


# --------------------------------------------------------------- F26/F27
def _write_skill(root: Path, name: str, description: str, stamp: float) -> None:
    folder = root / name
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / "SKILL.md"
    path.write_text(
        f"---\nname: {name}\ndescription: {description}\n---\nCuerpo.\n",
        encoding="utf-8",
    )
    os.utime(path, (stamp, stamp))


def test_skill_library_solo_relee_cuando_cambia_algo(agent_config, tmp_path: Path) -> None:
    root = Path(agent_config.skills_dir)
    root.mkdir(parents=True, exist_ok=True)
    _write_skill(root, "pdf", "primera", time.time() + 10)
    library = SkillLibrary(agent_config)
    assert library.get("pdf").description == "primera"
    assert library.refresh() is False

    _write_skill(root, "pdf", "segunda", time.time() + 20)
    assert library.refresh() is True
    assert library.get("pdf").description == "segunda"

    _write_skill(root, "otra", "nueva", time.time() + 30)
    assert library.refresh() is True
    assert library.refresh(force=True) is True


def _write_permissions(tmp_path: Path, read_root: Path) -> Path:
    path = tmp_path / "app" / "permissions.toml"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        "[general]\nmax_tool_calls_per_turn = 10\n"
        "[paths]\n"
        f"read_roots = ['{read_root.as_posix()}']\n"
        f"write_roots = ['{read_root.as_posix()}']\n",
        encoding="utf-8",
    )
    return path


class FakeHerdr:
    def list_agents(self):
        return []


def _make_runtime(
    permissions: Path,
    on_warn=None,
    on_confirm_ask=None,
) -> AgentRuntime:
    voice = SimpleNamespace(confirm_actions=False, agents={}, agent_profiles={}, default_cwd=None)
    callbacks = RuntimeCallbacks(on_warn=on_warn, on_confirm_ask=on_confirm_ask)
    return AgentRuntime(voice, FakeHerdr(), callbacks, config_path=permissions)


def test_runtime_no_reconstruye_tools_sin_cambios(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "work"
    root.mkdir()
    permissions = _write_permissions(tmp_path, root)
    runtime = _make_runtime(permissions)
    try:
        calls: list[int] = []
        monkeypatch.setattr(runtime, "_rebuild_tools", lambda: calls.append(1))
        runtime.refresh()
        assert calls == []
        stamp = time.time() + 100
        os.utime(permissions, (stamp, stamp))
        runtime.refresh()
        assert calls == [1]
    finally:
        runtime.close()


def test_runtime_reconstruye_cuando_cambia_una_skill(tmp_path: Path, monkeypatch) -> None:
    root = tmp_path / "work"
    root.mkdir()
    permissions = _write_permissions(tmp_path, root)
    runtime = _make_runtime(permissions)
    try:
        _write_skill(Path(runtime.config.skills_dir), "nueva", "desc", time.time() + 10)
        calls: list[int] = []
        monkeypatch.setattr(runtime, "_rebuild_tools", lambda: calls.append(1))
        runtime.refresh()
        assert calls == [1]
    finally:
        runtime.close()


def test_cambiar_read_roots_avisa_y_conserva_las_antiguas(tmp_path: Path) -> None:
    old_root = tmp_path / "viejo"
    old_root.mkdir()
    new_root = tmp_path / "nuevo"
    new_root.mkdir()
    permissions = _write_permissions(tmp_path, old_root)
    warnings: list[str] = []
    runtime = _make_runtime(permissions, on_warn=warnings.append)
    try:
        old_roots = list(runtime.config.paths.read_roots)
        permissions.write_text(
            "[general]\nmax_tool_calls_per_turn = 10\n"
            "[paths]\n"
            f"read_roots = ['{new_root.as_posix()}']\n"
            f"write_roots = ['{old_root.as_posix()}']\n",
            encoding="utf-8",
        )
        stamp = time.time() + 200
        os.utime(permissions, (stamp, stamp))
        runtime.refresh()
        assert runtime.config.paths.read_roots == old_roots
        assert any("reinicia" in message.lower() for message in warnings)
    finally:
        runtime.close()


def test_el_runtime_guarda_error_kind_en_la_auditoria(tmp_path: Path) -> None:
    root = tmp_path / "work"
    root.mkdir()
    permissions = _write_permissions(tmp_path, root)
    runtime = _make_runtime(permissions)
    try:
        runtime.registry.register(
            ToolSpec(
                name="falla",
                description="d",
                parameters={"type": "object", "properties": {}},
                level=0,
                handler=lambda args: ToolResult(ok=False, error="x", error_kind="timeout"),
            )
        )
        runtime.begin_turn("prueba")
        assert runtime.execute("falla", {}).error_kind == "timeout"
        rows = [
            json.loads(line) for line in runtime.config.audit_path.read_text("utf-8").splitlines()
        ]
        record = next(row for row in rows if row["tool"] == "falla")
        assert record["error_kind"] == "timeout"
    finally:
        runtime.close()


# ------------------------------------------------------------------ F02/F48
class FakeConfirm:
    def __init__(self) -> None:
        self.pending = True
        self.pending_token = "tok-1"
        self.armed = 0
        self.submitted: list[tuple[str, str | None]] = []

    def ask(self, summary, *, strong=False, require_number=None) -> bool:
        return True

    def submit_text(self, text, token=None):
        self.submitted.append((text, token))
        return True

    def submit_click(self, yes) -> None:
        pass

    def cancel(self, reason: str = "") -> None:
        pass

    def arm(self) -> None:
        self.armed += 1


def test_confirmacion_expone_token_y_mantiene_callbacks_de_dos_argumentos(
    tmp_path: Path,
) -> None:
    seen_two: list[tuple[str, bool]] = []
    seen_three: list[tuple[str, bool, str | None]] = []
    permissions = _write_permissions(tmp_path, tmp_path / "work")
    runtime = _make_runtime(
        permissions,
        on_confirm_ask=lambda summary, strong: seen_two.append((summary, strong)),
    )
    try:
        runtime._confirm_ask("¿Sigo?", False)
        assert seen_two == [("¿Sigo?", False)]

        runtime.confirm = FakeConfirm()
        runtime.callbacks.on_confirm_ask = lambda summary, strong, token: seen_three.append(
            (summary, strong, token)
        )
        assert runtime.pending_confirmation_token == "tok-1"
        runtime._confirm_ask("¿Sigo?", True)
        assert seen_three == [("¿Sigo?", True, "tok-1")]

        runtime.arm_confirmation()
        assert runtime.confirm.armed == 1
        assert runtime.submit_confirmation("sí", "tok-1") is True
        assert runtime.confirm.submitted == [("sí", "tok-1")]
    finally:
        runtime.close()


def test_el_token_real_de_confirmacion_llega_al_callback(tmp_path: Path) -> None:
    permissions = _write_permissions(tmp_path, tmp_path / "work")
    tokens: list[str | None] = []
    runtime = _make_runtime(
        permissions,
        on_confirm_ask=lambda summary, strong, token: tokens.append(token),
    )
    answer: list[bool] = []
    thread = threading.Thread(
        target=lambda: answer.append(runtime.confirm.ask("¿Sigo?")), daemon=True
    )
    try:
        thread.start()
        deadline = time.monotonic() + 2
        while runtime.pending_confirmation_token is None and time.monotonic() < deadline:
            time.sleep(0.01)
        token = runtime.pending_confirmation_token
        assert token and tokens == [token]
        assert runtime.submit_confirmation("sí", token) is True
        thread.join(2)
        assert answer == [True]
        assert runtime.pending_confirmation_token is None
    finally:
        runtime.close()


# ------------------------------------------------------------------ F36
def test_pinned_target_conserva_host_y_sni(agent_config, monkeypatch) -> None:
    calls: list[tuple[str, int]] = []

    def resolve(host: str, port: int):
        calls.append((host, port))
        return [PUBLIC_IP]

    monkeypatch.setattr(tw, "_resolve_ips", resolve)
    connect, headers, extensions = tw._pinned_request_target(
        "https://example.test:8443/a?b=1#frag", agent_config
    )
    assert calls == [("example.test", 8443)]
    assert connect == "https://93.184.216.34:8443/a?b=1"
    assert headers == {"Host": "example.test:8443"}
    assert extensions == {"sni_hostname": "example.test"}

    connect, headers, extensions = tw._pinned_request_target("https://example.test/a", agent_config)
    assert connect == "https://93.184.216.34/a"
    assert headers == {"Host": "example.test"}
    assert extensions == {"sni_hostname": "example.test"}

    connect, headers, extensions = tw._pinned_request_target("http://example.test/a", agent_config)
    assert connect == "http://93.184.216.34/a"
    assert headers == {"Host": "example.test"}
    assert extensions == {}

    connect, headers, extensions = tw._pinned_request_target("https://españa.test/a", agent_config)
    assert headers == {"Host": "xn--espaa-rta.test"}
    assert extensions == {"sni_hostname": "xn--espaa-rta.test"}


def test_pinned_target_bloquea_la_ip_privada_resuelta(agent_config, monkeypatch) -> None:
    monkeypatch.setattr(tw, "_resolve_ips", lambda host, port: [ipaddress.ip_address("10.0.0.5")])
    with pytest.raises(tw.UnsafeUrlError):
        tw._pinned_request_target("https://interno.test/", agent_config)


def test_el_transporte_de_pruebas_conserva_la_url_original(agent_config, monkeypatch) -> None:
    monkeypatch.setattr(tw, "_resolve_ips", lambda host, port: [PUBLIC_IP])
    seen: list[tuple[str, str | None]] = []

    def handler(request: httpx.Request) -> httpx.Response:
        seen.append((str(request.url), request.headers.get("host")))
        return httpx.Response(200, html="<p>hola</p>")

    result = tw.run_fetch_page(
        agent_config, "http://example.test/p", transport=httpx.MockTransport(handler)
    )
    assert result.ok
    assert seen == [("http://example.test/p", "example.test")]


def test_la_descarga_de_produccion_usa_la_ip_validada(agent_config, monkeypatch) -> None:
    monkeypatch.setattr(tw, "_resolve_ips", lambda host, port: [PUBLIC_IP])
    seen: list[tuple[str, str | None, str | None]] = []

    class CaptureTransport(httpx.BaseTransport):
        def handle_request(self, request: httpx.Request) -> httpx.Response:
            seen.append(
                (
                    str(request.url),
                    request.headers.get("host"),
                    request.extensions.get("sni_hostname"),
                )
            )
            return httpx.Response(200, html="<p>ok</p>")

    monkeypatch.setattr(tw.httpx, "HTTPTransport", lambda: CaptureTransport())
    result = tw.run_fetch_page(agent_config, "https://example.test/p")
    assert result.ok
    assert seen == [("https://93.184.216.34/p", "example.test", "example.test")]


# ------------------------------------------------------------------ F47
class FakeProvider:
    def __init__(self, results=None, error: Exception | None = None) -> None:
        self.results = [] if results is None else results
        self.error = error

    def search(self, query: str, max_results: int = 5):
        if self.error is not None:
            raise self.error
        return self.results


def test_los_errores_de_busqueda_se_clasifican() -> None:
    assert tw.run_web_search(FakeProvider([]), "nada").error_kind == "empty"
    assert (
        tw.run_web_search(FakeProvider(error=SearchUnavailableError("sin ddgs")), "x").error_kind
        == "unavailable"
    )
    assert (
        tw.run_web_search(FakeProvider(error=SearchError("lento", kind="timeout")), "x").error_kind
        == "timeout"
    )
    assert tw.run_web_search(FakeProvider(error=RuntimeError("boom")), "x").error_kind == "provider"


def test_el_motor_ddgs_se_reutiliza_entre_busquedas(monkeypatch) -> None:
    class FakeDDGS:
        created = 0

        def __init__(self, **kwargs) -> None:
            type(self).created += 1

        def text(self, query: str, **kwargs):
            return [{"title": "t", "href": "https://a.test", "body": "b"}]

    provider = DdgsSearchProvider()
    monkeypatch.setattr(provider, "_load_ddgs", lambda: FakeDDGS)
    assert provider.search("uno")[0]["url"] == "https://a.test"
    assert provider.search("dos")[0]["url"] == "https://a.test"
    assert FakeDDGS.created == 1


def test_un_timeout_del_motor_se_clasifica(monkeypatch) -> None:
    class TimeoutException(Exception):
        pass

    class BoomDDGS:
        def __init__(self, **kwargs) -> None:
            pass

        def text(self, query: str, **kwargs):
            raise TimeoutException("slow")

    provider = DdgsSearchProvider()
    monkeypatch.setattr(provider, "_load_ddgs", lambda: BoomDDGS)
    with pytest.raises(SearchError) as info:
        provider.search("x")
    assert info.value.kind == "timeout"

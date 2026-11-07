"""Pruebas del núcleo agéntico F1: config, rutas, política, auditoría, registro,
bucle y herramientas básicas.

Todas las pruebas escriben solo dentro de ``tmp_path``. Las que necesitan
enlaces simbólicos o junctions se saltan si el sistema no permite crearlos.
"""

from __future__ import annotations

import json
import logging
import os
import time
from datetime import datetime
from pathlib import Path
from typing import Any

import pytest

from ezagent.agent.audit import AuditLog
from ezagent.agent.config import (
    DEFAULT_PERMISSIONS_TOML,
    AgentConfig,
    ConfirmConfig,
    GeneralConfig,
    PathsConfig,
    SkillsConfig,
    ToolsDConfig,
    WebConfig,
)
from ezagent.agent.loop import AssistantReply, ToolCall, run_turn
from ezagent.agent.paths import PathDenied, resolve_safe
from ezagent.agent.policy import Decision, decide
from ezagent.agent.registry import (
    ToolInfo,
    ToolRegistry,
    ToolResult,
    ToolSpec,
)
from ezagent.agent.tools_core import build_core_tools


# ---------------------------------------------------------------------------
# Utilidades de prueba
# ---------------------------------------------------------------------------


def make_config(
    tmp_path: Path,
    *,
    read_roots: list[Path] | None = None,
    deny: list[Path] | None = None,
    deny_globs: list[str] | None = None,
) -> AgentConfig:
    """Configuración completa apuntando a carpetas de tmp_path."""
    app_dir = tmp_path / "app"
    workspace = tmp_path / "workspace"
    read_root = tmp_path / "read"
    read_root.mkdir(parents=True, exist_ok=True)
    workspace.mkdir(parents=True, exist_ok=True)
    roots = read_roots if read_roots is not None else [read_root, workspace]
    return AgentConfig(
        general=GeneralConfig(),
        paths=PathsConfig(
            read_roots=[str(path) for path in roots],
            write_roots=[str(workspace)],
            deny=[str(path) for path in (deny or [])],
            deny_globs=list(deny_globs or []),
        ),
        confirm=ConfirmConfig(
            accept=["sí", "si", "confirmo", "adelante", "dale"],
            reject=["no", "cancela", "para"],
        ),
        web=WebConfig(),
        skills=SkillsConfig(trusted=[]),
        tools_d=ToolsDConfig(allowed_executables=["code"]),
        app_dir=app_dir,
        workspace=workspace,
        permissions_path=app_dir / "permissions.toml",
        system_md_path=app_dir / "system.md",
        memory_dir=app_dir / "memory",
        history_path=app_dir / "memory" / "history.db",
        audit_path=app_dir / "logs" / "audit.jsonl",
        journal_dir=app_dir / "journal",
        skills_dir=app_dir / "skills",
        global_skills_dir=tmp_path / "global_skills",
        tools_d_dir=app_dir / "tools.d",
    )


def make_tool(
    name: str = "ping",
    *,
    handler: Any = None,
    level: int = 0,
    untrusted_output: bool = False,
    describe_fn: Any = None,
    timeout_s: float = 10.0,
    parameters: dict[str, Any] | None = None,
) -> ToolSpec:
    if handler is None:

        def handler(args: dict[str, Any]) -> ToolResult:
            return ToolResult(ok=True, data=args)

    return ToolSpec(
        name=name,
        description="Herramienta de prueba",
        parameters=parameters or {"type": "object", "properties": {}},
        level=level,
        handler=handler,
        untrusted_output=untrusted_output,
        timeout_s=timeout_s,
        describe_fn=describe_fn,
    )


class SequenceLLM:
    def __init__(self, replies: list[AssistantReply]) -> None:
        self.replies = list(replies)
        self.calls = 0

    def chat(
        self, messages: list[dict[str, Any]], tools: list[dict[str, Any]]
    ) -> AssistantReply:
        reply = self.replies[min(self.calls, len(self.replies) - 1)]
        self.calls += 1
        return reply


class RepeatingLLM:
    def __init__(self, tool_name: str = "ping") -> None:
        self.tool_name = tool_name
        self.calls = 0

    def chat(
        self, messages: list[dict[str, Any]], tools: list[dict[str, Any]]
    ) -> AssistantReply:
        self.calls += 1
        return AssistantReply(
            text="",
            tool_calls=[
                ToolCall(
                    id=f"call-{self.calls}",
                    name=self.tool_name,
                    arguments={},
                )
            ],
        )


class FakeConfirm:
    def __init__(self, answer: bool = True) -> None:
        self.answer = answer
        self.asks: list[tuple[str, bool, int | None]] = []

    def ask(
        self,
        summary: str,
        *,
        strong: bool = False,
        require_number: int | None = None,
    ) -> bool:
        self.asks.append((summary, strong, require_number))
        return self.answer


def audit_records(audit: AuditLog) -> list[dict[str, Any]]:
    return [
        json.loads(line)
        for line in audit.path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]


# ---------------------------------------------------------------------------
# config.py
# ---------------------------------------------------------------------------


def test_load_creates_default_template(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("APPDATA", str(tmp_path / "appdata"))
    monkeypatch.delenv("XDG_CONFIG_HOME", raising=False)
    config = AgentConfig.load()

    assert config.app_dir == tmp_path / "appdata" / "EzAgent"
    assert config.permissions_path == config.app_dir / "permissions.toml"
    assert config.permissions_path.exists()
    assert config.workspace == Path.home() / "EzAgentWorkspace"
    assert config.global_skills_dir == Path.home() / ".agents" / "skills"
    assert config.audit_path == config.app_dir / "logs" / "audit.jsonl"
    assert config.history_path == config.memory_dir / "history.db"
    assert config.tools_d_dir == config.app_dir / "tools.d"

    import tomllib

    data = tomllib.loads(config.permissions_path.read_text(encoding="utf-8"))
    assert data["general"]["max_tool_calls_per_turn"] == 4
    assert data["general"]["confirm_timeout_s"] == 15
    assert data["general"]["bulk_threshold"] == 5
    assert data["general"]["max_file_mb"] == 10
    assert data["paths"]["read_roots"][0] == "~/Documents"
    assert data["web"]["block_private_ips"] is True
    assert data["skills"]["allow_scripts"] is False
    assert data["tools_d"]["allowed_executables"] == ["code"]
    assert config.general.max_tool_calls_per_turn == 4
    assert config.general.confirm_timeout_s == 15.0
    assert config.paths.read_roots[0] == "~/Documents"
    assert config.confirm.accept[0] == "sí"
    assert config.skills.allow_scripts is False
    assert config.skills.trusted == []
    assert config.tools_d.allowed_executables == ["code"]


def test_default_template_has_no_destructive_toggle():
    assert "destructive_always_confirm" not in DEFAULT_PERMISSIONS_TOML


def test_load_custom_path_uses_parent_as_app_dir(tmp_path: Path):
    path = tmp_path / "perfil" / "permissions.toml"
    config = AgentConfig.load(path)
    assert path.exists()
    assert config.app_dir == path.parent
    assert config.permissions_path == path


def test_load_invalid_toml_falls_back(
    tmp_path: Path, caplog: pytest.LogCaptureFixture
):
    path = tmp_path / "permissions.toml"
    path.write_text("esto no es toml [", encoding="utf-8")
    with caplog.at_level(logging.WARNING):
        config = AgentConfig.load(path)
    assert config.general.max_tool_calls_per_turn == 4
    assert config.paths.read_roots == [
        "~/Documents",
        "~/Desktop",
        "~/Downloads",
        "~/EzAgentWorkspace",
    ]
    assert "valores por defecto" in caplog.text
    assert path.read_text(encoding="utf-8") == "esto no es toml ["


def test_load_unknown_keys_and_bad_types_warn(
    tmp_path: Path, caplog: pytest.LogCaptureFixture
):
    path = tmp_path / "permissions.toml"
    path.write_text(
        """
[general]
max_tool_calls_per_turn = "cuatro"
destructive_always_confirm = true

[paths]
read_roots = "no-es-una-lista"

[desconocida]
x = 1

[web]
block_private_ips = "sí"
""",
        encoding="utf-8",
    )
    with caplog.at_level(logging.WARNING):
        config = AgentConfig.load(path)
    assert config.general.max_tool_calls_per_turn == 4
    assert config.paths.read_roots[0] == "~/Documents"
    assert config.web.block_private_ips is True
    assert "max_tool_calls_per_turn" in caplog.text
    assert "destructive_always_confirm" in caplog.text
    assert "desconocida" in caplog.text
    assert "block_private_ips" in caplog.text


def test_load_reads_valid_overrides(tmp_path: Path):
    path = tmp_path / "permissions.toml"
    path.write_text(
        """
[general]
max_tool_calls_per_turn = 2
confirm_timeout_s = 3
bulk_threshold = 7
max_file_mb = 2.5

[paths]
read_roots = ['C:/datos']
write_roots = ['C:/salida']
deny = ['C:/privado']
deny_globs = ['*.log']

[confirm]
accept = ['vale']
reject = ['nope']

[web]
max_fetch_chars = 100
block_private_ips = false

[skills]
allow_scripts = true
trusted = ['skill-x']

[tools_d]
allowed_executables = ['notepad']
""",
        encoding="utf-8",
    )
    config = AgentConfig.load(path)
    assert config.general.max_tool_calls_per_turn == 2
    assert config.general.confirm_timeout_s == 3.0
    assert config.general.bulk_threshold == 7
    assert config.general.max_file_mb == 2.5
    assert config.paths.read_roots == ["C:/datos"]
    assert config.paths.write_roots == ["C:/salida"]
    assert config.paths.deny == ["C:/privado"]
    assert config.paths.deny_globs == ["*.log"]
    assert config.confirm.accept == ["vale"]
    assert config.confirm.reject == ["nope"]
    assert config.web.max_fetch_chars == 100
    assert config.web.block_private_ips is False
    assert config.skills.allow_scripts is True
    assert config.skills.trusted == ["skill-x"]
    assert config.tools_d.allowed_executables == ["notepad"]


def test_reload_picks_up_changes(tmp_path: Path):
    path = tmp_path / "permissions.toml"
    path.write_text("[general]\nmax_tool_calls_per_turn = 3\n", encoding="utf-8")
    config = AgentConfig.load(path)
    assert config.general.max_tool_calls_per_turn == 3
    path.write_text("[general]\nmax_tool_calls_per_turn = 6\n", encoding="utf-8")
    reloaded = config.reload()
    assert reloaded.general.max_tool_calls_per_turn == 6
    assert reloaded.permissions_path == path


def test_ensure_dirs_creates_layout(tmp_path: Path):
    config = make_config(tmp_path)
    config.ensure_dirs()
    assert config.app_dir.is_dir()
    assert config.workspace.is_dir()
    assert config.memory_dir.is_dir()
    assert config.audit_path.parent.is_dir()
    assert config.journal_dir.is_dir()
    assert config.skills_dir.is_dir()
    assert config.tools_d_dir.is_dir()
    # system.md lo crea la fase de prompt, no ensure_dirs.
    assert not config.system_md_path.exists()


# ---------------------------------------------------------------------------
# paths.py — ST-01 a ST-07
# ---------------------------------------------------------------------------


def test_resolve_safe_accepts_relative_and_absolute_inside_root(tmp_path: Path):
    root = tmp_path / "root"
    root.mkdir()
    relative = resolve_safe("sub/archivo.txt", root, [str(root)], [], [])
    assert relative == (root / "sub" / "archivo.txt").resolve()
    absolute = resolve_safe(str(root / "otro.txt"), root, [str(root)], [], [])
    assert absolute == (root / "otro.txt").resolve()


@pytest.mark.parametrize(
    "raw",
    [
        "..\\..\\Windows\\System32",
        "../../Windows/System32",
        "..\\..\\..\\Windows\\System32\\drivers",
        "../otra-carpeta/archivo.txt",
    ],
)
def test_st01_parent_traversal_rejected(tmp_path: Path, raw: str):
    root = tmp_path / "root"
    root.mkdir()
    with pytest.raises(PathDenied, match="fuera de las carpetas permitidas"):
        resolve_safe(raw, root, [str(root)], [], [])


def test_st01_absolute_outside_rejected(tmp_path: Path):
    root = tmp_path / "root"
    root.mkdir()
    outside = tmp_path / "outside" / "archivo.txt"
    with pytest.raises(PathDenied, match="fuera de las carpetas permitidas"):
        resolve_safe(str(outside), root, [str(root)], [], [])


def _create_link(link: Path, target: Path) -> bool:
    try:
        os.symlink(str(target), str(link), target_is_directory=True)
        return True
    except OSError:
        pass
    try:
        import _winapi

        _winapi.CreateJunction(str(target), str(link))
        return True
    except Exception:
        return False


def test_st02_link_pointing_outside_rejected(tmp_path: Path):
    root = tmp_path / "root"
    root.mkdir()
    outside = tmp_path / "outside"
    outside.mkdir()
    (outside / "secreto.txt").write_text("dato", encoding="utf-8")
    link = root / "enlace"
    if not _create_link(link, outside):
        pytest.skip("el sistema no permite crear enlaces simbólicos ni junctions")
    with pytest.raises(PathDenied, match="fuera de las carpetas permitidas"):
        resolve_safe(str(link / "secreto.txt"), root, [str(root)], [], [])


def test_st03_unc_and_device_paths_rejected(tmp_path: Path):
    root = tmp_path / "root"
    root.mkdir()
    for raw in (
        r"\\servidor\recurso",
        "//servidor/recurso",
        r"\\.\C:\Windows",
        r"\\?\C:\Windows",
    ):
        with pytest.raises(PathDenied, match="UNC|dispositivo"):
            resolve_safe(raw, root, [str(root)], [], [])


@pytest.mark.parametrize("name", ["CON", "NUL", "COM1", "lpt1.txt", "Aux.log"])
def test_st04_reserved_names_rejected(tmp_path: Path, name: str):
    root = tmp_path / "root"
    root.mkdir()
    with pytest.raises(PathDenied, match="reservado"):
        resolve_safe(str(root / name), root, [str(root)], [], [])


def test_st05_alternate_data_streams_rejected(tmp_path: Path):
    root = tmp_path / "root"
    root.mkdir()
    with pytest.raises(PathDenied, match="flujos"):
        resolve_safe(str(root / "a.txt:stream"), root, [str(root)], [], [])
    with pytest.raises(PathDenied, match="flujos"):
        resolve_safe("a.txt:stream", root, [str(root)], [], [])


def test_st06_drive_root_rejected(tmp_path: Path):
    anchor = Path(tmp_path.anchor)
    if not anchor.anchor:
        pytest.skip("sin raíz de unidad en este sistema")
    with pytest.raises(PathDenied, match="raíz de unidad"):
        resolve_safe(str(anchor), anchor, [str(anchor)], [], [])


def test_st07_deny_and_globs(tmp_path: Path):
    root = tmp_path / "root"
    root.mkdir()
    appdata = tmp_path / "appdata" / "EzAgent"
    appdata.mkdir(parents=True)
    ssh = root / ".ssh"
    ssh.mkdir()
    protected_files = [
        root / ".env",
        root / ".env.local",
        root / "cert.pem",
        root / "id_rsa",
        root / "id_rsa.pub",
        root / "secreto.kdbx",
        ssh / "config",
        appdata / "permissions.toml",
    ]
    for item in protected_files:
        item.parent.mkdir(parents=True, exist_ok=True)
        item.write_text("x", encoding="utf-8")
    globs = ["**/.env", "**/.env.*", "**/*.pem", "**/*.key", "**/id_rsa*", "**/*.kdbx"]
    roots = [str(root), str(appdata)]
    deny = [str(appdata), str(ssh)]
    for item in protected_files:
        with pytest.raises(PathDenied, match="protegid"):
            resolve_safe(str(item), root, roots, deny, globs)
    allowed = root / "nota.txt"
    allowed.write_text("x", encoding="utf-8")
    assert resolve_safe(str(allowed), root, roots, deny, globs) == allowed.resolve()


# ---------------------------------------------------------------------------
# policy.py
# ---------------------------------------------------------------------------


def test_policy_level3_is_denied():
    assert decide(3, False, 0, False, False) == Decision(
        "deny", 3, "Nivel 3: prohibido"
    )


def test_policy_destructive_requires_confirmation():
    assert decide(2, False, 0, True, False).action == "confirm"
    strong = decide(2, False, 6, True, False)
    assert strong.action == "confirm_strong"
    assert strong.level == 2


def test_policy_bulk_over_threshold_is_strong():
    decision = decide(1, False, 6, False, False, bulk_threshold=5)
    assert decision.action == "confirm_strong"
    assert "masiva" in decision.reason
    assert decide(1, False, 5, False, False).action == "allow"


def test_policy_taint_forces_confirmation_from_level1():
    assert decide(1, True, 0, False, False).action == "confirm"
    assert decide(0, True, 0, False, False).action == "allow"


def test_policy_level2_depends_on_toggle():
    assert decide(2, False, 0, False, False).action == "allow"
    assert decide(2, False, 0, False, True).action == "confirm"


def test_policy_levels0_and1_allowed():
    assert decide(0, False, 0, False, False) == Decision("allow", 0, "Permitido")
    assert decide(1, False, 0, False, False) == Decision("allow", 1, "Permitido")


def test_policy_custom_threshold():
    assert decide(1, False, 3, False, False, bulk_threshold=2).action == (
        "confirm_strong"
    )


# ---------------------------------------------------------------------------
# audit.py — ST-14 y ST-15
# ---------------------------------------------------------------------------


def test_st14_audit_redacts_secrets(tmp_path: Path):
    audit = AuditLog(tmp_path / "audit.jsonl")
    audit.log(
        turn_id="turno-1",
        tool="escribir",
        level=1,
        args={
            "api_key": "clave-api-123",
            "token": "token-value-456",
            "password": "pass-789",
            "secret": "secreto-000",
            "headers": "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.abc",
            "nota": "usa sk-live-ABC123456789 para probar",
            "ruta": "C:/datos/nota.txt",
        },
        decision="allow",
        taint=False,
        result_ok=True,
        ms=1.5,
    )
    raw = audit.path.read_text(encoding="utf-8")
    record = json.loads(raw.splitlines()[0])
    assert "clave-api-123" not in raw
    assert "token-value-456" not in raw
    assert "pass-789" not in raw
    assert "secreto-000" not in raw
    assert "eyJhbGciOiJIUzI1NiJ9.abc" not in raw
    assert "sk-live-ABC123456789" not in raw
    assert record["args"]["api_key"] == "[REDACTED]"
    assert record["args"]["token"] == "[REDACTED]"
    assert record["args"]["password"] == "[REDACTED]"
    assert record["args"]["secret"] == "[REDACTED]"
    assert "Bearer [REDACTED]" in record["args"]["headers"]
    assert "[REDACTED]" in record["args"]["nota"]
    assert record["args"]["ruta"] == "C:/datos/nota.txt"
    assert set(record) == {
        "ts",
        "turn_id",
        "tool",
        "level",
        "args",
        "decision",
        "taint",
        "result_ok",
        "ms",
    }


def test_audit_redact_does_not_mutate_input():
    audit = AuditLog(Path("inexistente.jsonl"))
    original = {"content": "texto", "token": "abc", "lista": [{"text": "x"}]}
    redacted = audit.redact(original)
    assert original["content"] == "texto"
    assert original["token"] == "abc"
    assert redacted["content_chars"] == 5
    assert redacted["token"] == "[REDACTED]"
    assert redacted["lista"][0]["text_chars"] == 1


def test_st15_audit_never_stores_contents(tmp_path: Path):
    audit = AuditLog(tmp_path / "audit.jsonl")
    audit.log(
        turn_id="turno-2",
        tool="read_file",
        level=0,
        args={
            "path": "C:/datos/archivo.txt",
            "content": "SECRETO-CONTENIDO",
            "text": "TEXTO-DE-PRUEBA",
            "output": "SALIDA-DE-PRUEBA",
            "data": {"clave": "DATO-INTERNO"},
            "nested": {"content": "MAS-SECRETO"},
            "contador": 7,
        },
        decision="allow",
        taint=False,
        result_ok=True,
        ms=0.5,
    )
    raw = audit.path.read_text(encoding="utf-8")
    record = json.loads(raw.splitlines()[0])
    args = record["args"]
    for secret in (
        "SECRETO-CONTENIDO",
        "TEXTO-DE-PRUEBA",
        "SALIDA-DE-PRUEBA",
        "DATO-INTERNO",
        "MAS-SECRETO",
    ):
        assert secret not in raw
    assert "content" not in args
    assert "text" not in args
    assert "output" not in args
    assert "data" not in args
    assert args["content_chars"] == len("SECRETO-CONTENIDO")
    assert args["text_chars"] == len("TEXTO-DE-PRUEBA")
    assert args["output_chars"] == len("SALIDA-DE-PRUEBA")
    assert isinstance(args["data_chars"], int) and args["data_chars"] > 0
    assert args["nested"]["content_chars"] == len("MAS-SECRETO")
    assert args["path"] == "C:/datos/archivo.txt"
    assert args["contador"] == 7


def test_audit_rotation_keeps_backups(tmp_path: Path):
    audit = AuditLog(tmp_path / "audit.jsonl", max_bytes=100, backups=2)
    for index in range(3):
        audit.log(
            turn_id=f"t{index}",
            tool="ping",
            level=0,
            args={"path": "C:/datos/archivo.txt", "n": index},
            decision="allow",
            taint=False,
            result_ok=True,
            ms=0.1,
        )
    assert audit.path.exists()
    assert audit.path.with_name("audit.jsonl.1").exists()
    assert audit.path.with_name("audit.jsonl.2").exists()
    assert not audit.path.with_name("audit.jsonl.3").exists()


# ---------------------------------------------------------------------------
# registry.py
# ---------------------------------------------------------------------------


VALID_SCHEMA = {
    "type": "object",
    "properties": {
        "path": {"type": "string"},
        "mode": {"type": "string", "enum": ["a", "b"]},
        "n": {"type": "integer", "minimum": 1, "maximum": 10},
    },
    "required": ["path"],
}


def _validation_registry() -> tuple[ToolRegistry, list[dict[str, Any]]]:
    calls: list[dict[str, Any]] = []

    def handler(args: dict[str, Any]) -> dict[str, Any]:
        calls.append(args)
        return {"ok": True, "data": args}

    registry = ToolRegistry()
    registry.register(make_tool("validar", handler=handler, parameters=VALID_SCHEMA))
    return registry, calls


@pytest.mark.parametrize(
    ("args", "fragment"),
    [
        ({}, "path"),
        ({"path": 3}, "tipo"),
        ({"path": "x", "mode": "c"}, "mode"),
        ({"path": "x", "n": 0}, "mayor o igual"),
        ({"path": "x", "n": 11}, "menor o igual"),
    ],
)
def test_registry_schema_validation(args: dict[str, Any], fragment: str):
    registry, calls = _validation_registry()
    result = registry.run("validar", args)
    assert result.ok is False
    assert result.error is not None
    assert fragment in result.error
    assert calls == []


def test_registry_schema_validation_accepts_valid_args():
    registry, calls = _validation_registry()
    result = registry.run("validar", {"path": "x", "mode": "a", "n": 2})
    assert result.ok is True
    assert result.data == {"path": "x", "mode": "a", "n": 2}
    assert calls == [{"path": "x", "mode": "a", "n": 2}]


def test_registry_timeout(tmp_path: Path):
    def slow(args: dict[str, Any]) -> ToolResult:
        time.sleep(0.4)
        return ToolResult(ok=True, data="tarde")

    registry = ToolRegistry()
    registry.register(make_tool("lenta", handler=slow, timeout_s=0.05))
    started = time.perf_counter()
    result = registry.run("lenta", {})
    elapsed = time.perf_counter() - started
    assert result.ok is False
    assert result.error is not None
    assert "tiempo" in result.error
    assert elapsed < 0.35


def test_registry_normalizes_results_and_exceptions():
    registry = ToolRegistry()
    registry.register(make_tool("dict_ok", handler=lambda args: {"ok": True, "data": 5}))
    registry.register(make_tool("dict_plain", handler=lambda args: {"valor": 7}))
    registry.register(make_tool("objeto", handler=lambda args: ToolResult(ok=True, data="x")))
    registry.register(make_tool("error_dict", handler=lambda args: {"error": "falló"}))

    def explota(args: dict[str, Any]) -> ToolResult:
        raise ValueError("boom")

    registry.register(make_tool("explota", handler=explota))

    assert registry.run("dict_ok", {}) == ToolResult(ok=True, data=5)
    assert registry.run("dict_plain", {}) == ToolResult(ok=True, data={"valor": 7})
    assert registry.run("objeto", {}) == ToolResult(ok=True, data="x")
    error = registry.run("error_dict", {})
    assert error.ok is False
    assert error.error == "falló"
    thrown = registry.run("explota", {})
    assert thrown.ok is False
    assert "ValueError" in (thrown.error or "")
    unknown = registry.run("no_existe", {})
    assert unknown.ok is False
    assert "desconocida" in (unknown.error or "")


def test_registry_render_for_llm_truncates():
    result = ToolResult(ok=True, data="x" * 5000)
    text = ToolRegistry.render_for_llm(result)
    assert len(text) <= 2000
    assert text.endswith("…(truncado)")
    assert ToolRegistry.render_for_llm(result, max_chars=0) == ""
    small = ToolResult(ok=True, data={"a": 1})
    rendered = ToolRegistry.render_for_llm(small)
    assert json.loads(rendered) == {"ok": True, "data": {"a": 1}, "error": None}


def test_registry_schemas_limit_and_shape():
    registry = ToolRegistry()
    for index in range(20):
        registry.register(make_tool(f"t{index}"))
    assert len(registry.names()) == 20
    schemas = registry.schemas()
    assert len(schemas) == 14
    assert [item["function"]["name"] for item in schemas] == [
        f"t{index}" for index in range(14)
    ]
    assert len(registry.schemas(limit=5)) == 5
    selected = registry.schemas(names=["t19", "t2"], limit=1)
    assert len(selected) == 1
    assert selected[0]["function"]["name"] == "t19"
    schema = registry.schemas(limit=1)[0]
    assert schema["type"] == "function"
    assert set(schema["function"]) == {"name", "description", "parameters"}


def test_tool_describe_default_and_custom():
    default_tool = make_tool("simple", level=2)
    assert default_tool.describe({}) == ToolInfo(level=2)
    custom = make_tool(
        "custom",
        level=1,
        describe_fn=lambda args: ToolInfo(
            level=2, bulk=3, always_confirm=True, summary="resumen"
        ),
    )
    assert custom.describe({}) == ToolInfo(
        level=2, bulk=3, always_confirm=True, summary="resumen"
    )
    assert custom.schema()["function"]["parameters"] == {
        "type": "object",
        "properties": {},
    }


def test_tool_result_to_dict():
    assert ToolResult(ok=False, error="x").to_dict() == {
        "ok": False,
        "data": None,
        "error": "x",
    }


def test_registry_get_and_duplicate_warning(caplog: pytest.LogCaptureFixture):
    registry = ToolRegistry()
    first = make_tool("dup", level=0)
    second = make_tool("dup", level=1)
    registry.register(first)
    with caplog.at_level(logging.WARNING):
        registry.register(second)
    assert registry.get("dup") is second
    assert "duplicada" in caplog.text


# ---------------------------------------------------------------------------
# loop.py — ST-23, taint y confirmaciones
# ---------------------------------------------------------------------------


def test_loop_returns_text_without_tools(tmp_path: Path):
    audit = AuditLog(tmp_path / "audit.jsonl")
    messages: list[dict[str, Any]] = []
    llm = SequenceLLM([AssistantReply(text="Hola.", tool_calls=[])])
    result = run_turn(
        llm,
        messages,
        ToolRegistry(),
        confirm=None,
        audit=audit,
        turn_id="turno",
    )
    assert result == "Hola."
    assert messages == []
    assert llm.calls == 1


def test_st23_loop_stops_after_four_calls(tmp_path: Path):
    registry = ToolRegistry()
    registry.register(make_tool("ping"))
    llm = RepeatingLLM()
    audit = AuditLog(tmp_path / "audit.jsonl")
    result = run_turn(
        llm,
        [],
        registry,
        confirm=None,
        audit=audit,
        turn_id="turno",
        max_steps=4,
    )
    assert result == "Llegué al límite de pasos. ¿Seguimos?"
    assert llm.calls == 4
    assert len(audit_records(audit)) == 4

    llm_big = RepeatingLLM()
    audit_big = AuditLog(tmp_path / "audit_big.jsonl")
    result_big = run_turn(
        llm_big,
        [],
        registry,
        confirm=None,
        audit=audit_big,
        turn_id="turno",
        max_steps=10,
    )
    assert result_big == "Llegué al límite de pasos. ¿Seguimos?"
    assert llm_big.calls == 4


def test_loop_taint_persists_and_forces_confirmation(tmp_path: Path):
    executed: list[str] = []

    def read_web(args: dict[str, Any]) -> ToolResult:
        executed.append("read")
        return ToolResult(ok=True, data="contenido web")

    def write_note(args: dict[str, Any]) -> ToolResult:
        executed.append("write")
        return ToolResult(ok=True, data="nota")

    registry = ToolRegistry()
    registry.register(make_tool("read_web", handler=read_web, level=1, untrusted_output=True))
    registry.register(make_tool("write_note", handler=write_note, level=1))
    llm = SequenceLLM(
        [
            AssistantReply(
                text="",
                tool_calls=[ToolCall(id="1", name="read_web", arguments={})],
            ),
            AssistantReply(
                text="",
                tool_calls=[ToolCall(id="2", name="write_note", arguments={})],
            ),
            AssistantReply(text="Hecho.", tool_calls=[]),
        ]
    )
    confirm = FakeConfirm(answer=True)
    audit = AuditLog(tmp_path / "audit.jsonl")
    result = run_turn(
        llm,
        [],
        registry,
        confirm=confirm,
        audit=audit,
        turn_id="turno",
    )
    assert result == "Hecho."
    assert executed == ["read", "write"]
    assert len(confirm.asks) == 1
    summary, strong, require_number = confirm.asks[0]
    assert "write_note" in summary
    assert strong is False
    assert require_number is None
    records = audit_records(audit)
    assert len(records) == 2
    assert records[0]["tool"] == "read_web"
    assert records[0]["decision"] == "allow"
    assert records[0]["taint"] is True
    assert records[1]["tool"] == "write_note"
    assert records[1]["decision"] == "confirm_yes"
    assert records[1]["taint"] is True
    assert records[1]["result_ok"] is True


def test_loop_cancelled_confirmation_does_not_execute(tmp_path: Path):
    executed: list[str] = []

    def destructive(args: dict[str, Any]) -> ToolResult:
        executed.append("destructive")
        return ToolResult(ok=True)

    registry = ToolRegistry()
    registry.register(
        make_tool(
            "destructiva",
            handler=destructive,
            level=2,
            describe_fn=lambda args: ToolInfo(
                level=2, summary="Voy a sobrescribir un archivo. ¿Confirmas?"
            ),
        )
    )
    llm = SequenceLLM(
        [
            AssistantReply(
                text="",
                tool_calls=[ToolCall(id="1", name="destructiva", arguments={})],
            ),
            AssistantReply(text="Vale, no lo hago.", tool_calls=[]),
        ]
    )
    confirm = FakeConfirm(answer=False)
    audit = AuditLog(tmp_path / "audit.jsonl")
    result = run_turn(
        llm,
        [],
        registry,
        confirm=confirm,
        audit=audit,
        turn_id="turno",
        confirm_nondestructive=True,
    )
    assert result == "Vale, no lo hago."
    assert executed == []
    assert len(confirm.asks) == 1
    records = audit_records(audit)
    assert records[0]["decision"] == "confirm_no"
    assert records[0]["result_ok"] is False


def test_loop_strong_confirmation_passes_bulk_number(tmp_path: Path):
    executed: list[str] = []

    def borrar(args: dict[str, Any]) -> ToolResult:
        executed.append("borrar")
        return ToolResult(ok=True, data="borrado")

    summary = "Voy a borrar 6 elementos. ¿Confirmas?"
    registry = ToolRegistry()
    registry.register(
        make_tool(
            "borrar",
            handler=borrar,
            level=2,
            describe_fn=lambda args: ToolInfo(
                level=2, bulk=6, always_confirm=True, summary=summary
            ),
        )
    )
    llm = SequenceLLM(
        [
            AssistantReply(
                text="",
                tool_calls=[ToolCall(id="1", name="borrar", arguments={})],
            ),
            AssistantReply(text="Listo.", tool_calls=[]),
        ]
    )
    confirm = FakeConfirm(answer=True)
    audit = AuditLog(tmp_path / "audit.jsonl")
    result = run_turn(
        llm,
        [],
        registry,
        confirm=confirm,
        audit=audit,
        turn_id="turno",
        confirm_nondestructive=False,
        bulk_threshold=5,
    )
    assert result == "Listo."
    assert executed == ["borrar"]
    assert confirm.asks == [(summary, True, 6)]
    records = audit_records(audit)
    assert records[0]["decision"] == "confirm_yes"


def test_loop_level3_is_denied_without_execution(tmp_path: Path):
    executed: list[str] = []

    def prohibida(args: dict[str, Any]) -> ToolResult:
        executed.append("prohibida")
        return ToolResult(ok=True)

    registry = ToolRegistry()
    registry.register(
        make_tool(
            "prohibida",
            handler=prohibida,
            level=0,
            describe_fn=lambda args: ToolInfo(level=3, summary="prohibida"),
        )
    )
    llm = SequenceLLM(
        [
            AssistantReply(
                text="",
                tool_calls=[ToolCall(id="1", name="prohibida", arguments={})],
            ),
            AssistantReply(text="No puedo hacer eso.", tool_calls=[]),
        ]
    )
    confirm = FakeConfirm(answer=True)
    audit = AuditLog(tmp_path / "audit.jsonl")
    result = run_turn(
        llm,
        [],
        registry,
        confirm=confirm,
        audit=audit,
        turn_id="turno",
    )
    assert result == "No puedo hacer eso."
    assert executed == []
    assert confirm.asks == []
    records = audit_records(audit)
    assert records[0]["decision"] == "deny"
    assert records[0]["result_ok"] is False


def test_loop_without_confirm_manager_denies(tmp_path: Path):
    executed: list[str] = []

    def peligrosa(args: dict[str, Any]) -> ToolResult:
        executed.append("peligrosa")
        return ToolResult(ok=True)

    registry = ToolRegistry()
    registry.register(make_tool("peligrosa", handler=peligrosa, level=2))
    llm = SequenceLLM(
        [
            AssistantReply(
                text="",
                tool_calls=[ToolCall(id="1", name="peligrosa", arguments={})],
            ),
            AssistantReply(text="No hay confirmación disponible.", tool_calls=[]),
        ]
    )
    audit = AuditLog(tmp_path / "audit.jsonl")
    result = run_turn(
        llm,
        [],
        registry,
        confirm=None,
        audit=audit,
        turn_id="turno",
        confirm_nondestructive=True,
    )
    assert result == "No hay confirmación disponible."
    assert executed == []
    assert audit_records(audit)[0]["decision"] == "deny"


def test_loop_truncates_tool_results_for_llm(tmp_path: Path):
    registry = ToolRegistry()
    registry.register(
        make_tool(
            "largo",
            handler=lambda args: ToolResult(ok=True, data="x" * 5000),
        )
    )
    messages: list[dict[str, Any]] = []
    llm = SequenceLLM(
        [
            AssistantReply(
                text="",
                tool_calls=[ToolCall(id="1", name="largo", arguments={})],
            ),
            AssistantReply(text="Fin.", tool_calls=[]),
        ]
    )
    audit = AuditLog(tmp_path / "audit.jsonl")
    result = run_turn(
        llm,
        messages,
        registry,
        confirm=None,
        audit=audit,
        turn_id="turno",
    )
    assert result == "Fin."
    tool_messages = [item for item in messages if item.get("role") == "tool"]
    assert len(tool_messages) == 1
    assert len(tool_messages[0]["content"]) <= 2000
    assert messages[0]["tool_calls"][0]["type"] == "function"


# ---------------------------------------------------------------------------
# tools_core.py
# ---------------------------------------------------------------------------


def _core_registry(config: AgentConfig) -> ToolRegistry:
    registry = ToolRegistry()
    registry.register_many(build_core_tools(config))
    return registry


def test_core_tool_catalog(tmp_path: Path):
    config = make_config(tmp_path)
    tools = build_core_tools(config)
    assert {tool.name for tool in tools} == {"get_datetime", "list_dir", "read_file"}
    assert all(tool.level == 0 for tool in tools)
    by_name = {tool.name: tool for tool in tools}
    assert by_name["read_file"].untrusted_output is True
    assert by_name["list_dir"].untrusted_output is False
    assert by_name["get_datetime"].untrusted_output is False


def test_get_datetime_local(tmp_path: Path):
    config = make_config(tmp_path)
    result = _core_registry(config).run("get_datetime", {})
    assert result.ok is True
    data = result.data
    assert isinstance(data, dict)
    assert data["weekday"] in {
        "lunes",
        "martes",
        "miércoles",
        "jueves",
        "viernes",
        "sábado",
        "domingo",
    }
    assert data["human"].startswith(data["weekday"])
    parsed = datetime.fromisoformat(data["iso"])
    assert parsed.year >= 2020
    assert isinstance(data["timezone"], str) and data["timezone"]


def test_get_datetime_utc(tmp_path: Path):
    config = make_config(tmp_path)
    result = _core_registry(config).run("get_datetime", {"timezone": "UTC"})
    assert result.ok is True
    assert result.data["timezone"] == "UTC"
    assert result.data["iso"].endswith("+00:00")
    assert datetime.fromisoformat(result.data["iso"]).utcoffset().total_seconds() == 0


def test_get_datetime_unknown_timezone(tmp_path: Path):
    config = make_config(tmp_path)
    result = _core_registry(config).run(
        "get_datetime", {"timezone": "Planeta/NoExiste"}
    )
    assert result.ok is False
    assert "zona horaria" in (result.error or "")


def test_list_dir_inside_root(tmp_path: Path):
    config = make_config(tmp_path)
    root = Path(config.paths.read_roots[0])
    (root / "carpeta").mkdir()
    (root / "nota.txt").write_text("hola", encoding="utf-8")
    result = _core_registry(config).run("list_dir", {"path": str(root)})
    assert result.ok is True
    data = result.data
    assert data["count"] == 2
    assert data["truncated"] is False
    entries = {entry["name"]: entry for entry in data["entries"]}
    assert entries["carpeta"]["type"] == "dir"
    assert entries["nota.txt"]["type"] == "file"
    assert entries["nota.txt"]["size"] == 4


def test_list_dir_outside_root_is_denied(tmp_path: Path):
    config = make_config(tmp_path)
    outside = tmp_path / "outside"
    outside.mkdir()
    result = _core_registry(config).run("list_dir", {"path": str(outside)})
    assert result.ok is False
    assert "fuera de las carpetas permitidas" in (result.error or "")


def test_list_dir_truncates_at_200(tmp_path: Path):
    config = make_config(tmp_path)
    root = Path(config.paths.read_roots[0])
    many = root / "muchos"
    many.mkdir()
    for index in range(205):
        (many / f"f{index:03d}.txt").touch()
    result = _core_registry(config).run("list_dir", {"path": str(many)})
    assert result.ok is True
    assert result.data["count"] == 200
    assert result.data["truncated"] is True


def test_read_file_wraps_untrusted_content(tmp_path: Path):
    config = make_config(tmp_path)
    root = Path(config.paths.read_roots[0])
    target = root / "nota.txt"
    target.write_text("línea uno\nlínea dos", encoding="utf-8")
    result = _core_registry(config).run("read_file", {"path": str(target)})
    assert result.ok is True
    assert isinstance(result.data, str)
    assert result.data.startswith("<untrusted_content source='")
    assert "línea uno\nlínea dos" in result.data
    assert result.data.endswith("</untrusted_content>")
    assert str(target.resolve()) in result.data


def test_read_file_respects_max_chars(tmp_path: Path):
    config = make_config(tmp_path)
    root = Path(config.paths.read_roots[0])
    target = root / "letras.txt"
    target.write_text("abcdefghij", encoding="utf-8")
    result = _core_registry(config).run(
        "read_file", {"path": str(target), "max_chars": 4}
    )
    assert result.ok is True
    assert "abcd" in result.data
    assert "efgh" not in result.data


def test_read_file_replaces_invalid_utf8(tmp_path: Path):
    config = make_config(tmp_path)
    root = Path(config.paths.read_roots[0])
    target = root / "bytes.txt"
    target.write_bytes(b"caf\xe9")
    result = _core_registry(config).run("read_file", {"path": str(target)})
    assert result.ok is True
    assert "\ufffd" in result.data


def test_read_file_outside_root_is_denied(tmp_path: Path):
    config = make_config(tmp_path)
    outside = tmp_path / "outside.txt"
    outside.write_text("no", encoding="utf-8")
    result = _core_registry(config).run("read_file", {"path": str(outside)})
    assert result.ok is False
    assert "fuera de las carpetas permitidas" in (result.error or "")


def test_read_file_env_is_denied(tmp_path: Path):
    config = make_config(tmp_path, deny_globs=["**/.env"])
    root = Path(config.paths.read_roots[0])
    target = root / ".env"
    target.write_text("SECRETO=1", encoding="utf-8")
    result = _core_registry(config).run("read_file", {"path": str(target)})
    assert result.ok is False
    assert "protegido" in (result.error or "")


def test_read_file_directory_is_rejected(tmp_path: Path):
    config = make_config(tmp_path)
    root = Path(config.paths.read_roots[0])
    result = _core_registry(config).run("read_file", {"path": str(root)})
    assert result.ok is False
    assert "carpeta" in (result.error or "")


def test_st25_permissions_toml_is_unreadable(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    monkeypatch.setenv("APPDATA", str(tmp_path / "appdata"))
    monkeypatch.delenv("XDG_CONFIG_HOME", raising=False)
    config = AgentConfig.load()
    config.paths.read_roots = [str(config.app_dir)]
    secret = "SECRETO-QUE-NO-DEBE-LEERSE"
    config.permissions_path.write_text(secret, encoding="utf-8")
    registry = _core_registry(config)

    read = registry.run("read_file", {"path": str(config.permissions_path)})
    listing = registry.run("list_dir", {"path": str(config.app_dir)})

    assert read.ok is False
    assert "protegida" in (read.error or "")
    assert listing.ok is False
    assert "protegida" in (listing.error or "")
    assert secret not in json.dumps(read.to_dict(), ensure_ascii=False)
    assert secret not in json.dumps(listing.to_dict(), ensure_ascii=False)
    assert set(registry.names()) == {"get_datetime", "list_dir", "read_file"}
    assert not any(name.startswith("write") for name in registry.names())

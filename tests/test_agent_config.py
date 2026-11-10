"""AgentConfig: la política de permisos nunca tumba el arranque."""

from __future__ import annotations

import logging
from pathlib import Path

import pytest

from ezagent.agent.config import DEFAULT_PERMISSIONS_TOML, AgentConfig


def load(tmp_path: Path, toml: str | None = None) -> AgentConfig:
    path = tmp_path / "app" / "permissions.toml"
    if toml is not None:
        path.parent.mkdir(parents=True)
        path.write_text(toml, encoding="utf-8")
    return AgentConfig.load(path)


def test_sin_archivo_crea_la_plantilla_y_usa_los_defaults(tmp_path: Path) -> None:
    config = load(tmp_path)
    assert config.permissions_path.read_text("utf-8") == DEFAULT_PERMISSIONS_TOML
    assert config.general.max_tool_calls_per_turn == 4
    assert config.general.confirm_timeout_s == 15.0
    assert config.general.bulk_threshold == 5
    assert config.general.max_file_mb == 10.0
    assert config.web.max_fetch_chars == 8000 and config.web.block_private_ips is True
    assert config.skills.allow_scripts is False and config.skills.trusted == []
    assert config.tools_d.allowed_executables == ["code"]


def test_la_plantilla_escrita_coincide_con_los_defaults_en_codigo(tmp_path: Path) -> None:
    from_template = load(tmp_path / "a")
    from_empty = load(tmp_path / "b", "")
    assert from_template.paths == from_empty.paths
    assert from_template.confirm == from_empty.confirm
    assert from_template.general == from_empty.general


def test_valores_validos_se_respetan(tmp_path: Path) -> None:
    config = load(
        tmp_path,
        """
[general]
max_tool_calls_per_turn = 9
confirm_timeout_s = 2.5
bulk_threshold = 1
max_file_mb = 1
[confirm]
accept = ["vale", "ok"]
reject = ["nunca"]
[web]
max_fetch_chars = 100
block_private_ips = false
[skills]
allow_scripts = true
trusted = ["a"]
[paths]
read_roots = ["~/x"]
write_roots = ["~/y"]
deny = ["~/z"]
deny_globs = ["*.sec"]
""",
    )
    assert config.general.max_tool_calls_per_turn == 9 and config.general.confirm_timeout_s == 2.5
    assert config.general.bulk_threshold == 1 and config.general.max_file_mb == 1.0
    assert config.confirm.accept == ["vale", "ok"] and config.confirm.reject == ["nunca"]
    assert config.web.max_fetch_chars == 100 and config.web.block_private_ips is False
    assert config.skills.allow_scripts is True and config.skills.trusted == ["a"]
    assert config.paths.read_roots == ["~/x"] and config.paths.deny_globs == ["*.sec"]


@pytest.mark.parametrize(
    "toml",
    [
        "[general]\nmax_tool_calls_per_turn = 'cuatro'",
        "[general]\nmax_tool_calls_per_turn = true",
        "[general]\nconfirm_timeout_s = 'rápido'",
        "[general]\nbulk_threshold = 2.5",
        "[web]\nblock_private_ips = 'yes'",
        "[web]\nmax_fetch_chars = '8000'",
        "[confirm]\naccept = 'sí'",
        "[confirm]\nreject = [1, 2]",
        "[paths]\ndeny = 'C:/Windows'",
        "[skills]\ntrusted = [1]",
    ],
)
def test_tipos_invalidos_vuelven_al_valor_por_defecto(tmp_path: Path, toml: str) -> None:
    config = load(tmp_path, toml)
    default = load(tmp_path / "ref")
    assert config.general == default.general
    assert config.web == default.web
    assert config.confirm == default.confirm
    assert config.paths == default.paths
    assert config.skills == default.skills


def test_toml_roto_o_no_utf8_usa_los_defaults(tmp_path: Path) -> None:
    broken = load(tmp_path / "a", "[general\nmax = ")
    assert broken.general.max_tool_calls_per_turn == 4
    path = tmp_path / "b" / "permissions.toml"
    path.parent.mkdir(parents=True)
    path.write_bytes(b"\xff\xfe[general]")
    assert AgentConfig.load(path).general.max_tool_calls_per_turn == 4


def test_secciones_que_no_son_tablas_se_ignoran(tmp_path: Path) -> None:
    config = load(tmp_path, 'general = "no soy tabla"\nweb = 3')
    assert config.general.max_tool_calls_per_turn == 4 and config.web.max_fetch_chars == 8000


def test_claves_y_secciones_desconocidas_avisan_pero_no_fallan(tmp_path: Path, caplog) -> None:
    with caplog.at_level(logging.WARNING, logger="ezagent.agent.config"):
        config = load(tmp_path, "[nada]\na = 1\n[general]\nmagia = 1")
    assert config.general.max_tool_calls_per_turn == 4
    text = caplog.text
    assert "Sección desconocida" in text and "nada" in text
    assert "Clave desconocida" in text and "magia" in text


def test_destructive_always_confirm_no_existe_y_se_ignora_con_aviso(tmp_path: Path, caplog) -> None:
    with caplog.at_level(logging.WARNING, logger="ezagent.agent.config"):
        load(tmp_path, "[general]\ndestructive_always_confirm = false")
    assert "siempre pide confirmación" in caplog.text


def test_los_secretos_estan_protegidos_por_defecto(tmp_path: Path) -> None:
    config = load(tmp_path)
    for must in ("~/.ssh", "C:/Windows", "C:/Program Files"):
        assert must in config.paths.deny
    for must in ("**/.env", "**/*.pem", "**/*.key", "**/id_rsa*", "**/*.kdbx"):
        assert must in config.paths.deny_globs


def test_las_rutas_derivadas_cuelgan_de_la_carpeta_de_la_politica(tmp_path: Path) -> None:
    config = load(tmp_path)
    app = tmp_path / "app"
    assert config.app_dir == app
    for derived in (
        config.system_md_path,
        config.memory_dir,
        config.audit_path,
        config.journal_dir,
        config.skills_dir,
        config.tools_d_dir,
        config.history_path,
    ):
        assert app in derived.parents
    assert config.workspace.name == "EzAgentWorkspace"


def test_ensure_dirs_crea_las_carpetas_dentro_del_perfil_aislado(
    tmp_path: Path, _isolate_machine: Path
) -> None:
    config = load(tmp_path)
    config.ensure_dirs()
    for directory in (
        config.app_dir,
        config.memory_dir,
        config.audit_path.parent,
        config.journal_dir,
        config.skills_dir,
        config.tools_d_dir,
        config.workspace,
    ):
        assert directory.is_dir()
    assert _isolate_machine in config.workspace.parents  # nunca el perfil real
    assert not config.global_skills_dir.exists()  # compartida con otros clientes: no se crea
    config.ensure_dirs()  # idempotente

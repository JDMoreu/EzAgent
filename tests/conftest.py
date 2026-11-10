"""Salvaguardas comunes de las pruebas.

Ninguna prueba puede tocar el equipo real del usuario: ni terminales de Herdr,
ni su carpeta personal, ni %APPDATA%. Todo se redirige a ``tmp_path``.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from ezagent.agent.config import AgentConfig
from ezagent.voice.herdr import Herdr


@pytest.fixture(autouse=True)
def _isolate_machine(
    monkeypatch: pytest.MonkeyPatch, tmp_path_factory: pytest.TempPathFactory
) -> Path:
    """Aísla Herdr, el perfil de usuario y las carpetas de configuración.

    Usa una carpeta propia (no ``tmp_path``) para que las pruebas que listan su
    ``tmp_path`` no vean la del perfil falso.
    """
    machine = tmp_path_factory.mktemp("machine")

    def blocked(self: Herdr, command: list[str], timeout: float, env: dict | None = None):
        raise AssertionError(f"La prueba intentó ejecutar un comando real de Herdr: {command[:3]}")

    monkeypatch.setattr(Herdr, "_run_command", blocked)
    monkeypatch.setattr(Herdr, "_sleep", staticmethod(lambda seconds: None))

    home = machine / "home"
    home.mkdir()
    for name in ("USERPROFILE", "HOME"):
        monkeypatch.setenv(name, str(home))
    monkeypatch.setenv("APPDATA", str(machine / "appdata"))
    monkeypatch.setenv("LOCALAPPDATA", str(machine / "localappdata"))
    monkeypatch.setenv("XDG_CONFIG_HOME", str(machine / "xdg"))
    monkeypatch.setenv("MINIMAX_DATA_DIR", str(machine / "minimax"))
    monkeypatch.setenv("CODEX_HOME", str(machine / "codex"))
    monkeypatch.setenv("OPENCODE_CONFIG_DIR", str(machine / "opencode-config"))
    for name in (
        "OPENCODE_CONFIG",
        "EZ_LLM_CONFIG",
        "LLM_PROVIDER",
        "LLM_MODEL",
        "LLM_BASE_URL",
        "LLM_API_KEY",
        "LLM_API_STYLE",
    ):
        monkeypatch.delenv(name, raising=False)
    return home


@pytest.fixture
def agent_config(tmp_path: Path) -> AgentConfig:
    """Política por defecto cargada desde una carpeta temporal (sin ensure_dirs)."""
    return AgentConfig.load(tmp_path / "app" / "permissions.toml")

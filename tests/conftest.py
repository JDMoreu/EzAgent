"""Salvaguardas comunes de las pruebas."""

from __future__ import annotations

from pathlib import Path

import pytest

from ezagent.voice.herdr import Herdr


@pytest.fixture(autouse=True)
def _never_touch_real_herdr(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    """Ninguna prueba puede ejecutar `hc`/`herdr-app` reales.

    Un `hc send` real escribe en terminales vivas del usuario (incluida la
    sesión que está ejecutando las pruebas). Las pruebas deben simular
    `_json_app`, `send_agent`, `get_agent`... o `_run_command`.
    """

    def blocked(
        self: Herdr, command: list[str], timeout: float, env: dict | None = None
    ):
        raise AssertionError(
            f"La prueba intentó ejecutar un comando real de Herdr: {command[:3]}"
        )

    monkeypatch.setattr(Herdr, "_run_command", blocked)
    # Nada de escribir el estado de OpenCode en el equipo real.
    monkeypatch.setenv("LOCALAPPDATA", str(tmp_path / "localappdata"))
    monkeypatch.setenv("MINIMAX_DATA_DIR", str(tmp_path / "minimax"))
    monkeypatch.setenv("CODEX_HOME", str(tmp_path / "codex"))
    monkeypatch.setenv("OPENCODE_CONFIG_DIR", str(tmp_path / "opencode-config"))
    monkeypatch.delenv("OPENCODE_CONFIG", raising=False)
    monkeypatch.setattr(Herdr, "_sleep", staticmethod(lambda seconds: None))

"""Localización de whisper sin rutas personales (E0)."""

from __future__ import annotations


def _isolate(monkeypatch, tmp_path):
    """Nunca leer la configuración real del usuario."""
    # DECISION: basta APPDATA; XDG por si el runner no define APPDATA.
    monkeypatch.setenv("APPDATA", str(tmp_path / "appdata"))
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "xdg"))
    monkeypatch.delenv("EZAGENT_WHISPER_HOME", raising=False)
    monkeypatch.delenv("EZAGENT_WHISPER_MODELS", raising=False)


def test_whisper_home_usa_variable_de_entorno(tmp_path, monkeypatch):
    from ezagent.voice.stt import whisper_home

    _isolate(monkeypatch, tmp_path)
    home = tmp_path / "whisper"
    home.mkdir()
    monkeypatch.setenv("EZAGENT_WHISPER_HOME", str(home))
    assert whisper_home() == home


def test_whisper_home_carpeta_por_defecto(tmp_path, monkeypatch):
    from ezagent.voice.stt import whisper_home

    _isolate(monkeypatch, tmp_path)
    local = tmp_path / "localappdata"
    (local / "EzAgent" / "whisper").mkdir(parents=True)
    monkeypatch.setenv("LOCALAPPDATA", str(local))
    assert whisper_home() == local / "EzAgent" / "whisper"


def test_whisper_home_ausente(tmp_path, monkeypatch):
    from ezagent.voice.stt import whisper_home

    _isolate(monkeypatch, tmp_path)
    local = tmp_path / "localappdata"
    local.mkdir()
    monkeypatch.setenv("LOCALAPPDATA", str(local))
    assert whisper_home() is None


def test_whisper_models_dir_env(tmp_path, monkeypatch):
    from ezagent.voice.stt import whisper_models_dir

    _isolate(monkeypatch, tmp_path)
    models = tmp_path / "models"
    models.mkdir()
    monkeypatch.setenv("EZAGENT_WHISPER_MODELS", str(models))
    assert whisper_models_dir() == models


def test_whisper_models_dir_dentro_del_home(tmp_path, monkeypatch):
    from ezagent.voice.stt import whisper_models_dir

    _isolate(monkeypatch, tmp_path)
    home = tmp_path / "whisper"
    (home / "models").mkdir(parents=True)
    monkeypatch.setenv("EZAGENT_WHISPER_HOME", str(home))
    assert whisper_models_dir() == home / "models"


def test_ensure_whisper_sin_whisper_no_lanza(tmp_path, monkeypatch):
    """Sin whisper instalado la app no se cae: ensure_whisper devuelve None."""
    from ezagent.voice import app as app_module
    from ezagent.voice.config import VoiceConfig

    _isolate(monkeypatch, tmp_path)
    local = tmp_path / "localappdata"
    local.mkdir()
    monkeypatch.setenv("LOCALAPPDATA", str(local))
    monkeypatch.setattr(app_module, "_whisper_ready", lambda url, timeout=1.0: False)
    monkeypatch.setattr(app_module, "whisper_model_path", lambda model: None)

    cfg = VoiceConfig(
        whisper_url="http://127.0.0.1:8088/inference",
        auto_start_whisper=True,
    )
    assert app_module.ensure_whisper(cfg, wait_secs=0.1) is None

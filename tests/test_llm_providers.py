"""Resolución de proveedor/modelo/clave compatible con OpenAI."""

from __future__ import annotations

import json

import pytest

from ezagent.llm_providers import normalize_provider, resolve_llm


def test_default_es_opencode(tmp_path):
    cfg = resolve_llm(environ={}, config_file=tmp_path / "no-existe.json")
    assert cfg.provider == "opencode"
    assert cfg.model == "deepseek-v4.1-flash"
    assert cfg.base_url == "https://opencode.ai/inference/go/openai/v1"
    assert cfg.api_key == ""


def test_variables_por_proveedor(tmp_path):
    cfg = resolve_llm(
        environ={
            "LLM_PROVIDER": "groq",
            "GROQ_API_KEY": "gsk-test",
        },
        config_file=tmp_path / "no-existe.json",
    )
    assert cfg.provider == "groq"
    assert cfg.model == "llama-3.3-70b-versatile"
    assert cfg.base_url == "https://api.groq.com/openai/v1"
    assert cfg.api_key == "gsk-test"


def test_overrides_genericos(tmp_path):
    cfg = resolve_llm(
        environ={
            "LLM_PROVIDER": "openai",
            "OPENAI_API_KEY": "sk-test",
            "LLM_MODEL": "gpt-4.1",
            "LLM_BASE_URL": "https://proxy.example/v1",
            "LLM_API_KEY": "proxy-key",
        },
        config_file=tmp_path / "no-existe.json",
    )
    assert cfg.model == "gpt-4.1"
    assert cfg.base_url == "https://proxy.example/v1"
    assert cfg.api_key == "proxy-key"


def test_alias_claude_y_perfil_de_archivo(tmp_path):
    path = tmp_path / "llm.json"
    path.write_text(
        json.dumps(
            {
                "provider": "claude",
                "providers": {
                    "anthropic": {
                        "base_url": "https://api.anthropic.com/v1/",
                        "api_key": "sk-ant-test",
                        "model": "claude-sonnet-5-5",
                    }
                },
            }
        ),
        encoding="utf-8",
    )
    cfg = resolve_llm(environ={}, config_file=path)
    assert normalize_provider("claude") == "anthropic"
    assert cfg.provider == "anthropic"
    assert cfg.api_key == "sk-ant-test"
    assert cfg.model == "claude-sonnet-5-5"


def test_al_cambiar_de_proveedor_ignora_el_modelo_viejo_de_opencode(tmp_path):
    cfg = resolve_llm(
        environ={
            "LLM_PROVIDER": "gemini",
            "GEMINI_API_KEY": "gemini-test",
            "LLM_MODEL": "deepseek-v4.1-flash",
            "LLM_BASE_URL": "https://opencode.ai/inference/go/openai/v1",
        },
        config_file=tmp_path / "no-existe.json",
    )
    assert cfg.model == "gemini-2.5-flash"
    assert cfg.base_url.startswith("https://generativelanguage.googleapis.com/")


def test_custom_necesita_base_url(tmp_path):
    with pytest.raises(ValueError, match="LLM_BASE_URL"):
        resolve_llm(
            provider="custom",
            environ={},
            config_file=tmp_path / "no-existe.json",
        )


def test_archivo_explicito_inexistente_falla(monkeypatch):
    monkeypatch.setenv("EZ_LLM_CONFIG", "C:/no/existe/llm.json")
    with pytest.raises(ValueError, match="EZ_LLM_CONFIG"):
        resolve_llm(environ={})


def test_catalogo_y_guardado_de_perfil(tmp_path):
    from ezagent.llm_providers import (
        load_provider_catalog,
        save_provider_profile,
    )

    path = tmp_path / "llm.json"
    path.write_text(
        json.dumps(
            {
                "provider": "groq",
                "providers": {
                    "groq": {
                        "label": "Groq",
                        "base_url": "https://api.groq.com/openai/v1",
                        "model": "llama-3.3-70b-versatile",
                        "api_key": "gsk-test",
                    }
                },
            }
        ),
        encoding="utf-8",
    )
    catalog = load_provider_catalog(environ={}, config_file=path)
    assert catalog["groq"]["label"] == "Groq"
    assert catalog["groq"]["api_key"] == "gsk-test"
    assert "openai" in catalog

    save_provider_profile(
        "mi-api",
        model="modelo-propio",
        base_url="https://api.ejemplo/v1",
        api_key="clave-propia",
        config_file=path,
    )
    saved = json.loads(path.read_text(encoding="utf-8"))
    assert saved["provider"] == "mi-api"
    assert saved["providers"]["mi-api"]["base_url"] == "https://api.ejemplo/v1"
    assert saved["providers"]["groq"]["label"] == "Groq"
    resolved = resolve_llm(environ={}, config_file=path)
    assert resolved.provider == "mi-api"
    assert resolved.model == "modelo-propio"
    assert resolved.api_key == "clave-propia"


def test_catalogo_de_modelos_por_proveedor():
    from ezagent.llm_providers import provider_models

    assert any(
        value == "llama-3.3-70b-versatile"
        for value, _label in provider_models("groq")
    )
    assert any(
        value == "claude-sonnet-5-5"
        for value, _label in provider_models("claude")
    )
    assert provider_models("custom") == ()


def test_presets_nuevos_y_formato_de_api(tmp_path):
    from ezagent.llm_providers import PROVIDERS, load_provider_catalog

    for name in ("groq", "cerebras", "gemini", "opencode", "anthropic", "openai", "deepseek", "custom"):
        assert name in PROVIDERS
    catalog = load_provider_catalog(environ={"CEREBRAS_API_KEY": "csk"}, config_file=tmp_path / "x.json")
    assert catalog["cerebras"]["api_key"] == "csk"
    assert catalog["anthropic"]["api_style"] == "anthropic"
    assert catalog["groq"]["api_style"] == "openai"
    assert catalog["custom"]["base_url"] == ""

    cfg = resolve_llm(provider="anthropic", environ={}, config_file=tmp_path / "x.json")
    assert cfg.api_style == "anthropic"
    cfg = resolve_llm(
        provider="custom",
        base_url="https://api.minimax.io/anthropic",
        model="MiniMax-M2",
        api_style="anthropic",
        environ={},
        config_file=tmp_path / "x.json",
    )
    assert (cfg.api_style, cfg.model) == ("anthropic", "MiniMax-M2")


def test_perfil_personalizado_guarda_formato_y_nombre(tmp_path):
    from ezagent.llm_providers import load_provider_catalog, save_provider_profile

    path = tmp_path / "llm.json"
    save_provider_profile(
        "Minimax",
        model="MiniMax-M2",
        base_url="https://api.minimax.io/anthropic",
        api_key="mm-key",
        api_style="anthropic",
        config_file=path,
    )
    catalog = load_provider_catalog(environ={}, config_file=path)
    assert catalog["minimax"]["api_style"] == "anthropic"
    assert catalog["minimax"]["label"] == "Minimax"
    cfg = resolve_llm(environ={}, config_file=path)
    assert (cfg.provider, cfg.api_style, cfg.api_key) == ("minimax", "anthropic", "mm-key")
    # Un preset guardado conserva su nombre de fábrica aunque llm.json traiga otro.
    data = json.loads(path.read_text(encoding="utf-8"))
    data["providers"]["groq"] = {"label": "Viejo", "model": "x", "base_url": "https://api.groq.com/openai/v1"}
    path.write_text(json.dumps(data), encoding="utf-8")
    assert load_provider_catalog(environ={}, config_file=path)["groq"]["label"] == "Groq"


def test_lista_de_modelos_de_la_api(monkeypatch):
    import httpx

    from ezagent.llm_providers import fetch_models, models_url

    assert models_url("https://api.groq.com/openai/v1", "openai") == "https://api.groq.com/openai/v1/models"
    assert models_url("https://api.anthropic.com/v1", "anthropic") == "https://api.anthropic.com/v1/models"
    assert models_url("https://api.minimax.io/anthropic", "anthropic") == (
        "https://api.minimax.io/anthropic/v1/models"
    )
    seen: dict = {}

    def fake_get(url, headers=None, params=None, timeout=None):
        seen.update(url=url, headers=headers)
        return httpx.Response(
            200,
            json={
                "data": [
                    {"id": "llama-3.3-70b-versatile"},
                    {"id": "whisper-large-v3"},
                    {"id": "models/gemini-2.5-flash"},
                    {"id": "text-embedding-3-small"},
                    {"id": "llama-3.3-70b-versatile"},
                ]
            },
            request=httpx.Request("GET", url),
        )

    monkeypatch.setattr(httpx, "get", fake_get)
    models = fetch_models("https://api.groq.com/openai/v1", "gsk", "openai")
    assert models == ["gemini-2.5-flash", "llama-3.3-70b-versatile"]
    assert seen["headers"]["Authorization"] == "Bearer gsk"

    fetch_models("https://api.anthropic.com/v1", "sk-ant", "anthropic")
    assert seen["headers"]["x-api-key"] == "sk-ant"
    assert "Authorization" not in seen["headers"]

    def failing_get(url, headers=None, params=None, timeout=None):
        return httpx.Response(401, json={}, request=httpx.Request("GET", url))

    monkeypatch.setattr(httpx, "get", failing_get)
    with pytest.raises(ValueError, match="401") as info:
        fetch_models("https://api.groq.com/openai/v1", "gsk-secreta", "openai")
    assert "gsk-secreta" not in str(info.value)


def test_modelo_sin_prefijo_se_completa_si_es_inequivoco():
    from ezagent.llm_providers import match_model

    groq = ["allam-2-7b", "openai/gpt-oss-120b", "openai/gpt-oss-20b", "qwen/qwen3.8-27b"]
    assert match_model("qwen3.8-27b", groq) == "qwen/qwen3.8-27b"
    assert match_model("QWEN3.8-27B", groq) == "qwen/qwen3.8-27b"
    assert match_model("openai/gpt-oss-20b", groq) == "openai/gpt-oss-20b"
    assert match_model("modelo-que-no-existe", groq) == "modelo-que-no-existe"
    # Ambiguo: se deja como lo escribió el usuario.
    assert match_model("gpt-oss", ["a/gpt-oss", "b/gpt-oss"]) == "gpt-oss"
    assert match_model("qwen3.8-27b", []) == "qwen3.8-27b"


def test_al_llegar_la_lista_se_corrige_el_modelo_guardado(monkeypatch):
    from types import SimpleNamespace

    import ezagent.voice.app as app_module
    from ezagent.voice.app import VoiceApp

    saved: list[dict] = []
    monkeypatch.setattr(
        app_module, "save_provider_profile", lambda provider, **kw: saved.append(kw)
    )
    app = VoiceApp.__new__(VoiceApp)
    reloads: list[int] = []
    app.cfg = SimpleNamespace(
        llm_provider="groq",
        llm_model="qwen3.8-27b",
        llm_base_url="https://api.groq.com/openai/v1",
        api_key="gsk",
        llm_api_style="openai",
        save=lambda: None,
    )
    app.messenger = SimpleNamespace(reload_llm=lambda: reloads.append(1))
    models = ("openai/gpt-oss-120b", "qwen/qwen3.8-27b")
    # De otro proveedor: no toca nada.
    app._fix_model_name("opencode", models)
    assert app.cfg.llm_model == "qwen3.8-27b" and not saved
    app._fix_model_name("groq", models)
    assert app.cfg.llm_model == "qwen/qwen3.8-27b"
    assert saved[0]["model"] == "qwen/qwen3.8-27b"
    assert reloads == [1]
    assert app._saved_llm[1] == "qwen/qwen3.8-27b"

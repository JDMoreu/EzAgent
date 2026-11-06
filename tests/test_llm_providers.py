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

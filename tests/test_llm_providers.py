"""Proveedores LLM: prioridad de configuración, perfiles guardados y lista de modelos."""

from __future__ import annotations

import json
import os
from pathlib import Path

import httpx
import pytest

from ezagent import llm_providers as lp
from ezagent.llm_providers import (
    MODEL_CATALOG,
    PROVIDERS,
    fetch_models,
    is_local_endpoint,
    load_provider_catalog,
    match_model,
    models_url,
    needs_api_key,
    normalize_api_style,
    normalize_provider,
    provider_models,
    provider_spec,
    save_provider_profile,
)


@pytest.fixture
def cfg(tmp_path: Path) -> Path:
    return tmp_path / "llm.json"


def write(path: Path, data: dict) -> Path:
    path.write_text(json.dumps(data), encoding="utf-8")
    return path


def resolve(cfg: Path, env: dict | None = None, **kwargs):
    return lp.resolve_llm(environ=env or {}, config_file=cfg, **kwargs)


# ------------------------------------------------------------ normalización
@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        (None, "opencode"),
        ("", "opencode"),
        ("  ", "opencode"),
        ("OpenAI", "openai"),
        ("claude", "anthropic"),
        ("CLAUDE", "anthropic"),
        ("google", "gemini"),
        ("open-ai", "openai"),
        ("open_ai", "openai"),
        ("mi-proveedor", "mi-proveedor"),
    ],
)
def test_normalize_provider(raw, expected: str) -> None:
    assert normalize_provider(raw) == expected


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("openai", "openai"),
        ("ANTHROPIC", "anthropic"),
        (" anthropic ", "anthropic"),
        ("grpc", "openai"),
        ("", "openai"),
        (None, "openai"),
    ],
)
def test_normalize_api_style(raw, expected: str) -> None:
    assert normalize_api_style(raw) == expected


@pytest.mark.parametrize(
    ("url", "expected"),
    [
        ("http://localhost:11434/v1", True),
        ("http://127.0.0.1:1234", True),
        ("http://[::1]:8000/v1", True),
        ("http://0.0.0.0:8080", True),
        ("HTTP://LOCALHOST/v1", True),
        ("https://api.openai.com/v1", False),
        ("http://localhost.evil.com/v1", False),
        ("http://192.168.1.10/v1", False),
        ("", False),
        ("no es url", False),
    ],
)
def test_is_local_endpoint(url: str, expected: bool) -> None:
    assert is_local_endpoint(url) is expected


def test_solo_la_nube_exige_clave() -> None:
    assert needs_api_key("openai", "https://api.openai.com/v1") is True
    assert needs_api_key("openai", "http://localhost:11434/v1") is False  # Ollama
    assert needs_api_key("custom", "https://x.io") is False
    assert needs_api_key("desconocido", "https://x.io") is False


def test_cada_preset_es_coherente() -> None:
    for name, spec in PROVIDERS.items():
        assert spec.name == name
        assert spec.api_style in lp.API_STYLES
        if name != "custom":
            assert spec.base_url.startswith("https://") and spec.default_model
            assert spec.api_key_env.endswith("_API_KEY")
    assert PROVIDERS["anthropic"].api_style == "anthropic"
    assert len({s.api_key_env for n, s in PROVIDERS.items() if n != "custom"}) == len(PROVIDERS) - 1


def test_el_catalogo_de_modelos_incluye_el_modelo_por_defecto_de_cada_proveedor() -> None:
    for name, models in MODEL_CATALOG.items():
        ids = [m for m, _ in models]
        assert PROVIDERS[name].default_model in ids, name
        assert len(ids) == len(set(ids))
    assert provider_models("claude") == MODEL_CATALOG["anthropic"]
    assert provider_models("custom") == ()
    assert provider_models("nada") == ()


def test_provider_spec_acepta_alias() -> None:
    assert provider_spec("claude").name == "anthropic"
    assert provider_spec("nada") is None


# ------------------------------------------------------------ resolve_llm
def test_sin_nada_se_usa_opencode_con_sus_valores_de_fabrica(cfg: Path) -> None:
    got = resolve(cfg)
    spec = PROVIDERS["opencode"]
    assert (got.provider, got.model, got.base_url) == (
        "opencode",
        spec.default_model,
        spec.base_url,
    )
    assert got.api_key == "" and got.source == "defaults" and got.api_style == "openai"


def test_prioridad_argumentos_entorno_perfil_preset(cfg: Path) -> None:
    write(
        cfg,
        {
            "provider": "openai",
            "providers": {
                "openai": {
                    "model": "del-archivo",
                    "base_url": "https://archivo/v1",
                    "api_key": "k-archivo",
                }
            },
        },
    )
    assert resolve(cfg).model == "del-archivo"
    env = {"LLM_MODEL": "del-entorno", "LLM_BASE_URL": "https://entorno/v1", "LLM_API_KEY": "k-env"}
    got = resolve(cfg, env)
    assert (got.model, got.base_url, got.api_key) == ("del-entorno", "https://entorno/v1", "k-env")
    got = resolve(cfg, env, model="del-arg", base_url="https://arg/v1", api_key="k-arg")
    assert (got.model, got.base_url, got.api_key) == ("del-arg", "https://arg/v1", "k-arg")


def test_la_clave_cae_a_la_variable_propia_del_proveedor(cfg: Path) -> None:
    got = resolve(cfg, {"LLM_PROVIDER": "groq", "GROQ_API_KEY": "gsk"})
    assert got.provider == "groq" and got.api_key == "gsk"
    assert resolve(cfg, {"LLM_PROVIDER": "groq", "OPENAI_API_KEY": "otra"}).api_key == ""


def test_el_proveedor_del_archivo_se_respeta_y_el_del_entorno_lo_pisa(cfg: Path) -> None:
    write(cfg, {"provider": "deepseek"})
    assert resolve(cfg).provider == "deepseek"
    assert resolve(cfg, {"LLM_PROVIDER": "groq"}).provider == "groq"
    assert resolve(cfg, provider="openai").provider == "openai"


def test_valores_de_opencode_en_el_entorno_no_contaminan_a_otro_proveedor(cfg: Path) -> None:
    env = {
        "LLM_BASE_URL": "https://opencode.ai/inference/go/openai/v1/",
        "LLM_MODEL": "deepseek-v4.1-flash",
    }
    got = resolve(cfg, env, provider="openai")
    assert got.base_url == PROVIDERS["openai"].base_url
    assert got.model == PROVIDERS["openai"].default_model
    # …pero con OpenCode seleccionado sí cuentan.
    assert resolve(cfg, {"LLM_MODEL": "deepseek-v4.1-flash"}).model == "deepseek-v4.1-flash"


def test_un_alias_en_el_perfil_se_encuentra_por_nombre_canonico(cfg: Path) -> None:
    write(cfg, {"providers": {"claude": {"model": "claude-haiku-5-5"}}})
    got = resolve(cfg, provider="anthropic")
    assert got.model == "claude-haiku-5-5" and got.api_style == "anthropic"


def test_un_proveedor_desconocido_es_un_endpoint_personalizado(cfg: Path) -> None:
    write(
        cfg,
        {
            "providers": {
                "mi-nube": {
                    "base_url": "https://mi.nube/v1",
                    "model": "m1",
                    "api_style": "anthropic",
                }
            }
        },
    )
    got = resolve(cfg, provider="mi-nube")
    assert (got.base_url, got.model, got.api_style) == ("https://mi.nube/v1", "m1", "anthropic")


def test_sin_url_un_proveedor_personalizado_falla_con_mensaje_claro(cfg: Path) -> None:
    with pytest.raises(ValueError, match="necesita LLM_BASE_URL"):
        resolve(cfg, provider="custom")
    with pytest.raises(ValueError, match="necesita LLM_BASE_URL"):
        resolve(cfg, provider="algo-raro")


def test_el_estilo_de_api_sigue_su_prioridad(cfg: Path) -> None:
    write(cfg, {"providers": {"openai": {"api_style": "anthropic"}}})
    assert resolve(cfg, provider="openai").api_style == "anthropic"
    assert resolve(cfg, {"LLM_API_STYLE": "openai"}, provider="openai").api_style == "openai"
    assert resolve(cfg, provider="openai", api_style="openai").api_style == "openai"
    assert resolve(cfg, {"LLM_API_STYLE": "basura"}, provider="openai").api_style == "openai"


def test_source_indica_de_donde_viene_la_configuracion(cfg: Path) -> None:
    assert resolve(cfg).source == "defaults"
    write(cfg, {"provider": "openai"})
    assert resolve(cfg).source == "file"
    assert resolve(cfg, {"LLM_MODEL": "x"}).source == "env"


def test_la_clave_nunca_aparece_en_repr(cfg: Path) -> None:
    got = resolve(cfg, {"LLM_API_KEY": "SUPER-SECRETA"})
    assert "SUPER-SECRETA" not in repr(got) and "SUPER-SECRETA" not in str(got)


def test_los_valores_se_recortan(cfg: Path) -> None:
    got = resolve(
        cfg,
        {"LLM_MODEL": "  m  ", "LLM_API_KEY": " k \n", "LLM_BASE_URL": " https://x/v1 "},
        provider="openai",
    )
    assert (got.model, got.api_key, got.base_url) == ("m", "k", "https://x/v1")


def test_archivo_corrupto_da_un_error_legible(cfg: Path) -> None:
    cfg.write_text("{ esto no es json", encoding="utf-8")
    with pytest.raises(ValueError, match="llm.json inválido"):
        resolve(cfg)


def test_archivo_que_no_es_un_objeto_se_ignora(cfg: Path) -> None:
    cfg.write_text("[1, 2, 3]", encoding="utf-8")
    assert resolve(cfg).provider == "opencode"


def test_ez_llm_config_apuntando_a_un_archivo_inexistente_es_un_error(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("EZ_LLM_CONFIG", str(tmp_path / "no-existe.json"))
    with pytest.raises(ValueError, match="No existe EZ_LLM_CONFIG"):
        lp.resolve_llm(environ={})


def test_config_path_respeta_la_variable_y_si_no_appdata(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    appdata = Path(os.environ["APPDATA"])
    assert lp.config_path() == appdata / "EzAgent" / "llm.json"
    monkeypatch.setenv("EZ_LLM_CONFIG", str(tmp_path / "otro.json"))
    assert lp.config_path() == tmp_path / "otro.json"


# -------------------------------------------------------------- catálogo
def test_el_catalogo_trae_todos_los_presets_con_claves_del_entorno(cfg: Path) -> None:
    catalog = load_provider_catalog(environ={"GROQ_API_KEY": "gsk"}, config_file=cfg)
    assert set(PROVIDERS) <= set(catalog)
    assert catalog["groq"]["api_key"] == "gsk" and catalog["openai"]["api_key"] == ""
    assert catalog["anthropic"]["api_style"] == "anthropic"
    assert set(catalog["openai"]) == {"label", "base_url", "model", "api_key", "api_style"}


def test_perfiles_guardados_se_mezclan_sin_perder_los_valores_de_fabrica(cfg: Path) -> None:
    write(
        cfg,
        {
            "providers": {
                "openai": {"model": "gpt-x", "label": "ignorada"},
                "mi-anthropic": {
                    "label": "Mi servicio",
                    "base_url": "https://m/a",
                    "model": "m",
                    "api_style": "anthropic",
                    "api_key": "k",
                },
                "": {"model": "sin nombre"},
                "roto": "no es dict",
            }
        },
    )
    catalog = load_provider_catalog(environ={}, config_file=cfg)
    assert catalog["openai"]["model"] == "gpt-x"
    assert catalog["openai"]["base_url"] == PROVIDERS["openai"].base_url
    assert catalog["openai"]["label"] == PROVIDERS["openai"].label  # preset conserva su nombre
    mine = catalog["mi-anthropic"]
    assert (mine["label"], mine["api_style"], mine["api_key"]) == ("Mi servicio", "anthropic", "k")
    assert "roto" not in catalog and "" not in catalog


# --------------------------------------------------------------- guardado
def test_guardar_un_perfil_lo_deja_activo_y_recargable(cfg: Path) -> None:
    name = save_provider_profile(
        "Claude",
        model=" m ",
        base_url=" https://x/v1 ",
        api_key=" k ",
        api_style="anthropic",
        config_file=cfg,
    )
    assert name == "anthropic"
    data = json.loads(cfg.read_text("utf-8"))
    assert data["provider"] == "anthropic"
    assert data["providers"]["anthropic"] == {
        "label": PROVIDERS["anthropic"].label,
        "base_url": "https://x/v1",
        "model": "m",
        "api_key": "k",
        "api_style": "anthropic",
    }
    got = resolve(cfg)
    assert (got.provider, got.model, got.api_key, got.api_style) == (
        "anthropic",
        "m",
        "k",
        "anthropic",
    )


def test_guardar_conserva_los_otros_perfiles_y_la_etiqueta_personalizada(cfg: Path) -> None:
    write(cfg, {"providers": {"mi": {"label": "Mi nube", "model": "a"}, "openai": {"model": "z"}}})
    save_provider_profile("mi", model="b", base_url="https://m", api_key="", config_file=cfg)
    data = json.loads(cfg.read_text("utf-8"))
    assert data["providers"]["mi"]["label"] == "Mi nube" and data["providers"]["mi"]["model"] == "b"
    assert data["providers"]["openai"] == {"model": "z"}


def test_guardar_sobre_un_archivo_corrupto_no_lo_destruye(cfg: Path) -> None:
    cfg.write_text("{corrupto", encoding="utf-8")
    with pytest.raises(ValueError, match="llm.json inválido"):
        save_provider_profile("openai", model="m", base_url="u", api_key="k", config_file=cfg)
    assert cfg.read_text("utf-8") == "{corrupto"


def test_guardar_no_deja_temporales(cfg: Path) -> None:
    save_provider_profile("openai", model="m", base_url="https://u", api_key="k", config_file=cfg)
    assert [p.name for p in cfg.parent.iterdir()] == ["llm.json"]


def test_guardar_normaliza_un_estilo_invalido(cfg: Path) -> None:
    save_provider_profile(
        "openai", model="m", base_url="u", api_key="", api_style="raro", config_file=cfg
    )
    assert json.loads(cfg.read_text("utf-8"))["providers"]["openai"]["api_style"] == "openai"


# ------------------------------------------------------------- modelos
@pytest.mark.parametrize(
    ("model", "available", "expected"),
    [
        ("qwen3-32b", ["qwen/qwen3-32b", "otro"], "qwen/qwen3-32b"),
        ("QWEN3-32B", ["qwen/qwen3-32b"], "qwen/qwen3-32b"),
        ("Gpt-4o", ["gpt-4o"], "gpt-4o"),
        ("qwen3-32b", ["a/qwen3-32b", "b/qwen3-32b"], "qwen3-32b"),  # ambiguo: se deja igual
        ("modelo", ["modelo", "x/modelo"], "modelo"),  # coincidencia exacta gana
        ("nada", ["otro"], "nada"),
        ("", ["a"], ""),
        ("m", [], "m"),
        ("  m  ", ["x/m"], "x/m"),
    ],
)
def test_match_model(model: str, available: list[str], expected: str) -> None:
    assert match_model(model, available) == expected


@pytest.mark.parametrize(
    ("base", "style", "expected"),
    [
        ("https://api.openai.com/v1", "openai", "https://api.openai.com/v1/models"),
        ("https://api.openai.com/v1/", "openai", "https://api.openai.com/v1/models"),
        ("https://api.anthropic.com", "anthropic", "https://api.anthropic.com/v1/models"),
        ("https://api.anthropic.com/v1", "anthropic", "https://api.anthropic.com/v1/models"),
        ("https://x/anthropic", "anthropic", "https://x/anthropic/v1/models"),
        ("", "openai", "/models"),
    ],
)
def test_models_url(base: str, style: str, expected: str) -> None:
    assert models_url(base, style) == expected


class FakeGet:
    def __init__(self, response=None, error=None) -> None:
        self.response, self.error, self.calls = response, error, []

    def __call__(self, url, headers=None, params=None, timeout=None):
        self.calls.append({"url": url, "headers": headers, "params": params, "timeout": timeout})
        if self.error:
            raise self.error
        return self.response


def reply(status: int = 200, payload=None, text: str | None = None) -> httpx.Response:
    request = httpx.Request("GET", "https://x/models")
    if text is not None:
        return httpx.Response(status, text=text, request=request)
    return httpx.Response(status, json=payload, request=request)


def test_fetch_models_filtra_ordena_y_limpia_ids(monkeypatch: pytest.MonkeyPatch) -> None:
    payload = {
        "data": [
            {"id": "models/gemini-2.5-flash"},
            {"id": "gpt-4o"},
            {"id": "text-embedding-3"},
            {"id": "whisper-1"},
            {"id": "gpt-4o"},
            {"id": "dall-e-3"},
            {"id": "gpt-4o-tts"},
            {"id": ""},
            {"nombre": "sin id"},
            "claude-x",
            {"id": "llama-guard"},
        ]
    }
    fake = FakeGet(reply(payload=payload))
    monkeypatch.setattr(httpx, "get", fake)
    assert fetch_models("https://x/v1", "k") == ["claude-x", "gemini-2.5-flash", "gpt-4o"]


def test_fetch_models_acepta_una_lista_plana_y_respeta_el_limite(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(httpx, "get", FakeGet(reply(payload=[f"m{i:03d}" for i in range(200)])))
    got = fetch_models("https://x/v1", "k", limit=5)
    assert got == ["m000", "m001", "m002", "m003", "m004"]


def test_fetch_models_envia_la_cabecera_correcta_segun_el_estilo(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    fake = FakeGet(reply(payload={"data": []}))
    monkeypatch.setattr(httpx, "get", fake)
    fetch_models("https://x/v1", "clave-1")
    fetch_models("https://x", "clave-2", "anthropic")
    fetch_models("http://localhost:11434/v1", "")
    openai_call, anthropic_call, local_call = fake.calls
    assert openai_call["headers"]["Authorization"] == "Bearer clave-1"
    assert "x-api-key" not in openai_call["headers"] and openai_call["params"] is None
    assert anthropic_call["headers"]["x-api-key"] == "clave-2"
    assert anthropic_call["headers"]["anthropic-version"] == "2023-06-01"
    assert "Authorization" not in anthropic_call["headers"]
    assert anthropic_call["url"] == "https://x/v1/models" and anthropic_call["params"] == {
        "limit": "1000"
    }
    assert "Authorization" not in local_call["headers"]  # sin clave no se envía cabecera vacía


@pytest.mark.parametrize(
    ("fake", "message"),
    [
        (FakeGet(reply(400, payload={})), "respondió 400"),
        (FakeGet(reply(401, payload={})), "respondió 401"),
        (FakeGet(reply(500, payload={})), "respondió 500"),
        (FakeGet(reply(200, text="<html>")), "no es JSON"),
        (FakeGet(error=httpx.ConnectError("sin red")), "ConnectError"),
        (FakeGet(error=httpx.ReadTimeout("lento")), "ReadTimeout"),
    ],
)
def test_fetch_models_errores_sin_filtrar_la_clave(
    monkeypatch: pytest.MonkeyPatch, fake: FakeGet, message: str
) -> None:
    monkeypatch.setattr(httpx, "get", fake)
    with pytest.raises(ValueError, match=message) as info:
        fetch_models("https://x/v1", "CLAVE-PRIVADA")
    assert "CLAVE-PRIVADA" not in str(info.value)

"""STT: filtro de alucinaciones de Whisper y cliente HTTP de whisper.cpp."""

from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import httpx
import pytest

from ezagent.voice import stt
from ezagent.voice.stt import STTError, is_hallucination, transcribe


# ----------------------------------------------------------- alucinaciones
@pytest.mark.parametrize(
    "text",
    [
        "¡Gracias por ver el vídeo!",
        "Gracias por ver el video.",
        "GRACIAS POR VER",
        "Gracias por vernos",
        "Suscríbete al canal.",
        "suscribete",
        "No olvides suscribirte",
        "Hasta la próxima",
        "Subtítulos realizados por la comunidad de Amara.org",
        "amara.org",
        "Subtitulado por la comunidad de Amara.org",
    ],
)
def test_frases_tipicas_de_subtitulos_se_descartan_siempre(text: str) -> None:
    assert is_hallucination(text) is True
    assert is_hallucination(text, weak_audio=True) is True


@pytest.mark.parametrize(
    "text", ["Gracias.", "muchas gracias", "Gracias a todos", "hasta luego", "Chao"]
)
def test_cortesias_sueltas_solo_se_descartan_con_audio_debil(text: str) -> None:
    assert is_hallucination(text, weak_audio=False) is False
    assert is_hallucination(text, weak_audio=True) is True


@pytest.mark.parametrize(
    "text",
    [
        "",
        "   ",
        "...",
        "Gracias, ahora abre OpenCode",
        "Gracias por ver el vídeo que hice y revísalo",
        "abre Claude Code",
        "dime la hora",
    ],
)
def test_ordenes_reales_nunca_se_descartan(text: str) -> None:
    assert is_hallucination(text, weak_audio=True) is False


def test_una_orden_larga_que_contiene_gracias_no_es_alucinacion() -> None:
    assert not is_hallucination("Muchas gracias, ahora crea una carpeta nueva", weak_audio=True)


def test_la_comparacion_ignora_tildes_mayusculas_y_puntuacion() -> None:
    assert is_hallucination("  ¡¡SUSCRÍBETE   AL   CANAL!!  ")
    assert is_hallucination("hasta la proxima...")


# ------------------------------------------------------------ transcribe
def cfg(**overrides) -> SimpleNamespace:
    values = dict(
        whisper_url="http://127.0.0.1:8080/inference",
        stt_model="",
        stt_prompt="",
        stt_timeout_secs=5.0,
    )
    values.update(overrides)
    return SimpleNamespace(**values)


class Post:
    def __init__(self, *responses) -> None:
        self.responses = list(responses)
        self.calls: list[dict] = []

    def __call__(self, url, data=None, files=None, timeout=None):
        self.calls.append({"url": url, "data": data, "files": files, "timeout": timeout})
        outcome = self.responses.pop(0)
        if isinstance(outcome, Exception):
            raise outcome
        return outcome


def reply(status: int = 200, payload=None, text: str | None = None) -> httpx.Response:
    request = httpx.Request("POST", "http://x")
    if text is not None:
        return httpx.Response(status, text=text, request=request)
    return httpx.Response(status, json=payload, request=request)


def test_transcribe_envia_el_audio_y_devuelve_el_texto_limpio(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    post = Post(reply(payload={"text": "  abre claude code \n"}))
    monkeypatch.setattr(httpx, "post", post)
    assert transcribe(cfg(stt_prompt="EzAgent, OpenCode"), b"RIFFwav") == "abre claude code"
    (call,) = post.calls
    assert call["url"] == "http://127.0.0.1:8080/inference" and call["timeout"] == 5.0
    assert call["data"] == {
        "model": "ggml-large-v3-turbo-q5_0",
        "language": "es",
        "response_format": "json",
        "temperature": "0.00",
        "prompt": "EzAgent, OpenCode",
    }
    assert call["files"] == {"file": ("utterance.wav", b"RIFFwav", "audio/wav")}


def test_sin_prompt_no_se_envia_el_campo(monkeypatch: pytest.MonkeyPatch) -> None:
    post = Post(reply(payload={"text": "hola"}))
    monkeypatch.setattr(httpx, "post", post)
    transcribe(cfg(stt_model="ggml-base-q5_1"), b"x")
    assert (
        "prompt" not in post.calls[0]["data"] and post.calls[0]["data"]["model"] == "ggml-base-q5_1"
    )


def test_un_404_prueba_la_ruta_alternativa_de_openai(monkeypatch: pytest.MonkeyPatch) -> None:
    post = Post(reply(404, text="no"), reply(payload={"text": "ok"}))
    monkeypatch.setattr(httpx, "post", post)
    assert transcribe(cfg(), b"x") == "ok"
    assert [c["url"] for c in post.calls] == [
        "http://127.0.0.1:8080/inference",
        "http://127.0.0.1:8080/v1/audio/transcriptions",
    ]


def test_un_fallo_de_red_tambien_prueba_la_alternativa(monkeypatch: pytest.MonkeyPatch) -> None:
    post = Post(httpx.ConnectError("rechazada"), reply(payload={"text": "ok"}))
    monkeypatch.setattr(httpx, "post", post)
    assert transcribe(cfg(), b"x") == "ok"


def test_una_url_sin_alternativa_solo_se_intenta_una_vez(monkeypatch: pytest.MonkeyPatch) -> None:
    post = Post(reply(404, text="no"))
    monkeypatch.setattr(httpx, "post", post)
    with pytest.raises(STTError, match="No puedo conectar"):
        transcribe(cfg(whisper_url="http://host/custom"), b"x")
    assert len(post.calls) == 1


def test_sin_servidor_el_error_explica_como_arrancarlo(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(httpx, "post", Post(httpx.ConnectError("x"), httpx.ConnectError("y")))
    with pytest.raises(STTError) as info:
        transcribe(cfg(), b"x")
    assert "start-whisper.ps1" in str(info.value)


@pytest.mark.parametrize("status", [400, 500, 503])
def test_un_error_del_servidor_no_se_reintenta_y_trae_el_cuerpo(monkeypatch, status: int) -> None:
    post = Post(reply(status, text="modelo no cargado"))
    monkeypatch.setattr(httpx, "post", post)
    with pytest.raises(STTError, match=f"{status}.*modelo no cargado"):
        transcribe(cfg(), b"x")
    assert len(post.calls) == 1


def test_una_respuesta_que_no_es_json_se_usa_como_texto(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(httpx, "post", Post(reply(200, text=" texto plano ")))
    assert transcribe(cfg(), b"x") == "texto plano"


def test_json_sin_campo_text_da_cadena_vacia(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(httpx, "post", Post(reply(payload={"otra": 1})))
    assert transcribe(cfg(), b"x") == ""


@pytest.mark.parametrize(
    ("url", "expected"),
    [
        ("http://h:1/inference", "http://h:1/v1/audio/transcriptions"),
        ("http://h:1/inference/", "http://h:1/v1/audio/transcriptions"),
        ("http://h:1/v1/audio/transcriptions", "http://h:1/inference"),
        ("http://h:1/api/inference?x=1", "http://h:1/api/v1/audio/transcriptions?x=1"),
        ("http://h:1/otra", None),
        ("http://h:1", None),
    ],
)
def test_ruta_alternativa(url: str, expected: str | None) -> None:
    assert stt._derive_fallback(url) == expected


# ----------------------------------------------------------- modelos locales
@pytest.fixture
def models(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    home = tmp_path / "whisper"
    (home / "models").mkdir(parents=True)
    monkeypatch.setenv("EZAGENT_WHISPER_HOME", str(home))
    monkeypatch.delenv("EZAGENT_WHISPER_MODELS", raising=False)
    return home / "models"


def test_los_modelos_se_listan_en_orden_de_calidad_con_etiquetas(models: Path) -> None:
    for name in ("ggml-large-v3-turbo-q5_0", "ggml-tiny-q5_1", "mi-modelo-raro", "ggml-base-q5_1"):
        (models / f"{name}.bin").write_bytes(b"x")
    (models / "ggml-small-q5_1.txt").write_text("no es un modelo")
    listed = stt.available_whisper_models()
    assert [key for key, _ in listed] == [
        "ggml-tiny-q5_1",
        "ggml-base-q5_1",
        "ggml-large-v3-turbo-q5_0",
        "mi-modelo-raro",
    ]
    assert dict(listed)["ggml-large-v3-turbo-q5_0"].endswith("recomendada")
    assert dict(listed)["mi-modelo-raro"] == "Whisper mi modelo raro"


def test_el_modelo_actual_aparece_aunque_no_este_instalado(models: Path) -> None:
    listed = dict(stt.available_whisper_models("ggml-medium-q5_0"))
    assert "ggml-medium-q5_0" in listed


def test_whisper_model_path_acepta_nombre_con_o_sin_extension_y_rutas(
    models: Path, tmp_path: Path
) -> None:
    (models / "ggml-base-q5_1.bin").write_bytes(b"x")
    expected = models / "ggml-base-q5_1.bin"
    assert stt.whisper_model_path("ggml-base-q5_1") == expected
    assert stt.whisper_model_path("ggml-base-q5_1.bin") == expected
    outside = tmp_path / "otro.bin"
    outside.write_bytes(b"x")
    assert stt.whisper_model_path(str(outside)) == outside
    assert stt.whisper_model_path("no-existe") is None
    assert stt.whisper_model_path("") is None and stt.whisper_model_path(None) is None  # type: ignore[arg-type]


def test_whisper_home_respeta_el_entorno_y_nunca_lanza(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    folder = tmp_path / "w"
    folder.mkdir()
    monkeypatch.setenv("EZAGENT_WHISPER_HOME", str(folder))
    assert stt.whisper_home() == folder
    monkeypatch.setenv("EZAGENT_WHISPER_HOME", str(tmp_path / "no-existe"))
    monkeypatch.setattr(
        stt.VoiceConfig, "load", classmethod(lambda cls: (_ for _ in ()).throw(OSError("x")))
    )
    assert stt.whisper_home() is None  # sin carpeta válida y con la config rota

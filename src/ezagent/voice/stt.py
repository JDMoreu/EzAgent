"""STT de `ezagent-voice`: whisper.cpp por HTTP."""

from __future__ import annotations

import atexit
import os
import re
import threading
import time
import unicodedata
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit, urlunsplit

import httpx

from .config import VoiceConfig
from .stt_dictionary import correct_transcription, transcription_hint


class STTError(RuntimeError):
    pass


# F12: un cliente por proceso reutiliza conexión y TLS entre turnos. El
# endpoint que funcionó se recuerda para no repetir el 404 de la ruta alterna.
_CLIENT_LOCK = threading.Lock()
_CLIENT: httpx.Client | None = None
_WORKING_ENDPOINT: dict[str, str] = {}
_DEFAULT_POST = httpx.post


def _client() -> httpx.Client:
    global _CLIENT
    with _CLIENT_LOCK:
        if _CLIENT is None:
            _CLIENT = httpx.Client()
        return _CLIENT


def close_client() -> None:
    """Cierra el cliente persistente (atexit y pruebas)."""
    global _CLIENT
    with _CLIENT_LOCK:
        client, _CLIENT = _CLIENT, None
        _WORKING_ENDPOINT.clear()
    if client is not None:
        client.close()


atexit.register(close_client)


def _post(
    client: Any | None, url: str, *, data: dict, files: dict, timeout: float
) -> httpx.Response:
    # DECISION: honor a runtime replacement of httpx.post so the existing STT
    # tests keep their transport seam; production uses the pooled client (F12).
    if client is not None:
        return client.post(url, data=data, files=files, timeout=timeout)
    if httpx.post is not _DEFAULT_POST:
        return httpx.post(url, data=data, files=files, timeout=timeout)
    return _client().post(url, data=data, files=files, timeout=timeout)


def _endpoints(primary: str) -> list[str]:
    endpoints = [primary]
    fallback = _derive_fallback(primary)
    if fallback:
        endpoints.append(fallback)
    with _CLIENT_LOCK:
        cached = _WORKING_ENDPOINT.get(primary)
    if cached in endpoints:
        endpoints.remove(cached)
        endpoints.insert(0, cached)
    return endpoints


def _remember_endpoint(primary: str, url: str) -> None:
    with _CLIENT_LOCK:
        _WORKING_ENDPOINT[primary] = url


def _forget_endpoint(primary: str, url: str) -> None:
    with _CLIENT_LOCK:
        if _WORKING_ENDPOINT.get(primary) == url:
            del _WORKING_ENDPOINT[primary]


_WHISPER_ORDER = (
    "ggml-tiny-q5_1",
    "ggml-base-q5_1",
    "ggml-small-q5_1",
    "ggml-medium-q5_0",
    "ggml-large-v3-q5_0",
    "ggml-large-v3-turbo-q5_0",
    "ggml-large-v3-turbo-q8_0",
)
_WHISPER_LABELS = {
    "ggml-tiny-q5_1": "Whisper Tiny · la más rápida",
    "ggml-base-q5_1": "Whisper Base · rápida",
    "ggml-small-q5_1": "Whisper Small · equilibrada",
    "ggml-medium-q5_0": "Whisper Medium · precisa",
    "ggml-large-v3-q5_0": "Whisper Large v3 · máxima precisión",
    "ggml-large-v3-turbo-q5_0": "Whisper Large v3 Turbo · recomendada",
    "ggml-large-v3-turbo-q8_0": "Whisper Large v3 Turbo · alta calidad",
}


def whisper_home() -> Path | None:
    """Carpeta con whisper-server.exe y models/.

    Orden: env EZAGENT_WHISPER_HOME → cfg.whisper_home (voice.json)
    → %LOCALAPPDATA%/EzAgent/whisper → None. Nunca lanza.
    """
    try:
        explicit = os.getenv("EZAGENT_WHISPER_HOME")
        if explicit:
            root = Path(explicit).expanduser()
            if root.is_dir():
                return root
        configured = str(VoiceConfig.load().whisper_home or "").strip()
        if configured:
            root = Path(configured).expanduser()
            if root.is_dir():
                return root
        base = os.getenv("LOCALAPPDATA") or str(Path.home() / "AppData" / "Local")
        default = Path(base) / "EzAgent" / "whisper"
        return default if default.is_dir() else None
    except Exception:  # noqa: BLE001 - la autodetección nunca debe romper
        return None


def whisper_models_dir() -> Path | None:
    """Carpeta de modelos: EZAGENT_WHISPER_MODELS o ``whisper_home()/models``."""
    try:
        explicit = os.getenv("EZAGENT_WHISPER_MODELS")
        if explicit:
            root = Path(explicit).expanduser()
            if root.is_dir():
                return root
        home = whisper_home()
        if home is not None and (home / "models").is_dir():
            return home / "models"
    except Exception:  # noqa: BLE001
        pass
    return None


def whisper_model_path(model: str) -> Path | None:
    """Resuelve un nombre de modelo o una ruta a un ``.bin`` de whisper.cpp."""
    text = str(model or "").strip()
    if not text:
        return None
    direct = Path(text).expanduser()
    if direct.is_file():
        return direct
    folder = whisper_models_dir()
    if folder is None:
        return None
    for name in (text, text if text.endswith(".bin") else text + ".bin"):
        candidate = folder / name
        if candidate.is_file():
            return candidate
    return None


def available_whisper_models(current: str = "") -> tuple[tuple[str, str], ...]:
    """Modelos instalados, listos para una fila de selección del menú."""
    folder = whisper_models_dir()
    found: dict[str, str] = {}
    if folder is not None:
        for path in folder.glob("*.bin"):
            if path.is_file():
                key = path.stem
                found[key] = _WHISPER_LABELS.get(
                    key,
                    "Whisper " + key.removeprefix("ggml-").replace("-", " "),
                )
    current = str(current or "").strip()
    if current and current not in found:
        found[current] = _WHISPER_LABELS.get(current, current)
    order = {name: index for index, name in enumerate(_WHISPER_ORDER)}
    return tuple(
        (key, found[key])
        for key in sorted(
            found,
            key=lambda key: (order.get(key, len(order)), key.lower()),
        )
    )


def _derive_fallback(url: str) -> str | None:
    """Alterna entre /inference y la ruta OpenAI del mismo servidor."""
    parts = urlsplit(url)
    path = parts.path.rstrip("/")
    if path.endswith("/inference"):
        new_path = path[: -len("/inference")] + "/v1/audio/transcriptions"
    elif path.endswith("/v1/audio/transcriptions"):
        new_path = path[: -len("/v1/audio/transcriptions")] + "/inference"
    else:
        return None
    return urlunsplit((parts.scheme, parts.netloc, new_path, parts.query, ""))


def _form(cfg: VoiceConfig) -> dict[str, str]:
    form = {
        "model": cfg.stt_model or "ggml-large-v3-turbo-q5_0",
        "language": "es",
        "response_format": "json",
        "temperature": "0.00",
    }
    prompt = cfg.stt_prompt
    if getattr(cfg, "stt_vocabulary_hints", True):
        try:
            prompt = transcription_hint(
                prompt,
                str(getattr(cfg, "stt_dictionary", "") or ""),
                getattr(cfg, "agents", {}) or {},
            )
        except ValueError as exc:
            raise STTError(f"Diccionario de transcripción inválido: {exc}") from exc
    if prompt:
        form["prompt"] = prompt
    return form


def _text(cfg: VoiceConfig, response: httpx.Response) -> str:
    try:
        text = response.json().get("text", "")
    except ValueError:
        text = response.text
    try:
        return correct_transcription(
            " ".join(str(text).split()), str(getattr(cfg, "stt_dictionary", "") or "")
        )
    except ValueError as exc:
        raise STTError(f"Diccionario de transcripción inválido: {exc}") from exc


def transcribe(cfg: VoiceConfig, wav_bytes: bytes, *, client: Any | None = None) -> str:
    form = _form(cfg)
    budget = float(getattr(cfg, "stt_timeout_secs", 60.0) or 60.0)
    deadline = time.monotonic() + budget
    endpoints = _endpoints(cfg.whisper_url)
    files = {"file": ("utterance.wav", wav_bytes, "audio/wav")}
    last_error = ""
    for index, url in enumerate(endpoints):
        # Un solo presupuesto para toda la llamada: el reintento de ruta usa
        # lo que quede, no un timeout completo nuevo.
        timeout = budget if index == 0 else deadline - time.monotonic()
        if timeout <= 0:
            break
        try:
            response = _post(client, url, data=form, files=files, timeout=timeout)
        except (httpx.ConnectError, httpx.ConnectTimeout) as exc:
            _forget_endpoint(cfg.whisper_url, url)
            last_error = f"{url}: {exc}"
            continue
        except httpx.TimeoutException as exc:
            # Un timeout de lectura no es incompatibilidad de ruta: repetirlo
            # con la ruta alterna solo duplicaría la espera (F12).
            raise STTError(f"whisper-server no respondió a tiempo ({url}): {exc}") from exc
        except httpx.HTTPError as exc:
            raise STTError(f"whisper-server: {exc}") from exc
        if response.status_code == 404:
            _forget_endpoint(cfg.whisper_url, url)
            last_error = f"{url}: 404"
            continue
        if response.status_code >= 400:
            raise STTError(
                f"whisper-server respondió {response.status_code}: {response.text[:200]}"
            )
        _remember_endpoint(cfg.whisper_url, url)
        return _text(cfg, response)

    raise STTError(
        "No puedo conectar con whisper-server. Arranca scripts\\start-whisper.ps1. " + last_error
    )


# Frases que Whisper inventa con silencio o ruido (vienen de subtítulos de
# vídeos). Nadie se las dice a un agente de código: se descartan siempre.
_ALWAYS_HALLUCINATED = frozenset(
    {
        "gracias por ver el video",
        "gracias por ver",
        "gracias por vernos",
        "suscribete",
        "suscribete al canal",
        "no olvides suscribirte",
        "hasta la proxima",
    }
)
# Cortesías sueltas: pueden ser reales («gracias»), así que solo se descartan
# cuando el audio era casi silencio o ruido (`weak_audio`).
_HALLUCINATED_IF_WEAK = frozenset(
    {"gracias", "muchas gracias", "gracias a todos", "hasta luego", "chao"}
)


def _norm(text: str) -> str:
    raw = unicodedata.normalize("NFKD", text.lower())
    raw = "".join(c for c in raw if not unicodedata.combining(c))
    return " ".join(re.sub(r"[^a-z0-9 ]+", " ", raw).split())


def is_hallucination(text: str, *, weak_audio: bool = False) -> bool:
    """True si ``text`` es una frase típicamente inventada por Whisper."""
    key = _norm(text)
    if not key:
        return False
    if key in _ALWAYS_HALLUCINATED or "amara org" in key:
        return True
    return weak_audio and key in _HALLUCINATED_IF_WEAK


__all__ = [
    "STTError",
    "available_whisper_models",
    "close_client",
    "is_hallucination",
    "transcribe",
    "whisper_home",
    "whisper_model_path",
    "whisper_models_dir",
]

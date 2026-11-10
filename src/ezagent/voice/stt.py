"""STT de `ezagent-voice`: whisper.cpp por HTTP."""

from __future__ import annotations

import os
import re
import unicodedata
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit

import httpx

from .config import VoiceConfig


class STTError(RuntimeError):
    pass


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


def transcribe(cfg: VoiceConfig, wav_bytes: bytes) -> str:
    endpoints = [cfg.whisper_url]
    fallback = _derive_fallback(cfg.whisper_url)
    if fallback:
        endpoints.append(fallback)

    form = {
        "model": cfg.stt_model or "ggml-large-v3-turbo-q5_0",
        "language": "es",
        "response_format": "json",
        "temperature": "0.00",
    }
    if cfg.stt_prompt:
        form["prompt"] = cfg.stt_prompt

    last_error = ""
    for url in endpoints:
        try:
            response = httpx.post(
                url,
                data=form,
                files={"file": ("utterance.wav", wav_bytes, "audio/wav")},
                timeout=cfg.stt_timeout_secs,
            )
        except httpx.HTTPError as exc:
            last_error = f"{url}: {exc}"
            continue
        if response.status_code == 404:
            last_error = f"{url}: 404"
            continue
        if response.status_code >= 400:
            raise STTError(
                f"whisper-server respondió {response.status_code}: {response.text[:200]}"
            )
        try:
            text = response.json().get("text", "")
        except ValueError:
            text = response.text
        return str(text).strip()

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
    "is_hallucination",
    "transcribe",
    "whisper_home",
    "whisper_model_path",
    "whisper_models_dir",
]

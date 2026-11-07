"""Gestión del modelo Kokoro: por defecto usa la variante int8 (más rápida en CPU)."""

from __future__ import annotations

import os
import urllib.request
from pathlib import Path

from loguru import logger

# Ruta heredada de una versión con Pipecat. EzAgent NO usa Pipecat: solo
# reutiliza esta caché para no volver a descargar los modelos Kokoro.
CACHE_DIR = Path(os.path.expanduser("~/.cache/pipecat/kokoro-onnx"))
VOICES_URL = "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin"
MODEL_URLS = {
    "int8": (
        "kokoro-v1.0.int8.onnx",
        "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.int8.onnx",
    ),
    "fp32": (
        "kokoro-v1.0.onnx",
        "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx",
    ),
}


def _download(url: str, dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    logger.info(f"Descargando {url} -> {dest}")
    tmp = dest.with_suffix(dest.suffix + ".part")
    urllib.request.urlretrieve(url, tmp)
    tmp.replace(dest)


def _ensure(path: Path, url: str) -> Path:
    if not path.exists():
        _download(url, path)
    return path


def resolve_kokoro_files(
    variant: str = "int8",
    model_path: str | None = None,
    voices_path: str | None = None,
) -> tuple[str | None, str | None]:
    """Devuelve (modelo, voces) para Kokoro."""
    voices: Path | None = Path(voices_path) if voices_path else None
    if voices is None:
        voices = _ensure(CACHE_DIR / "voices-v1.0.bin", VOICES_URL)

    if model_path:
        model: Path | None = Path(model_path)
    elif variant in MODEL_URLS:
        filename, url = MODEL_URLS[variant]
        model = _ensure(CACHE_DIR / filename, url)
    else:
        logger.warning(
            f"KOKORO_MODEL_VARIANT={variant!r} no reconocido; usa KOKORO_MODEL_PATH"
        )
        model = None

    return (str(model) if model else None, str(voices) if voices else None)

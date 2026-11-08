"""Gestión del modelo Kokoro: por defecto usa la variante int8 (más rápida en CPU).

Los archivos viven en ``%LOCALAPPDATA%\\EzAgent\\models\\kokoro``. Si existen en
la caché heredada de una versión anterior (``~/.cache/pipecat/kokoro-onnx``) se
copian una sola vez, tras comprobar su hash; si no, los descarga el gestor.
"""

from __future__ import annotations

import os
from pathlib import Path

from loguru import logger

from ezagent.core.downloads import (
    Asset,
    DownloadError,
    adopt,
    asset_path,
    download,
    is_installed,
    load_manifest,
)

# Caché heredada de una versión con Pipecat. EzAgent NO usa Pipecat: solo se
# lee, nunca se escribe, para no obligar a descargar de nuevo los modelos.
LEGACY_DIR = Path(os.path.expanduser("~/.cache/pipecat/kokoro-onnx"))
VOICES_ASSET = "kokoro-voices"
MODEL_ASSETS = {"int8": "kokoro-int8", "fp32": "kokoro-fp32"}


def _ensure(asset: Asset) -> Path:
    if not is_installed(asset):
        legacy = LEGACY_DIR / Path(asset.dest).name
        if adopt(asset, legacy):
            logger.info(f"{asset.id}: copiado desde la caché anterior")
        else:
            download(asset)
    return asset_path(asset)


def resolve_kokoro_files(
    variant: str = "int8",
    model_path: str | None = None,
    voices_path: str | None = None,
) -> tuple[str | None, str | None]:
    """Devuelve (modelo, voces) para Kokoro."""
    manifest = load_manifest()
    voices: Path | None = Path(voices_path) if voices_path else None
    if voices is None:
        voices = _ensure(manifest[VOICES_ASSET])

    if model_path:
        model: Path | None = Path(model_path)
    elif variant in MODEL_ASSETS:
        model = _ensure(manifest[MODEL_ASSETS[variant]])
    else:
        logger.warning(
            f"KOKORO_MODEL_VARIANT={variant!r} no reconocido; usa KOKORO_MODEL_PATH"
        )
        model = None

    return (str(model) if model else None, str(voices) if voices else None)


__all__ = ["DownloadError", "resolve_kokoro_files"]

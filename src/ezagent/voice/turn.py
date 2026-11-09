"""Detección semántica del fin de turno con Smart Turn v3 (Pipecat, BSD-2).

El VAD por energía solo sabe si hay silencio; no sabe si la frase terminó o es
una pausa para pensar. Smart Turn mira los últimos 8 s de audio y estima la
probabilidad de que el turno haya acabado. Con eso se puede cerrar el turno a
los ~0,3 s de silencio cuando la frase está completa y seguir esperando el
silencio largo de siempre cuando no lo está.

Las características son las de ``WhisperFeatureExtractor(chunk_length=8)``
reescritas con numpy, para no depender de ``transformers``.
"""

from __future__ import annotations

import threading
from concurrent.futures import Future, ThreadPoolExecutor
from pathlib import Path

import numpy as np
from loguru import logger

SAMPLE_RATE = 16000
WINDOW_SECS = 8
N_SAMPLES = SAMPLE_RATE * WINDOW_SECS
N_FFT = 400
HOP = 160
N_MELS = 80
ASSET_ID = "smart-turn-v3.2-cpu"


# --------------------------------------------------------- características
def _hz_to_mel(freq: np.ndarray) -> np.ndarray:
    """Escala mel de Slaney (la que usa Whisper)."""
    freq = np.asarray(freq, dtype=np.float64)
    mels = 3.0 * freq / 200.0
    logstep = 27.0 / np.log(6.4)
    high = freq >= 1000.0
    mels[high] = 15.0 + np.log(freq[high] / 1000.0) * logstep
    return mels


def _mel_to_hz(mels: np.ndarray) -> np.ndarray:
    mels = np.asarray(mels, dtype=np.float64)
    freq = 200.0 * mels / 3.0
    logstep = np.log(6.4) / 27.0
    high = mels >= 15.0
    freq[high] = 1000.0 * np.exp(logstep * (mels[high] - 15.0))
    return freq


def _mel_filters() -> np.ndarray:
    """Banco de filtros triangulares (201 x 80) con normalización de Slaney."""
    fft_freqs = np.linspace(0, SAMPLE_RATE // 2, 1 + N_FFT // 2)
    edges = _hz_to_mel(np.array([0.0, SAMPLE_RATE / 2]))
    filter_freqs = _mel_to_hz(np.linspace(edges[0], edges[1], N_MELS + 2))
    diff = np.diff(filter_freqs)
    slopes = filter_freqs[None, :] - fft_freqs[:, None]
    down = -slopes[:, :-2] / diff[:-1]
    up = slopes[:, 2:] / diff[1:]
    filters = np.maximum(0.0, np.minimum(down, up))
    enorm = 2.0 / (filter_freqs[2 : N_MELS + 2] - filter_freqs[:N_MELS])
    return filters * enorm[None, :]


_FILTERS = _mel_filters()
_WINDOW = np.hanning(N_FFT + 1)[:-1]  # Hann periódica


def log_mel(audio: np.ndarray) -> np.ndarray:
    """Log-mel de Whisper (80 x 800) de 8 s de audio a 16 kHz, ya normalizado."""
    padded = np.pad(audio.astype(np.float64), N_FFT // 2, mode="reflect")
    frames = 1 + (len(padded) - N_FFT) // HOP
    index = np.arange(N_FFT)[None, :] + HOP * np.arange(frames)[:, None]
    spectrum = np.abs(np.fft.rfft(padded[index] * _WINDOW, n=N_FFT)) ** 2
    mel = np.maximum(1e-10, _FILTERS.T @ spectrum.T)
    spec = np.log10(mel)[:, :-1]
    spec = np.maximum(spec, spec.max() - 8.0)
    return ((spec + 4.0) / 4.0).astype(np.float32)


def prepare(pcm: bytes, rate: int) -> np.ndarray:
    """PCM int16 mono -> entrada del modelo (1 x 80 x 800)."""
    audio = np.frombuffer(pcm, dtype=np.int16).astype(np.float32) / 32768.0
    if rate != SAMPLE_RATE and len(audio):
        # DECISION: interpolación lineal; para clasificar el fin de turno no
        # hace falta un remuestreo de calidad de estudio.
        length = int(round(len(audio) * SAMPLE_RATE / rate))
        positions = np.linspace(0, len(audio) - 1, max(1, length))
        audio = np.interp(positions, np.arange(len(audio)), audio)
    # Como el inference.py de referencia: los últimos 8 s, con ceros al
    # PRINCIPIO para que el final de la voz quede al final de la ventana, y
    # normalización sobre la ventana entera (relleno incluido).
    window = np.zeros(N_SAMPLES, dtype=np.float64)
    tail = np.asarray(audio[-N_SAMPLES:], dtype=np.float64)
    if len(tail):
        window[-len(tail) :] = tail
    window = (window - window.mean()) / np.sqrt(window.var() + 1e-7)
    return log_mel(window)[None, :, :]


# ------------------------------------------------------------------ modelo
def resolve_model_path() -> Path:
    """Ruta del modelo; lo descarga (con hash verificado) si falta."""
    from ezagent.core.downloads import (
        asset_path,
        download,
        is_installed,
        load_manifest,
    )

    asset = load_manifest()[ASSET_ID]
    if not is_installed(asset):
        download(asset)
    return asset_path(asset)


class SmartTurn:
    """Evalúa el fin de turno en un hilo propio, sin bloquear el de audio."""

    def __init__(self, threshold: float = 0.5, model_path: str = "") -> None:
        self.threshold = threshold
        self._model_path = model_path
        self._session = None
        self._ready = threading.Event()
        self._failed = False
        self._pool = ThreadPoolExecutor(max_workers=1, thread_name_prefix="ezagent-turn")
        self._pool.submit(self._load)

    @property
    def ready(self) -> bool:
        return self._ready.is_set()

    def _load(self) -> None:
        try:
            import onnxruntime as ort

            path = self._model_path or str(resolve_model_path())
            options = ort.SessionOptions()
            options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
            options.inter_op_num_threads = 1
            options.intra_op_num_threads = 2
            self._session = ort.InferenceSession(
                path, sess_options=options, providers=["CPUExecutionProvider"]
            )
            self.probability(b"\0\0" * SAMPLE_RATE, SAMPLE_RATE)  # precalienta
            self._ready.set()
        except Exception as exc:  # noqa: BLE001
            self._failed = True
            logger.warning("Smart Turn no disponible; uso solo el silencio: {}", exc)

    def probability(self, pcm: bytes, rate: int) -> float:
        """Probabilidad de que el turno haya terminado (0 a 1)."""
        if self._session is None:
            raise RuntimeError("Smart Turn no está cargado")
        outputs = self._session.run(None, {"input_features": prepare(pcm, rate)})
        return float(np.asarray(outputs[0]).reshape(-1)[0])

    def submit(self, pcm: bytes, rate: int) -> Future[bool] | None:
        """Encola una evaluación; None si el modelo aún no está listo."""
        if not self._ready.is_set():
            return None
        return self._pool.submit(lambda: self.probability(pcm, rate) >= self.threshold)

    def close(self) -> None:
        self._pool.shutdown(wait=False, cancel_futures=True)

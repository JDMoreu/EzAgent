"""Fixtures de Smart Turn (dominio `turn`, F1-08).

Cubre `hz_to_mel`, `mel_to_hz`, `log_mel` y `prepare` de
`ezagent.voice.turn` sobre los WAV de `parity/golden/audio/`. Si el modelo
`smart-turn-v3.2-cpu.onnx` está instalado, añade la probabilidad real de
`SmartTurn.probability` (la misma sesión ONNX y las mismas opciones).

Una matriz log-mel completa son 64 000 flotantes por caso; el fixture guarda un
resumen exacto y comparable: forma, mínimo/máximo/media/suma y, por cada banda
mel, mínimo/máximo/media/argmax, más las columnas de cinco fotogramas. Los
tests de Rust comparan con la tolerancia de `parity/thresholds.toml`.
"""

from __future__ import annotations

import hashlib
import pathlib
import wave
from typing import Any

import numpy as np
from _common import ROOT, FixtureFile, main_guard, sha256

from ezagent.voice.turn import (
    N_FFT,
    SAMPLE_RATE,
    log_mel,
    prepare,
)
from ezagent.voice.turn import (
    _hz_to_mel as hz_to_mel,
)
from ezagent.voice.turn import (
    _mel_to_hz as mel_to_hz,
)

DOMAIN = "turn"
WAV_DIR = ROOT / "parity" / "golden" / "audio"
MODEL_NAME = "smart-turn-v3.2-cpu.onnx"


def _model_path() -> pathlib.Path | None:
    """Ruta real del modelo, resuelta antes de congelar el entorno.

    Se busca en disco en vez de llamar a `resolve_model_path`: si el modelo no
    está instalado, esa función intentaría descargarlo y el oráculo no hace red.
    """
    import os

    base = os.environ.get("EZAGENT_HOME")
    if base:
        root = pathlib.Path(base)
    else:
        local = os.environ.get("LOCALAPPDATA") or str(pathlib.Path.home() / "AppData" / "Local")
        root = pathlib.Path(local) / "EzAgent"
    path = root / "models" / MODEL_NAME
    return path if path.is_file() else None


_MODEL_PATH = _model_path()
_SESSION: Any = None


def _session() -> Any:
    global _SESSION
    if _SESSION is None:
        import onnxruntime as ort

        assert _MODEL_PATH is not None
        options = ort.SessionOptions()
        options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
        options.inter_op_num_threads = 1
        options.intra_op_num_threads = 2
        _SESSION = ort.InferenceSession(
            str(_MODEL_PATH), sess_options=options, providers=["CPUExecutionProvider"]
        )
    return _SESSION


def _probability(pcm: bytes, rate: int) -> float:
    outputs = _session().run(None, {"input_features": prepare(pcm, rate)})
    return float(np.asarray(outputs[0]).reshape(-1)[0])


def _wav_samples(name: str) -> tuple[bytes, np.ndarray, pathlib.Path]:
    path = WAV_DIR / f"{name}.wav"
    with wave.open(str(path), "rb") as handle:
        pcm = handle.readframes(handle.getnframes())
    audio = np.frombuffer(pcm, dtype="<i2").astype(np.float32) / 32768.0
    return pcm, audio, path


def _error_of(fn: Any, *args: object) -> str:
    try:
        fn(*args)
    except Exception as error:  # el oráculo es el mensaje real de Python
        return str(error)
    raise AssertionError(f"{fn.__name__} no falló con {args!r}")


# DECISION: una matriz log-mel son 64 000 flotantes; el caso guarda un resumen
# exacto (forma, extremos, media, suma, columnas de 5 fotogramas y estadísticas
# por banda) para que el JSONL sea legible y el test compare con tolerancia f32.
def _summary(features: np.ndarray) -> dict[str, Any]:
    data = np.asarray(features, dtype=np.float32)
    if data.ndim == 3:
        data = data[0]
    rows, frames = data.shape
    frame_indices = sorted({0, frames // 4, frames // 2, (3 * frames) // 4, frames - 1})
    return {
        "shape": [rows, frames],
        "min": float(np.min(data)),
        "max": float(np.max(data)),
        "mean": float(np.mean(data, dtype=np.float64)),
        "sum": float(np.sum(data, dtype=np.float64)),
        "rows": [
            {
                "min": float(row.min()),
                "max": float(row.max()),
                "mean": float(np.mean(row, dtype=np.float64)),
                "argmax": int(np.argmax(row)),
            }
            for row in data
        ],
        "frames": {
            str(index): [float(value) for value in data[:, index]] for index in frame_indices
        },
        "frame_indices": frame_indices,
    }


def _pcm_info(name: str) -> dict[str, Any]:
    path = WAV_DIR / f"{name}.wav"
    with wave.open(str(path), "rb") as handle:
        samples = handle.getnframes()
    return {
        "wav": f"parity/golden/audio/{name}.wav",
        "wav_sha256": sha256(path),
        "samples": samples,
        "rate": SAMPLE_RATE,
    }


HZ_CASES: list[list[float]] = [
    [0.0],
    [500.0],
    [999.999],
    [1000.0],
    [1000.001],
    [4000.0],
    [8000.0],
    [0.0, 500.0, 1000.0, 4000.0, 8000.0],
    [-1000.0, -1.0, 0.0, 1.0],
    [100.0, 200.0, 300.0, 400.0, 500.0],
    [1000000.0],
    [123.456, 789.012, 3456.789],
    [],
]

MEL_CASES: list[list[float]] = [
    [0.0],
    [7.5],
    [14.999],
    [15.0],
    [15.001],
    [20.0],
    [50.0],
    [0.0, 2.0, 7.5, 14.0, 15.0, 20.0, 50.0],
    [-10.0, -1.0, 0.0, 1.0],
    [3.0, 6.0, 9.0, 12.0, 15.0],
    [1000.0],
    [1.5, 16.5, 42.25],
    [],
]

LOG_MEL_WAVS = [
    "silence_8s",
    "tone_sweep_8s",
    "voice_dora_8s",
    "voice_alex_8s",
    "synthetic_voiced_8s",
    "mixed_all_8s",
    "silence_0_5s",
    "silence_1s",
    "voice_dora_corto",
    "voice_dora_9s",
]

PREPARE_CASES: list[tuple[str, int]] = [
    ("voice_dora_8s", 16000),
    ("voice_dora_9s", 16000),
    ("voice_dora_4s", 16000),
    ("voice_dora_corto", 16000),
    ("voice_dora_short_burst", 16000),
    ("silence_1s", 16000),
    ("tone_sweep_2s", 16000),
    ("voice_dora_corto", 8000),
    ("voice_dora_corto", 22050),
    ("voice_dora_corto", 24000),
    ("voice_dora_corto", 44100),
    ("voice_dora_corto", 48000),
    ("voice_dora_largo", 8000),
    ("voice_alex_largo", 44100),
    ("voice_dora_8s", 48000),
]

PROBABILITY_WAVS = [
    "silence_8s",
    "tone_sweep_8s",
    "voice_dora_8s",
    "voice_dora_9s",
    "voice_alex_8s",
    "voice_dora_short_burst",
    "noise_white_3s",
    "mixed_all_8s",
    "synthetic_voiced_8s",
]


def _add_hz(out: FixtureFile) -> None:
    for index, values in enumerate(HZ_CASES, start=1):
        result = hz_to_mel(np.array(values, dtype=np.float64))
        out.add(
            f"turn/hz_to_mel/{index:04d}",
            "hz_to_mel",
            {"freqs": values},
            {"mels": [float(value) for value in result]},
        )
    for index, values in enumerate(MEL_CASES, start=1):
        result = mel_to_hz(np.array(values, dtype=np.float64))
        out.add(
            f"turn/mel_to_hz/{index:04d}",
            "mel_to_hz",
            {"mels": values},
            {"freqs": [float(value) for value in result]},
        )


def _add_log_mel(out: FixtureFile) -> None:
    for index, name in enumerate(LOG_MEL_WAVS, start=1):
        pcm, audio, path = _wav_samples(name)
        result = log_mel(audio)
        out.add(
            f"turn/log_mel/{index:04d}_{name}",
            "log_mel",
            {
                "wav": f"parity/golden/audio/{name}.wav",
                "wav_sha256": sha256(path),
                "samples": len(audio),
                "rate": SAMPLE_RATE,
                "pcm_sha256": hashlib.sha256(pcm).hexdigest(),
            },
            _summary(result),
        )
    tiny = np.array([0.0, 0.1, -0.1], dtype=np.float64)
    out.add_error(
        "turn/log_mel/err01_tiny",
        "log_mel",
        {"samples_source": "microphone", "count": len(tiny)},
        _error_of(log_mel, tiny),
    )
    out.add_error(
        "turn/log_mel/err02_empty",
        "log_mel",
        {"samples_source": "microphone", "count": 0},
        _error_of(log_mel, np.zeros(0, dtype=np.float64)),
    )
    exactly = np.zeros(N_FFT // 2 + 1, dtype=np.float64)
    exactly[N_FFT // 4] = 0.5
    out.add(
        "turn/log_mel/edge_pad_limit",
        "log_mel",
        {"samples_source": "microphone", "count": len(exactly)},
        _summary(log_mel(exactly)),
    )
    just_under = np.zeros(N_FFT // 2, dtype=np.float64)
    out.add(
        "turn/log_mel/edge_pad_one_less",
        "log_mel",
        {"samples_source": "microphone", "count": len(just_under)},
        _summary(log_mel(just_under)),
    )


def _prepare_payload(name: str, rate: int) -> dict[str, Any]:
    payload = _pcm_info(name)
    payload["rate"] = rate
    return payload


def _add_prepare(out: FixtureFile) -> None:
    for index, (name, rate) in enumerate(PREPARE_CASES, start=1):
        pcm, _, _ = _wav_samples(name)
        features = prepare(pcm, rate)
        length = (
            len(pcm) // 2 if rate == SAMPLE_RATE else int(round(len(pcm) / 2 * SAMPLE_RATE / rate))
        )
        summary = _summary(features)
        summary["resampled_samples"] = length if pcm else 0
        out.add(
            f"turn/prepare/{index:04d}_{name}_r{rate}",
            "prepare",
            _prepare_payload(name, rate),
            summary,
        )

    empty = {"kind": "hex", "hex": ""}
    out.add(
        "turn/prepare/0100_empty",
        "prepare",
        {"pcm": empty, "rate": SAMPLE_RATE},
        {**_summary(prepare(b"", SAMPLE_RATE)), "resampled_samples": 0},
    )
    odd = {"kind": "hex", "hex": "00ff01"}
    out.add_error(
        "turn/prepare/err01_odd_byte",
        "prepare",
        {"pcm": odd, "rate": SAMPLE_RATE},
        _error_of(prepare, b"\x00\xff\x01", SAMPLE_RATE),
    )
    out.add_error(
        "turn/prepare/err02_rate_zero",
        "prepare",
        {"pcm": {"kind": "hex", "hex": "0000"}, "rate": 0},
        _error_of(prepare, b"\x00\x00", 0),
    )
    negative = prepare(b"\x00\x40\x00\xc0", -16000)
    out.add(
        "turn/prepare/0102_negative_rate",
        "prepare",
        {"pcm": {"kind": "hex", "hex": "004000c0"}, "rate": -16000},
        {**_summary(negative), "resampled_samples": 1},
    )
    single = prepare(b"\x00\x40", SAMPLE_RATE)
    out.add(
        "turn/prepare/0101_single_sample",
        "prepare",
        {"pcm": {"kind": "hex", "hex": "0040"}, "rate": SAMPLE_RATE},
        {**_summary(single), "resampled_samples": 1},
    )


def _add_probability(out: FixtureFile) -> None:
    if _MODEL_PATH is None:
        return
    model_hash = hashlib.sha256(_MODEL_PATH.read_bytes()).hexdigest()
    for index, name in enumerate(PROBABILITY_WAVS, start=1):
        pcm, _, path = _wav_samples(name)
        probability = _probability(pcm, SAMPLE_RATE)
        for threshold in (0.5, 0.2, 0.8):
            out.add(
                f"turn/probability/{index:04d}_{name}_t{str(threshold).replace('.', '_')}",
                "probability",
                {
                    "wav": f"parity/golden/audio/{name}.wav",
                    "wav_sha256": sha256(path),
                    "samples": len(pcm) // 2,
                    "rate": SAMPLE_RATE,
                    "threshold": threshold,
                    "model_sha256": model_hash,
                },
                {"probability": probability, "ended": probability >= threshold},
            )


def generate(out: FixtureFile) -> None:
    _add_hz(out)
    _add_log_mel(out)
    _add_prepare(out)
    _add_probability(out)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)

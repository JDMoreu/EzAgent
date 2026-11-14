"""Fixtures de audio (dominio `audio`, F1-08).

Cubre el comportamiento real de `ezagent.voice.audio` (`pcm_rms`,
`pcm_is_weak`, `pcm_to_wav`, `Segmenter` y `BargeInDetector`) y el remuestreo
lineal de `ezagent.voice.turn.prepare`.

Los WAV de `parity/golden/audio/` son deterministas: voz sintética de Kokoro
(resampleada de 24 kHz a 16 kHz con la misma interpolación lineal de
`turn.prepare`) o tonos/ruido con semilla fija. Cada WAV lleva sus límites de
voz en la salida (`regions`, en muestras) y su sha256.

Los PCM se describen con un objeto (`input.pcm`) para no repetir bytes:

* ``{"kind": "hex", "hex": "0100..."}`` — bytes exactos.
* ``{"kind": "pattern", "bytes": n, "pattern": "ramp", "seed": s}`` — el byte
  ``j`` vale ``(seed + j) % 256``.
* ``{"kind": "wav", "wav": "parity/golden/audio/x.wav", "start": s, "length": n}``
  — un trozo del WAV indicado (el archivo lo produce este mismo generador).

Los chunks sintéticos del segmentador son ``input.chunk_pattern`` bytes, donde
el chunk ``i`` repite ``(seed + i * bytes + j) % 256``. El reloj monótono se
congela a pasos de `CHUNK_SECS` para que `last_speech_end` sea reproducible.
"""

from __future__ import annotations

import hashlib
import io
import pathlib
import time
import wave
from concurrent.futures import Future
from typing import Any, Callable

import numpy as np
from _common import GOLDEN, ROOT, FixtureFile, main_guard, sha256

from ezagent.voice.audio import (
    BARGE_IN_SECS,
    CHUNK_SECS,
    BargeInDetector,
    Segmenter,
    pcm_is_weak,
    pcm_rms,
    pcm_to_wav,
)

DOMAIN = "audio"
RATE = 16000
WAV_DIR = GOLDEN / "audio"


# ------------------------------------------------------------------ utilidades
def _pcm16(samples: np.ndarray) -> bytes:
    clipped = np.clip(np.asarray(samples, dtype=np.float64), -1.0, 1.0)
    return (clipped * 32767.0).astype("<i2").tobytes()


def _samples_of(pcm: bytes) -> np.ndarray:
    pcm = pcm[: len(pcm) - len(pcm) % 2]
    return np.frombuffer(pcm, dtype="<i2").astype(np.float64) / 32768.0


def _wav_bytes(samples: np.ndarray) -> bytes:
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(RATE)
        handle.writeframes(_pcm16(samples))
    return buffer.getvalue()


def _write_wav(name: str, samples: np.ndarray) -> pathlib.Path:
    WAV_DIR.mkdir(parents=True, exist_ok=True)
    path = WAV_DIR / f"{name}.wav"
    path.write_bytes(_wav_bytes(samples))
    return path


def _read_wav_pcm(path: pathlib.Path) -> bytes:
    with wave.open(str(path), "rb") as handle:
        if handle.getnchannels() != 1 or handle.getsampwidth() != 2:
            raise ValueError(f"{path.name}: se esperaba mono PCM16")
        return handle.readframes(handle.getnframes())


def _pcm_bytes(spec: dict[str, Any]) -> bytes:
    kind = spec["kind"]
    if kind == "hex":
        return bytes.fromhex(spec["hex"])
    if kind == "pattern":
        size = int(spec["bytes"])
        seed = int(spec.get("seed", 0))
        return bytes((seed + index) % 256 for index in range(size))
    if kind == "wav":
        path = ROOT / spec["wav"]
        data = _read_wav_pcm(path)
        start = int(spec.get("start", 0))
        length = int(spec.get("length", len(data) - start))
        return data[start : start + length]
    raise ValueError(f"pcm desconocido: {kind}")


# DECISION: los PCM grandes se identifican por sha256 y extremos, no por hex
# completo; los tests reconstruyen los bytes con el patrón o el WAV indicado.
def _pcm_info(pcm: bytes | None) -> dict[str, Any] | None:
    if pcm is None:
        return None
    return {
        "bytes": len(pcm),
        "sha256": hashlib.sha256(pcm).hexdigest(),
        "head_hex": pcm[:16].hex(),
        "tail_hex": pcm[-16:].hex(),
    }


# DECISION: el remuestreo del dominio `audio` es el de `turn.prepare`; se copia
# aquí (np.interp) porque `audio.py` no tiene una función pública equivalente y
# el brief pide cubrirlo junto a pcm_rms y el segmentador.
def _resampled(samples: np.ndarray, from_rate: int, to_rate: int) -> np.ndarray:
    """`np.interp` de `turn.prepare` (mismo redondeo y mismas posiciones)."""
    if from_rate == to_rate or not len(samples):
        return np.asarray(samples, dtype=np.float64)
    length = int(round(len(samples) * to_rate / from_rate))
    positions = np.linspace(0, len(samples) - 1, max(1, length))
    return np.interp(positions, np.arange(len(samples)), samples)


# ------------------------------------------------------------------- voz Kokoro
def _kokoro_paths() -> tuple[pathlib.Path, pathlib.Path] | None:
    """Rutas reales del modelo, resueltas antes de congelar el entorno."""
    try:
        from ezagent.tts import resolve_kokoro_files

        model, voices = resolve_kokoro_files()
    except Exception:
        return None
    model_path = pathlib.Path(model)
    voices_path = pathlib.Path(voices)
    if model_path.is_file() and voices_path.is_file():
        return model_path, voices_path
    return None


_KOKORO_PATHS = _kokoro_paths()
_KOKORO: Any = None
_VOICE_CACHE: dict[tuple[str, str, float], np.ndarray] = {}


def _kokoro() -> Any:
    global _KOKORO
    if _KOKORO is None:
        from kokoro_onnx import Kokoro

        assert _KOKORO_PATHS is not None
        _KOKORO = Kokoro(str(_KOKORO_PATHS[0]), str(_KOKORO_PATHS[1]))
    return _KOKORO


def _fallback_voice(text: str, voice: str) -> np.ndarray:
    """Voz sintética determinista si Kokoro no está instalado."""
    seed = int.from_bytes(hashlib.sha256(f"{voice}:{text}".encode()).digest()[:4], "big")
    duration = max(0.4, 0.075 * len(text))
    count = int(RATE * duration)
    t = np.arange(count) / RATE
    f0 = 95.0 + float(seed % 50)
    signal = sum(np.sin(2.0 * np.pi * f0 * harmonic * t) / harmonic for harmonic in range(1, 6))
    syllable = 0.5 + 0.5 * np.sin(2.0 * np.pi * 4.2 * t + (seed % 17) / 17.0 * 2.0 * np.pi)
    envelope = np.clip(syllable, 0.0, 1.0) ** 1.5
    return 0.18 * signal * envelope


def _voice(text: str, voice: str, speed: float = 1.0) -> np.ndarray:
    key = (text, voice, speed)
    if key in _VOICE_CACHE:
        return _VOICE_CACHE[key].copy()
    if _KOKORO_PATHS is None:
        samples = _fallback_voice(text, voice)
    else:
        engine = _kokoro()
        raw, rate = engine.create(
            text,
            voice=engine.get_voice_style(voice),
            lang="es",
            speed=speed,
            sentence_pause=0.0,
            clause_pause=0.0,
        )
        samples = _resampled(np.asarray(raw, dtype=np.float32), int(rate), RATE)
    samples = np.asarray(samples, dtype=np.float64)
    _VOICE_CACHE[key] = samples
    return samples.copy()


# -------------------------------------------------------------- corpus de WAV
def _silence(secs: float) -> np.ndarray:
    return np.zeros(int(RATE * secs), dtype=np.float64)


def _tone(freq: float, secs: float, amplitude: float = 0.3) -> np.ndarray:
    t = np.arange(int(RATE * secs)) / RATE
    return amplitude * np.sin(2.0 * np.pi * freq * t)


def _sweep(start: float, end: float, secs: float, amplitude: float = 0.25) -> np.ndarray:
    count = int(RATE * secs)
    t = np.arange(count) / RATE
    frequency = start + (end - start) * t / max(secs, 1e-9)
    phase = 2.0 * np.pi * np.cumsum(frequency) / RATE
    return amplitude * np.sin(phase)


def _noise(secs: float, amplitude: float, seed: int) -> np.ndarray:
    generator = np.random.default_rng(seed)
    return amplitude * generator.standard_normal(int(RATE * secs))


def _voiced_harmonics(secs: float, amplitude: float, seed: int) -> np.ndarray:
    count = int(RATE * secs)
    t = np.arange(count) / RATE
    signal = sum(np.sin(2.0 * np.pi * 140.0 * harmonic * t) / harmonic for harmonic in range(1, 7))
    vibrato = 1.0 + 0.02 * np.sin(2.0 * np.pi * 5.0 * t + float(seed % 7))
    return amplitude * signal / 2.2 * vibrato


def _syllables(secs: float, amplitude: float, seed: int) -> np.ndarray:
    count = int(RATE * secs)
    t = np.arange(count) / RATE
    gate = (np.sin(2.0 * np.pi * 3.2 * t) > -0.1).astype(np.float64)
    f0 = 120.0 + float(seed % 30)
    signal = sum(np.sin(2.0 * np.pi * f0 * harmonic * t) / harmonic for harmonic in range(1, 5))
    return amplitude * signal / 2.0 * gate


def _join(parts: list[tuple[np.ndarray, bool]]) -> tuple[np.ndarray, list[list[int]]]:
    arrays: list[np.ndarray] = []
    regions: list[list[int]] = []
    position = 0
    for array, voiced in parts:
        arrays.append(array)
        if voiced and len(array):
            regions.append([position, position + len(array)])
        position += len(array)
    if not arrays:
        return np.zeros(0), []
    return np.concatenate(arrays), regions


def _fixed_length(samples: np.ndarray, secs: float) -> np.ndarray:
    count = int(RATE * secs)
    if len(samples) >= count:
        return samples[:count].copy()
    padded = np.zeros(count, dtype=np.float64)
    padded[: len(samples)] = samples
    return padded


def _clip_regions(regions: list[list[int]], count: int) -> list[list[int]]:
    clipped: list[list[int]] = []
    for start, end in regions:
        if start >= count:
            continue
        clipped.append([start, min(end, count)])
    return clipped


def _corpus() -> list[dict[str, Any]]:
    dora = "ef_dora"
    alex = "em_alex"
    santa = "em_santa"
    escuchame = _voice("EzAgent, escúchame.", dora)
    hola = _voice("Hola, ¿cómo estás?", dora)
    revisar = _voice("Necesito que revises el informe de la mañana antes de la reunión.", dora)
    reunion = _voice("La reunión empieza a las tres de la tarde.", santa)
    registro = _voice("Voy a revisar el registro del sistema y te aviso cuando termine.", alex)
    numeros = _voice("Uno, dos, tres, cuatro, cinco.", dora)
    si = _voice("Sí.", dora)
    no = _voice("No, todavía no.", alex)

    items: list[dict[str, Any]] = []

    def add(
        name: str,
        kind: str,
        samples: np.ndarray,
        regions: list[list[int]],
        note: str,
    ) -> None:
        items.append(
            {
                "name": name,
                "kind": kind,
                "samples": np.asarray(samples, dtype=np.float64),
                "regions": regions,
                "note": note,
            }
        )

    add("silence_0_5s", "silencio", _silence(0.5), [], "medio segundo de ceros")
    add("silence_1s", "silencio", _silence(1.0), [], "un segundo de ceros")
    add("silence_3s", "silencio", _silence(3.0), [], "tres segundos de ceros")
    add("silence_8s", "silencio", _silence(8.0), [], "ventana completa de Smart Turn")
    add("silence_10s", "silencio", _silence(10.0), [], "más de la ventana de Smart Turn")

    add("tone_440_1s", "tono", _tone(440.0, 1.0), [[0, RATE]], "tono continuo de 440 Hz")
    add("tone_220_2s", "tono", _tone(220.0, 2.0, 0.2), [[0, 2 * RATE]], "tono grave continuo")
    add("tone_1k_0_5s", "tono", _tone(1000.0, 0.5, 0.4), [[0, RATE // 2]], "tono agudo corto")
    add("tone_sweep_2s", "tono", _sweep(100.0, 3000.0, 2.0), [[0, 2 * RATE]], "barrido 100→3000 Hz")
    add("tone_sweep_8s", "tono", _sweep(50.0, 4000.0, 8.0), [[0, 8 * RATE]], "barrido de 8 s")
    beeps = np.concatenate(
        [
            _silence(0.3),
            _tone(880.0, 0.15, 0.35),
            _silence(0.4),
            _tone(880.0, 0.15, 0.35),
            _silence(0.4),
            _tone(880.0, 0.15, 0.35),
            _silence(0.45),
        ]
    )
    add(
        "tone_beeps_2s",
        "tono",
        beeps,
        [
            [int(0.3 * RATE), int(0.45 * RATE)],
            [int(0.85 * RATE), int(1.0 * RATE)],
            [int(1.4 * RATE), int(1.55 * RATE)],
        ],
        "tres pitidos con pausas",
    )

    add("noise_white_1s", "ruido", _noise(1.0, 0.01, 11), [], "ruido blanco suave")
    add("noise_white_3s", "ruido", _noise(3.0, 0.08, 12), [], "ruido blanco fuerte")
    add("noise_low_2s", "ruido", _noise(2.0, 0.0004, 13), [], "ruido casi inaudible")
    bursts = np.zeros(2 * RATE)
    for start in range(0, 2 * RATE, int(0.4 * RATE)):
        size = int(0.2 * RATE)
        bursts[start : start + size] = _noise(0.2, 0.05, 14 + start)[
            : len(bursts[start : start + size])
        ]
    add("noise_bursts_2s", "ruido", bursts, [], "ráfagas de ruido con silencios")
    clicks = _noise(1.0, 0.003, 15)
    for position in (0.1, 0.35, 0.6, 0.85):
        index = int(position * RATE)
        clicks[index : index + 20] += 0.8 * np.hanning(20)
    add("noise_clicks_1s", "ruido", clicks, [], "ruido de sala con cuatro clics")

    add("voice_dora_escuchame", "voz", escuchame, [[0, len(escuchame)]], "frase de reactivación")
    add("voice_dora_corto", "voz", hola, [[0, len(hola)]], "saludo corto")
    add("voice_dora_largo", "voz", revisar, [[0, len(revisar)]], "frase larga")
    add("voice_alex_corto", "voz", si, [[0, len(si)]], "una sola palabra")
    add("voice_alex_largo", "voz", registro, [[0, len(registro)]], "frase larga, voz masculina")
    add("voice_santa_corto", "voz", reunion, [[0, len(reunion)]], "otra voz, frase media")
    add("voice_santa_largo", "voz", numeros, [[0, len(numeros)]], "enumeración")
    add("voice_dora_no", "voz", no, [[0, len(no)]], "negación corta")

    two_phrases, two_regions = _join([(hola, True), (_silence(0.4), False), (no, True)])
    add("voice_dora_dos_frases", "voz", two_phrases, two_regions, "dos frases con pausa")

    conversation, conversation_regions = _join(
        [
            (hola, True),
            (_silence(0.8), False),
            (si, True),
            (_silence(0.8), False),
            (reunion, True),
        ]
    )
    add("voice_mixed_conversation", "voz", conversation, conversation_regions, "tres turnos de voz")

    add("voice_dora_quiet", "voz", 0.18 * hola, [[0, len(hola)]], "voz a bajo volumen")
    add("voice_dora_whisper", "voz", 0.06 * hola, [[0, len(hola)]], "casi un susurro")
    add("voice_dora_loud", "voz", 1.6 * hola, [[0, len(hola)]], "voz saturada")
    add(
        "voice_dora_clipped_start",
        "voz",
        hola[int(0.12 * RATE) :],
        [[0, len(hola) - int(0.12 * RATE)]],
        "empieza a mitad de palabra",
    )
    add(
        "voice_dora_clipped_end",
        "voz",
        hola[: int(0.8 * len(hola))],
        [[0, int(0.8 * len(hola))]],
        "corta la última sílaba",
    )
    add(
        "voice_dora_short_burst",
        "voz",
        hola[: int(0.18 * RATE)],
        [[0, int(0.18 * RATE)]],
        "ráfaga de 180 ms",
    )

    noisy, noisy_regions = _join([(hola, True)])
    noisy = noisy + _noise(len(noisy) / RATE, 0.03, 21)
    add("voice_dora_noise_10db", "voz+ruido", noisy, noisy_regions, "voz con ruido a 10 dB")
    noisy3, noisy3_regions = _join([(escuchame, True)])
    noisy3 = noisy3 + _noise(len(noisy3) / RATE, 0.08, 22)
    add("voice_dora_noise_3db", "voz+ruido", noisy3, noisy3_regions, "voz con ruido a 3 dB")
    with_tone, with_tone_regions = _join([(hola, True)])
    with_tone = with_tone + _tone(440.0, len(with_tone) / RATE, 0.08)
    add("voice_dora_tone_mix", "voz+tono", with_tone, with_tone_regions, "voz con un tono de fondo")

    pause_voice, pause_regions = _join([(escuchame, True), (_silence(1.5), False), (revisar, True)])
    add(
        "voice_dora_pause_voice",
        "voz",
        pause_voice,
        pause_regions,
        "dos frases con 1,5 s entre ellas",
    )
    leading, leading_regions = _join([(_silence(1.0), False), (hola, True)])
    add(
        "voice_dora_leading_silence",
        "voz",
        leading,
        leading_regions,
        "un segundo de silencio antes",
    )
    trailing, trailing_regions = _join([(hola, True), (_silence(2.0), False)])
    add(
        "voice_dora_trailing_silence",
        "voz",
        trailing,
        trailing_regions,
        "dos segundos de silencio después",
    )

    eight = _fixed_length(hola, 8.0)
    add(
        "voice_dora_8s",
        "voz",
        eight,
        [[0, min(len(hola), 8 * RATE)]],
        "8 s exactos con voz al principio",
    )
    nine, nine_regions = _join([(_silence(1.0), False), (revisar, True)])
    nine = _fixed_length(nine, 9.0)
    add(
        "voice_dora_9s",
        "voz",
        nine,
        _clip_regions(nine_regions, len(nine)),
        "9 s con voz tras 1 s de silencio",
    )
    four = _fixed_length(revisar, 4.0)
    add("voice_dora_4s", "voz", four, [[0, min(len(revisar), 4 * RATE)]], "4 s exactos")
    alex_eight = _fixed_length(registro, 8.0)
    add(
        "voice_alex_8s",
        "voz",
        alex_eight,
        [[0, min(len(registro), 8 * RATE)]],
        "8 s con voz masculina",
    )
    mixed, mixed_regions = _join(
        [
            (_silence(1.0), False),
            (_tone(300.0, 1.0, 0.1), False),
            (_noise(1.0, 0.02, 23), False),
            (revisar, True),
        ]
    )
    mixed = _fixed_length(mixed, 8.0)
    add("mixed_all_8s", "mezcla", mixed, mixed_regions, "silencio, tono, ruido y voz en 8 s")
    long_tail, long_tail_regions = _join([(escuchame, True), (_silence(7.0), False)])
    long_tail = _fixed_length(long_tail, 10.0)
    add("voice_dora_silence_tail_10s", "voz", long_tail, long_tail_regions, "voz y 9 s de silencio")

    synthetic = _voiced_harmonics(1.0, 0.25, 31)
    add(
        "synthetic_voiced_1s",
        "sintética",
        synthetic,
        [[0, len(synthetic)]],
        "voz armónica sintética",
    )
    synthetic3 = _voiced_harmonics(3.0, 0.2, 32)
    add(
        "synthetic_voiced_3s",
        "sintética",
        synthetic3,
        [[0, len(synthetic3)]],
        "voz armónica de 3 s",
    )
    synthetic8 = _voiced_harmonics(8.0, 0.2, 33)
    add(
        "synthetic_voiced_8s",
        "sintética",
        synthetic8,
        [[0, len(synthetic8)]],
        "voz armónica de 8 s",
    )
    syllabic = _syllables(2.0, 0.2, 34)
    add("synthetic_syllables_2s", "sintética", syllabic, [[0, len(syllabic)]], "sílabas sintéticas")

    clicks_only = np.zeros(1 * RATE)
    for position in (0.05, 0.3, 0.55, 0.8):
        index = int(position * RATE)
        clicks_only[index] = 0.9
    add("clicks_1s", "clics", clicks_only, [], "cuatro clics aislados")

    blips = np.zeros(3 * RATE)
    for position in (0.5, 1.5, 2.5):
        index = int(position * RATE)
        blips[index : index + 80] = 0.7 * np.hanning(80)
    add("pauses_and_blips_3s", "clics", blips, [], "pausas largas con chasquidos")

    return items


# --------------------------------------------------------------- WAV como caso
# DECISION: cada WAV es un caso `fn: "wav"` con su sha256 y sus regiones de voz
# en muestras; así los límites quedan marcados junto al audio sin un índice
# aparte en `parity/golden/`.
def _add_wav_cases(out: FixtureFile, corpus: list[dict[str, Any]]) -> dict[str, bytes]:
    pcm_cache: dict[str, bytes] = {}
    for item in corpus:
        name = item["name"]
        path = _write_wav(name, item["samples"])
        pcm = _read_wav_pcm(path)
        pcm_cache[name] = pcm
        relative = path.relative_to(ROOT).as_posix()
        out.add(
            f"audio/wav/{name}",
            "wav",
            {"path": relative},
            {
                "sha256": sha256(path),
                "bytes": path.stat().st_size,
                "samples": len(pcm) // 2,
                "rate": RATE,
                "channels": 1,
                "kind": item["kind"],
                "note": item["note"],
                "duration_secs": len(pcm) / 2 / RATE,
                "regions": item["regions"],
                "peak": float(np.max(np.abs(_samples_of(pcm)))) if pcm else 0.0,
                "rms": pcm_rms(pcm),
            },
        )
    return pcm_cache


# ------------------------------------------------------------------- pcm_rms
PCM_RMS_CASES: list[tuple[str, dict[str, Any]]] = [
    ("empty", {"kind": "hex", "hex": ""}),
    ("odd", {"kind": "hex", "hex": "01"}),
    ("zero_sample", {"kind": "hex", "hex": "0000"}),
    ("zero_two", {"kind": "hex", "hex": "00000000"}),
    ("minus_one", {"kind": "hex", "hex": "ffff"}),
    ("min_i16", {"kind": "hex", "hex": "0080"}),
    ("max_i16", {"kind": "hex", "hex": "ff7f"}),
    ("half_i16", {"kind": "hex", "hex": "0080ffff"}),
    ("two_values", {"kind": "hex", "hex": "00004000"}),
    ("three_bytes", {"kind": "hex", "hex": "00004000ff"}),
    ("ramp_960", {"kind": "pattern", "bytes": 960, "pattern": "ramp", "seed": 0}),
    ("ramp_961", {"kind": "pattern", "bytes": 961, "pattern": "ramp", "seed": 0}),
    ("ramp_32000", {"kind": "pattern", "bytes": 32000, "pattern": "ramp", "seed": 7}),
    ("silence_1s", {"kind": "wav", "wav": "parity/golden/audio/silence_1s.wav"}),
    (
        "silence_1s_chunk",
        {"kind": "wav", "wav": "parity/golden/audio/silence_1s.wav", "length": 960},
    ),
    (
        "tone_440_chunk",
        {"kind": "wav", "wav": "parity/golden/audio/tone_440_1s.wav", "length": 960},
    ),
    (
        "tone_440_mid",
        {"kind": "wav", "wav": "parity/golden/audio/tone_440_1s.wav", "start": 8000, "length": 961},
    ),
    (
        "voice_chunk",
        {"kind": "wav", "wav": "parity/golden/audio/voice_dora_corto.wav", "length": 960},
    ),
    ("voice_full", {"kind": "wav", "wav": "parity/golden/audio/voice_dora_corto.wav"}),
    (
        "noise_chunk",
        {"kind": "wav", "wav": "parity/golden/audio/noise_white_1s.wav", "length": 960},
    ),
    (
        "clicks_chunk",
        {"kind": "wav", "wav": "parity/golden/audio/clicks_1s.wav", "start": 1440, "length": 960},
    ),
    (
        "clipped_loud",
        {"kind": "wav", "wav": "parity/golden/audio/voice_dora_loud.wav", "length": 4800},
    ),
    ("quiet", {"kind": "wav", "wav": "parity/golden/audio/voice_dora_quiet.wav", "length": 4800}),
]


def _add_pcm_rms(out: FixtureFile) -> None:
    for index, (name, spec) in enumerate(PCM_RMS_CASES, start=1):
        out.add(
            f"audio/pcm_rms/{index:04d}_{name}",
            "pcm_rms",
            {"pcm": spec},
            {"rms": pcm_rms(_pcm_bytes(spec))},
        )


# --------------------------------------------------------------- pcm_is_weak
PCM_WEAK_CASES: list[tuple[str, dict[str, Any], int]] = [
    ("empty", {"kind": "hex", "hex": ""}, 16000),
    ("one_byte", {"kind": "hex", "hex": "00"}, 16000),
    ("one_sample", {"kind": "hex", "hex": "ffff"}, 16000),
    ("short_100", {"kind": "pattern", "bytes": 200, "pattern": "ramp", "seed": 0}, 16000),
    ("silence_1s", {"kind": "wav", "wav": "parity/golden/audio/silence_1s.wav"}, 16000),
    (
        "silence_chunk",
        {"kind": "wav", "wav": "parity/golden/audio/silence_1s.wav", "length": 960},
        16000,
    ),
    ("quiet_voice", {"kind": "wav", "wav": "parity/golden/audio/voice_dora_quiet.wav"}, 16000),
    ("whisper", {"kind": "wav", "wav": "parity/golden/audio/voice_dora_whisper.wav"}, 16000),
    ("voice", {"kind": "wav", "wav": "parity/golden/audio/voice_dora_corto.wav"}, 16000),
    (
        "voice_short_burst",
        {"kind": "wav", "wav": "parity/golden/audio/voice_dora_short_burst.wav"},
        16000,
    ),
    ("tone", {"kind": "wav", "wav": "parity/golden/audio/tone_440_1s.wav"}, 16000),
    ("tone_beeps", {"kind": "wav", "wav": "parity/golden/audio/tone_beeps_2s.wav"}, 16000),
    ("noise", {"kind": "wav", "wav": "parity/golden/audio/noise_white_1s.wav"}, 16000),
    ("noise_low", {"kind": "wav", "wav": "parity/golden/audio/noise_low_2s.wav"}, 16000),
    ("clicks", {"kind": "wav", "wav": "parity/golden/audio/clicks_1s.wav"}, 16000),
    ("voice_rate_8k", {"kind": "wav", "wav": "parity/golden/audio/voice_dora_corto.wav"}, 8000),
    ("voice_rate_48k", {"kind": "wav", "wav": "parity/golden/audio/voice_dora_corto.wav"}, 48000),
    ("voice_rate_zero", {"kind": "wav", "wav": "parity/golden/audio/voice_dora_corto.wav"}, 0),
    (
        "voice_rate_negative",
        {"kind": "wav", "wav": "parity/golden/audio/voice_dora_corto.wav"},
        -16000,
    ),
    ("odd_bytes", {"kind": "pattern", "bytes": 1921, "pattern": "ramp", "seed": 3}, 16000),
    (
        "click_then_silence",
        {"kind": "wav", "wav": "parity/golden/audio/pauses_and_blips_3s.wav"},
        16000,
    ),
    (
        "voice_plus_noise",
        {"kind": "wav", "wav": "parity/golden/audio/voice_dora_noise_3db.wav"},
        16000,
    ),
]


def _add_pcm_is_weak(out: FixtureFile) -> None:
    for index, (name, spec, rate) in enumerate(PCM_WEAK_CASES, start=1):
        pcm = _pcm_bytes(spec)
        out.add(
            f"audio/pcm_is_weak/{index:04d}_{name}",
            "pcm_is_weak",
            {"pcm": spec, "rate": rate},
            {"weak": pcm_is_weak(pcm, rate)},
        )


# --------------------------------------------------------------- pcm_to_wav
PCM_TO_WAV_CASES: list[tuple[str, dict[str, Any], int]] = [
    ("empty", {"kind": "hex", "hex": ""}, 16000),
    ("one_sample", {"kind": "hex", "hex": "0100"}, 16000),
    ("two_samples", {"kind": "hex", "hex": "01000200"}, 16000),
    ("odd_three", {"kind": "hex", "hex": "01000200ff"}, 16000),
    ("chunk", {"kind": "pattern", "bytes": 960, "pattern": "ramp", "seed": 0}, 16000),
    ("rate_8k", {"kind": "pattern", "bytes": 960, "pattern": "ramp", "seed": 1}, 8000),
    ("rate_22050", {"kind": "pattern", "bytes": 960, "pattern": "ramp", "seed": 2}, 22050),
    ("rate_24000", {"kind": "pattern", "bytes": 960, "pattern": "ramp", "seed": 3}, 24000),
    ("rate_44100", {"kind": "pattern", "bytes": 960, "pattern": "ramp", "seed": 4}, 44100),
    ("rate_48000", {"kind": "pattern", "bytes": 960, "pattern": "ramp", "seed": 5}, 48000),
    ("rate_70000", {"kind": "pattern", "bytes": 4, "pattern": "ramp", "seed": 7}, 70000),
]


def _wav_header(wav: bytes) -> dict[str, Any]:
    with wave.open(io.BytesIO(wav), "rb") as handle:
        return {
            "channels": handle.getnchannels(),
            "sample_width": handle.getsampwidth(),
            "rate": handle.getframerate(),
            "frames": handle.getnframes(),
        }


def _add_pcm_to_wav(out: FixtureFile) -> None:
    for index, (name, spec, rate) in enumerate(PCM_TO_WAV_CASES, start=1):
        pcm = _pcm_bytes(spec)
        wav = pcm_to_wav(pcm, rate)
        out.add(
            f"audio/pcm_to_wav/{index:04d}_{name}",
            "pcm_to_wav",
            {"pcm": spec, "rate": rate},
            {
                "bytes": len(wav),
                "sha256": hashlib.sha256(wav).hexdigest(),
                "header": _wav_header(wav),
            },
        )
    for index, rate in enumerate([-1, -16000, 0], start=1):
        try:
            pcm_to_wav(b"\x00\x00", rate)
        except Exception as error:
            out.add_error(
                f"audio/pcm_to_wav/err{index:02d}",
                "pcm_to_wav",
                {"pcm": {"kind": "hex", "hex": "0000"}, "rate": rate},
                str(error),
            )


# --------------------------------------------------------------- remuestreo
def _samples_input(spec: dict[str, Any]) -> np.ndarray:
    if "samples_i16" in spec:
        return np.asarray(spec["samples_i16"], dtype=np.float32) / 32768.0
    if spec["kind"] == "ramp_i16":
        count = int(spec["count"])
        start = int(spec.get("start", -12000))
        step = int(spec.get("step", 37))
        values = start + step * np.arange(count)
        modulus = 60000
        wrapped = ((values + 30000) % modulus) - 30000
        return np.asarray(wrapped, dtype=np.int16).astype(np.float32) / 32768.0
    raise ValueError(f"muestras desconocidas: {spec}")


RESAMPLE_CASES: list[tuple[str, dict[str, Any], int, int]] = [
    ("empty", {"kind": "ramp_i16", "count": 0}, 16000, 8000),
    ("one_to_8k", {"samples_i16": [16384]}, 16000, 8000),
    ("two_to_8k", {"samples_i16": [0, 16384]}, 16000, 8000),
    ("three_to_8k", {"samples_i16": [0, 8192, 16384]}, 16000, 8000),
    ("three_to_48k", {"samples_i16": [0, 8192, 16384]}, 16000, 48000),
    ("five_to_8k", {"samples_i16": [0, 4096, 8192, 12288, 16384]}, 16000, 8000),
    ("seven_to_16k", {"samples_i16": [-16384, -10922, -5461, 0, 5461, 10922, 16384]}, 8000, 16000),
    ("same_rate", {"samples_i16": [0, 1000, -1000, 32767]}, 16000, 16000),
    ("ramp_480_16k_to_22k", {"kind": "ramp_i16", "count": 480}, 16000, 22050),
    ("ramp_480_22k_to_16k", {"kind": "ramp_i16", "count": 480}, 22050, 16000),
    ("ramp_1000_44k_to_16k", {"kind": "ramp_i16", "count": 1000}, 44100, 16000),
    ("ramp_1000_16k_to_44k", {"kind": "ramp_i16", "count": 1000}, 16000, 44100),
    ("ramp_1000_48k_to_44k", {"kind": "ramp_i16", "count": 1000}, 48000, 44100),
    ("ramp_1000_24k_to_16k", {"kind": "ramp_i16", "count": 1000}, 24000, 16000),
    ("ramp_1000_16k_to_24k", {"kind": "ramp_i16", "count": 1000}, 16000, 24000),
    ("ramp_1000_8k_to_22k", {"kind": "ramp_i16", "count": 1000}, 8000, 22050),
    ("ramp_1000_22k_to_8k", {"kind": "ramp_i16", "count": 1000}, 22050, 8000),
    ("ramp_1000_44k_to_48k", {"kind": "ramp_i16", "count": 1000}, 44100, 48000),
    ("ramp_1000_48k_to_8k", {"kind": "ramp_i16", "count": 1000}, 48000, 8000),
    ("tone_480", {"kind": "ramp_i16", "count": 480, "start": 0, "step": 137}, 16000, 24000),
    ("short_5_48k_to_16k", {"samples_i16": [0, 1000, -1000, 500, -500]}, 48000, 16000),
    ("short_5_16k_to_48k", {"samples_i16": [0, 1000, -1000, 500, -500]}, 16000, 48000),
    ("rate_zero", {"samples_i16": [0, 1000]}, 0, 16000),
    ("rate_negative", {"samples_i16": [0, 1000]}, -16000, 16000),
    ("to_zero", {"samples_i16": [0, 1000]}, 16000, 0),
]

RESAMPLE_LENGTH_CASES: list[tuple[int, int, int]] = [
    (0, 16000, 8000),
    (1, 16000, 8000),
    (2, 16000, 8000),
    (3, 16000, 8000),
    (5, 16000, 8000),
    (7, 8000, 16000),
    (1, 8000, 16000),
    (2, 48000, 16000),
    (3, 48000, 16000),
    (480, 16000, 22050),
    (1000, 44100, 16000),
    (1000, 16000, 44100),
    (1000, 48000, 44100),
    (16, 16000, 16000),
    (960, 24000, 16000),
    (960, 16000, 24000),
    (9, 8000, 22050),
    (9, 22050, 8000),
    (1, 16000, 48000),
    (1, 48000, 16000),
    (11, 44100, 48000),
    (11, 48000, 44100),
    (13, 22050, 16000),
    (13, 16000, 22050),
    (15, 8000, 48000),
    (15, 48000, 8000),
    (17, 16000, 8000),
    (17, 8000, 16000),
    (3, 16000, 0),
]


def _add_resample(out: FixtureFile) -> None:
    for index, (name, spec, from_rate, to_rate) in enumerate(RESAMPLE_CASES, start=1):
        samples = _samples_input(spec)
        try:
            result = _resampled(samples, from_rate, to_rate)
        except Exception as error:
            out.add_error(
                f"audio/resample_linear/{index:04d}_{name}",
                "resample_linear",
                {"samples": spec, "from_rate": from_rate, "to_rate": to_rate},
                str(error),
            )
            continue
        values = result.tolist()
        out.add(
            f"audio/resample_linear/{index:04d}_{name}",
            "resample_linear",
            {"samples": spec, "from_rate": from_rate, "to_rate": to_rate},
            {
                "length": len(values),
                "head": values[:5],
                "tail": values[-5:],
                "sum": float(np.sum(result, dtype=np.float64)),
                "sum_squares": float(np.sum(np.square(result), dtype=np.float64)),
            },
        )
    for index, (length, from_rate, to_rate) in enumerate(RESAMPLE_LENGTH_CASES, start=1):
        try:
            value = int(round(length * to_rate / from_rate))
        except Exception as error:
            out.add_error(
                f"audio/resample_length/{index:04d}",
                "resample_length",
                {"length": length, "from_rate": from_rate, "to_rate": to_rate},
                str(error),
            )
            continue
        out.add(
            f"audio/resample_length/{index:04d}",
            "resample_length",
            {"length": length, "from_rate": from_rate, "to_rate": to_rate},
            {"length": value},
        )
    for index, (length, from_rate, to_rate) in enumerate([(3, 0, 16000), (0, 0, 16000)], start=1):
        try:
            int(round(length * to_rate / from_rate))
        except Exception as error:
            message = str(error)
        else:
            message = "no falló"
        out.add_error(
            f"audio/resample_length/err{index:02d}",
            "resample_length",
            {"length": length, "from_rate": from_rate, "to_rate": to_rate},
            message,
        )


# ------------------------------------------------------------------ barge-in
def _barge_chunk(index: int) -> bytes:
    return bytes([index % 256, (index * 7) % 256, (index * 13) % 256, (index * 29) % 256])


BARGE_CASES: list[tuple[str, float, float, list[float]]] = [
    ("sustained", 0.008, BARGE_IN_SECS, [0.1] * 9),
    ("short", 0.008, BARGE_IN_SECS, [0.1] * 8),
    ("gap_one", 0.008, BARGE_IN_SECS, [0.1] * 8 + [0.001] + [0.1]),
    ("gap_three", 0.008, BARGE_IN_SECS, [0.1] * 4 + [0.0] * 3 + [0.1] * 5),
    ("gap_four_resets", 0.008, BARGE_IN_SECS, [0.1] * 4 + [0.0] * 4 + [0.1] * 5),
    ("gap_four_then_late", 0.008, BARGE_IN_SECS, [0.1] * 4 + [0.0] * 4 + [0.1] * 8),
    ("at_threshold", 0.008, BARGE_IN_SECS, [0.008] * 9),
    ("below_threshold", 0.008, BARGE_IN_SECS, [0.0079] * 12),
    ("zero_threshold", 0.0, BARGE_IN_SECS, [0.0] * 9),
    ("negative_threshold", -1.0, BARGE_IN_SECS, [0.0] * 9),
    ("need_one", 0.008, 0.0, [0.1]),
    ("need_four", 0.008, 0.1, [0.1] * 4),
    ("need_nine", 0.008, 0.26, [0.1] * 9),
    ("need_ten_float", 0.008, 0.27, [0.1] * 10),
    ("negative_secs", 0.008, -1.0, [0.1]),
    ("reset_after_confirm", 0.008, BARGE_IN_SECS, [0.1] * 9 + [0.0] * 2 + [0.1]),
    ("two_runs", 0.008, BARGE_IN_SECS, [0.1] * 9 + [0.0] * 5 + [0.1] * 9),
    ("all_low", 0.008, BARGE_IN_SECS, [0.0] * 10),
    ("long_run", 0.008, BARGE_IN_SECS, [0.1] * 30),
    ("need_34", 0.008, 1.0, [0.1] * 34),
    ("silence_at_start", 0.008, BARGE_IN_SECS, [0.0] * 5 + [0.1] * 9),
    ("gaps_between_syllables", 0.008, BARGE_IN_SECS, ([0.1] * 3 + [0.0]) * 4),
]


def _add_barge_in(out: FixtureFile) -> None:
    for index, (name, threshold, secs, levels) in enumerate(BARGE_CASES, start=1):
        detector = BargeInDetector(threshold=threshold, secs=secs)
        runs: list[dict[str, Any]] = []
        for feed, level in enumerate(levels):
            run = detector.feed(_barge_chunk(feed), level)
            if run is None:
                continue
            first = feed - len(run) + 1
            chunks = list(range(first, feed + 1))
            payload = b"".join(chunk for chunk, _ in run)
            runs.append(
                {
                    "at": feed,
                    "chunks": chunks,
                    "run": [[index, levels[index]] for index in chunks],
                    "bytes": len(payload),
                    "sha256": hashlib.sha256(payload).hexdigest(),
                }
            )
        out.add(
            f"audio/barge_in/{index:04d}_{name}",
            "barge_in",
            {"threshold": threshold, "secs": secs, "levels": levels},
            {"need": BargeInDetector(threshold=threshold, secs=secs).need, "runs": runs},
        )
    for index, secs in enumerate(
        [BARGE_IN_SECS, 0.0, -1.0, 0.03, 0.1, 0.26, 0.27, 0.3, 1.0, 2.5], start=1
    ):
        out.add(
            f"audio/barge_in_need/{index:04d}",
            "barge_in_need",
            {"secs": secs},
            {"need": BargeInDetector(secs=secs).need},
        )


# ----------------------------------------------------------------- segmentador
def _default_config() -> dict[str, Any]:
    segmenter = Segmenter()
    return {
        "min_rms": segmenter.min_rms,
        "silence_secs": segmenter.silence_secs,
        "pre_roll_secs": segmenter.pre_roll_secs,
        "min_speech_secs": segmenter.min_speech_secs,
        "max_utterance_secs": segmenter.max_utterance_secs,
        "noise_multiplier": segmenter.noise_multiplier,
        "quick_silence_secs": segmenter.quick_silence_secs,
        "min_voiced_secs": segmenter.min_voiced_secs,
        "probe_interval_secs": segmenter.probe_interval_secs,
    }


class _ScriptedFuture(Future):  # type: ignore[type-arg]
    def __init__(self, outcome: str, checks: int = 1) -> None:
        super().__init__()
        self.outcome = outcome
        self.checks = checks

    def done(self) -> bool:
        if self.outcome == "pending":
            self.checks -= 1
            return self.checks <= 0
        return True

    def result(self) -> bool:
        if self.outcome == "raise":
            raise RuntimeError("sonda rota")
        return self.outcome in ("true", "pending")


def _probe_for(
    spec: dict[str, Any],
    state: dict[str, Any],
    calls: list[dict[str, Any]],
) -> Callable[[bytes], Future[bool] | None]:
    mode = spec["mode"]

    def record(pcm: bytes) -> None:
        calls.append(
            {
                "chunk": state["chunk"],
                "tail_bytes": len(pcm),
                "tail_chunks": len(pcm) // state["chunk_bytes"],
                "tail_sha256": hashlib.sha256(pcm).hexdigest(),
                "silence_secs": round(state["segmenter"]._silence_run, 10),
            }
        )

    if mode == "none":

        def none_probe(pcm: bytes) -> Future[bool] | None:
            record(pcm)
            return None

        return none_probe
    if mode == "raise":

        def raise_probe(pcm: bytes) -> Future[bool] | None:
            record(pcm)
            raise RuntimeError("sonda rota")

        return raise_probe
    if mode == "true":

        def true_probe(pcm: bytes) -> Future[bool] | None:
            record(pcm)
            return _ScriptedFuture("true")

        return true_probe
    if mode == "raise_result":

        def raise_result(pcm: bytes) -> Future[bool] | None:
            record(pcm)
            return _ScriptedFuture("raise")

        return raise_result
    if mode == "pending_then_true":
        checks = int(spec.get("checks", 3))

        def pending(pcm: bytes) -> Future[bool] | None:
            record(pcm)
            return _ScriptedFuture("pending", checks)

        return pending
    if mode == "false_then_true":
        remaining = int(spec.get("false_count", 1))

        def false_then(pcm: bytes) -> Future[bool] | None:
            nonlocal remaining
            record(pcm)
            if remaining > 0:
                remaining -= 1
                return _ScriptedFuture("false")
            return _ScriptedFuture("true")

        return false_then
    raise ValueError(f"sonda desconocida: {mode}")


def _speech_end_chunk(value: float | None, origin: float) -> int | None:
    if value is None:
        return None
    return int(round((value - origin) / CHUNK_SECS)) - 1


# DECISION: se congela `time.monotonic` a pasos de CHUNK_SECS para que
# `last_speech_end` y `speech_end_chunk` sean reproducibles; el Python real usa
# el reloj de pared y ese valor solo se compara como posición de chunk.
def _run_segmenter(
    chunks: list[bytes],
    levels: list[float],
    chunk_bytes: int,
    config: dict[str, Any],
    probe: dict[str, Any] | None,
    pre_roll_at: dict[str, Any],
    flush_at: list[int],
    reset_at: list[int],
    flush_end: bool,
) -> dict[str, Any]:
    clock = [1000.0]
    origin = clock[0]
    original = time.monotonic
    state: dict[str, Any] = {"chunk": 0, "chunk_bytes": chunk_bytes, "segmenter": None}
    calls: list[dict[str, Any]] = []
    end_probe = _probe_for(probe, state, calls) if probe is not None else None
    settings = _default_config()
    settings.update(config)
    segmenter = Segmenter(end_probe=end_probe, **settings)
    state["segmenter"] = segmenter
    events: list[dict[str, Any]] = []
    updates: list[dict[str, Any]] = []
    flushes: list[dict[str, Any]] = []
    time.monotonic = lambda: clock[0]
    try:
        for index, (chunk, level) in enumerate(zip(chunks, levels, strict=True)):
            if str(index) in pre_roll_at:
                segmenter.set_pre_roll(pre_roll_at[str(index)])
                updates.append(
                    {
                        "chunk": index,
                        "pre_roll_secs": segmenter.pre_roll_secs,
                        "capacity": segmenter._pre.maxlen,
                    }
                )
            if index in flush_at:
                flushes.append({"chunk": index, "pcm": _pcm_info(segmenter.flush())})
            if index in reset_at:
                was_in_speech = segmenter._in_speech
                segmenter.reset()
                events.append(
                    {
                        "chunk": index,
                        "event": "reset",
                        "was_in_speech": was_in_speech,
                        "noise": segmenter.noise,
                    }
                )
            state["chunk"] = index
            clock[0] = origin + (index + 1) * CHUNK_SECS
            pre_before = len(segmenter._pre)
            pcm, started = segmenter.feed(chunk, level)
            if started:
                start_threshold, end_threshold = segmenter.thresholds()
                events.append(
                    {
                        "chunk": index,
                        "event": "speech_start",
                        "level": level,
                        "noise": segmenter.noise,
                        "start_threshold": start_threshold,
                        "end_threshold": end_threshold,
                        "pre_chunks": pre_before,
                        "pre_capacity": segmenter._pre.maxlen,
                    }
                )
            if pcm is not None:
                events.append(
                    {
                        "chunk": index,
                        "event": "utterance",
                        "discarded": pcm == b"",
                        "pcm": _pcm_info(pcm),
                        "close_reason": segmenter.last_close_reason,
                        "speech_end_chunk": _speech_end_chunk(segmenter.last_speech_end, origin),
                    }
                )
        if flush_end:
            flushes.append({"chunk": len(chunks), "pcm": _pcm_info(segmenter.flush())})
    finally:
        time.monotonic = original
    start_threshold, end_threshold = segmenter.thresholds()
    return {
        "events": events,
        "probe_calls": calls,
        "pre_roll_updates": updates,
        "flushes": flushes,
        "final": {
            "in_speech": segmenter._in_speech,
            "noise": segmenter.noise,
            "pre_roll_secs": segmenter.pre_roll_secs,
            "last_close_reason": segmenter.last_close_reason,
            "last_speech_end_set": segmenter.last_speech_end is not None,
            "start_threshold": start_threshold,
            "end_threshold": end_threshold,
            "speech_secs": round(segmenter._speech_secs, 10),
            "silence_secs": round(segmenter._silence_run, 10),
            "speech_run": segmenter._speech_run,
        },
    }


def _wav_chunks(pcm: bytes, chunk_bytes: int) -> tuple[list[bytes], int]:
    usable = len(pcm) - len(pcm) % chunk_bytes
    chunks = [pcm[start : start + chunk_bytes] for start in range(0, usable, chunk_bytes)]
    return chunks, len(pcm) - usable


def _add_segmenter_wav(
    out: FixtureFile,
    name: str,
    config: dict[str, Any],
    probe: dict[str, Any] | None = None,
    pre_roll_at: dict[str, Any] | None = None,
    flush_at: list[int] | None = None,
    reset_at: list[int] | None = None,
    flush_end: bool = False,
    chunk_bytes: int = 960,
    label: str | None = None,
) -> None:
    pcm = _read_wav_pcm(WAV_DIR / f"{name}.wav")
    chunks, ignored = _wav_chunks(pcm, chunk_bytes)
    levels = [pcm_rms(chunk) for chunk in chunks]
    result = _run_segmenter(
        chunks,
        levels,
        chunk_bytes,
        config,
        probe,
        pre_roll_at or {},
        flush_at or [],
        reset_at or [],
        flush_end,
    )
    result["ignored_tail_bytes"] = ignored
    out.add(
        f"audio/segmenter/wav-{label or name}",
        "segmenter",
        {
            "wav": f"parity/golden/audio/{name}.wav",
            "wav_sha256": sha256(WAV_DIR / f"{name}.wav"),
            "rate": RATE,
            "chunk_samples": chunk_bytes // 2,
            "chunk_bytes": chunk_bytes,
            "drop_partial_chunk": True,
            "config": config,
            "probe": probe,
            "pre_roll_at": pre_roll_at or {},
            "flush_at": flush_at or [],
            "reset_at": reset_at or [],
            "flush_end": flush_end,
        },
        result,
    )


def _add_segmenter_script(
    out: FixtureFile,
    name: str,
    levels: list[float],
    config: dict[str, Any],
    probe: dict[str, Any] | None = None,
    pre_roll_at: dict[str, Any] | None = None,
    flush_at: list[int] | None = None,
    reset_at: list[int] | None = None,
    flush_end: bool = False,
    chunk_bytes: int = 64,
    seed: int = 0,
) -> None:
    chunks = [
        bytes((seed + index * chunk_bytes + offset) % 256 for offset in range(chunk_bytes))
        for index in range(len(levels))
    ]
    result = _run_segmenter(
        chunks,
        levels,
        chunk_bytes,
        config,
        probe,
        pre_roll_at or {},
        flush_at or [],
        reset_at or [],
        flush_end,
    )
    out.add(
        f"audio/segmenter/{name}",
        "segmenter",
        {
            "levels": levels,
            "chunk_pattern": {"bytes": chunk_bytes, "seed": seed},
            "config": config,
            "probe": probe,
            "pre_roll_at": pre_roll_at or {},
            "flush_at": flush_at or [],
            "reset_at": reset_at or [],
            "flush_end": flush_end,
        },
        result,
    )


def _add_segmenter(out: FixtureFile) -> None:
    _add_segmenter_wav(out, "silence_1s", {})
    _add_segmenter_wav(out, "silence_3s", {"silence_secs": 0.5})
    _add_segmenter_wav(out, "voice_dora_corto", {"silence_secs": 0.3})
    _add_segmenter_wav(out, "voice_dora_largo", {"silence_secs": 0.4})
    _add_segmenter_wav(out, "voice_dora_leading_silence", {})
    _add_segmenter_wav(out, "voice_dora_trailing_silence", {"silence_secs": 0.5})
    _add_segmenter_wav(
        out,
        "voice_dora_pause_voice",
        {"silence_secs": 0.7, "min_speech_secs": 0.1},
    )
    _add_segmenter_wav(out, "voice_dora_clipped_start", {})
    _add_segmenter_wav(out, "voice_dora_clipped_end", {})
    _add_segmenter_wav(
        out,
        "voice_mixed_conversation",
        {"silence_secs": 0.5, "pre_roll_secs": 0.15},
    )
    _add_segmenter_wav(out, "voice_dora_noise_10db", {})
    _add_segmenter_wav(out, "voice_dora_noise_3db", {"noise_multiplier": 2.0})
    _add_segmenter_wav(out, "voice_dora_quiet", {"min_rms": 0.001})
    _add_segmenter_wav(out, "voice_dora_whisper", {"min_rms": 0.0003})
    _add_segmenter_wav(out, "tone_440_1s", {})
    _add_segmenter_wav(out, "tone_beeps_2s", {"silence_secs": 0.3})
    _add_segmenter_wav(out, "noise_white_1s", {})
    _add_segmenter_wav(out, "noise_low_2s", {"min_rms": 0.0002})
    _add_segmenter_wav(out, "clicks_1s", {})
    _add_segmenter_wav(out, "synthetic_voiced_3s", {})
    _add_segmenter_wav(out, "synthetic_syllables_2s", {"silence_secs": 0.4})
    _add_segmenter_wav(out, "mixed_all_8s", {"silence_secs": 0.8})
    _add_segmenter_wav(out, "voice_dora_8s", {"max_utterance_secs": 1.0})
    _add_segmenter_wav(
        out,
        "voice_dora_9s",
        {"max_utterance_secs": 2.0, "min_speech_secs": 2.0},
    )
    _add_segmenter_wav(out, "voice_dora_4s", {"pre_roll_secs": 0.0})
    _add_segmenter_wav(
        out,
        "voice_dora_escuchame",
        {"pre_roll_secs": 1.0, "silence_secs": 0.35},
    )
    _add_segmenter_wav(
        out,
        "voice_alex_largo",
        {"silence_secs": 0.3, "min_speech_secs": 0.05, "min_voiced_secs": 0.0},
    )
    _add_segmenter_wav(
        out,
        "voice_dora_dos_frases",
        {"silence_secs": 0.35, "pre_roll_secs": 0.3},
    )
    _add_segmenter_wav(out, "voice_santa_corto", {"silence_secs": 0.4})
    _add_segmenter_wav(out, "voice_santa_largo", {"silence_secs": 0.4})
    _add_segmenter_wav(
        out,
        "voice_alex_corto",
        {"silence_secs": 0.2, "min_speech_secs": 0.05},
    )
    _add_segmenter_wav(out, "voice_dora_no", {"silence_secs": 0.3})
    _add_segmenter_wav(out, "voice_dora_tone_mix", {})
    _add_segmenter_wav(out, "noise_bursts_2s", {})
    _add_segmenter_wav(out, "noise_clicks_1s", {})
    _add_segmenter_wav(out, "silence_10s", {"silence_secs": 0.5})
    _add_segmenter_wav(out, "synthetic_voiced_1s", {})
    _add_segmenter_wav(out, "tone_1k_0_5s", {})
    _add_segmenter_wav(out, "tone_220_2s", {})

    _add_segmenter_wav(
        out,
        "voice_dora_trailing_silence",
        {"silence_secs": 2.0},
        probe={"mode": "true"},
        label="voice_dora_probe_true",
    )
    _add_segmenter_wav(
        out,
        "voice_dora_trailing_silence",
        {"silence_secs": 2.0, "quick_silence_secs": 0.2, "probe_interval_secs": 0.1},
        probe={"mode": "false_then_true", "false_count": 1},
        label="voice_dora_probe_false_then_true",
    )
    _add_segmenter_wav(
        out,
        "voice_dora_trailing_silence",
        {"silence_secs": 1.5, "quick_silence_secs": 0.2},
        probe={"mode": "none"},
        label="voice_dora_probe_none",
    )
    _add_segmenter_wav(
        out,
        "voice_dora_trailing_silence",
        {"silence_secs": 1.5, "quick_silence_secs": 0.2},
        probe={"mode": "raise"},
        label="voice_dora_probe_raise",
    )
    _add_segmenter_wav(
        out,
        "voice_dora_trailing_silence",
        {"silence_secs": 1.5, "quick_silence_secs": 0.2},
        probe={"mode": "raise_result"},
        label="voice_dora_probe_raise_result",
    )
    _add_segmenter_wav(
        out,
        "voice_dora_silence_tail_10s",
        {"silence_secs": 2.0, "quick_silence_secs": 0.3},
        probe={"mode": "pending_then_true", "checks": 2},
        label="voice_dora_probe_pending",
    )


def _add_segmenter_scripts(out: FixtureFile) -> None:
    _add_segmenter_script(
        out,
        "script-thresholds",
        [0.0, 0.0002, 0.0005, 0.00059, 0.0006, 0.0007, 0.001, 0.002] * 3,
        {},
    )
    _add_segmenter_script(
        out,
        "script-calibration-jump",
        [0.001, 0.003, 0.005, 0.01] + [0.0] * 30,
        {},
    )
    _add_segmenter_script(
        out,
        "script-calibration-steady",
        [0.004] * 40,
        {},
    )
    _add_segmenter_script(
        out,
        "script-two-loud-only",
        [0.0] * 5 + [0.05] * 2 + [0.0] * 10,
        {},
    )
    _add_segmenter_script(
        out,
        "script-three-loud-start",
        [0.0] * 5 + [0.05] * 3 + [0.0] * 10,
        {},
    )
    _add_segmenter_script(
        out,
        "script-min-voiced",
        [0.0] * 5 + [0.05] * 4 + [0.0] * 3 + [0.05] * 2 + [0.0] * 60,
        {"min_voiced_secs": 0.2},
    )
    _add_segmenter_script(
        out,
        "script-min-speech",
        [0.0] * 5 + [0.05] * 5 + [0.0] * 60,
        {"min_speech_secs": 2.0, "silence_secs": 0.2},
    )
    for pre_roll in (0.0, 0.15, 0.6, 1.0, 2.0):
        _add_segmenter_script(
            out,
            f"script-pre-roll-{str(pre_roll).replace('.', '_')}",
            [0.0] * 5 + [0.05] * 12 + [0.0] * 50,
            {"pre_roll_secs": pre_roll, "silence_secs": 0.5},
        )
    _add_segmenter_script(
        out,
        "script-pre-roll-long-silence",
        [0.0] * 40 + [0.05] * 12 + [0.0] * 50,
        {"pre_roll_secs": 1.0, "silence_secs": 0.5},
    )
    _add_segmenter_script(
        out,
        "script-pre-roll-live",
        [0.0] * 5 + [0.05] * 12 + [0.0] * 50,
        {"silence_secs": 0.5},
        pre_roll_at={"3": 0.0, "10": 0.9, "14": None, "15": "x", "16": -1.0},
    )
    _add_segmenter_script(
        out,
        "script-flush-in-speech",
        [0.0] * 5 + [0.05] * 10,
        {},
        flush_at=[15],
        flush_end=True,
    )
    _add_segmenter_script(
        out,
        "script-flush-right-after-start",
        [0.0] * 5 + [0.05] * 6,
        {"min_speech_secs": 0.0},
        flush_at=[9],
        flush_end=True,
    )
    _add_segmenter_script(
        out,
        "script-flush-too-early",
        [0.0] * 5 + [0.05] * 6,
        {},
        flush_at=[8],
        flush_end=False,
    )
    _add_segmenter_script(
        out,
        "script-flush-not-in-speech",
        [0.0] * 5,
        {},
        flush_at=[2],
        flush_end=True,
    )
    _add_segmenter_script(
        out,
        "script-reset",
        [0.0] * 5 + [0.05] * 8 + [0.0] * 40,
        {},
        reset_at=[8],
    )
    _add_segmenter_script(
        out,
        "script-max",
        [0.0] * 5 + [0.05] * 100 + [0.0] * 5,
        {"max_utterance_secs": 0.5, "silence_secs": 10.0},
    )
    _add_segmenter_script(
        out,
        "script-silence-wins-over-max",
        [0.0] * 5 + [0.05] * 5 + [0.0] * 40,
        {"max_utterance_secs": 0.12, "silence_secs": 0.06, "min_speech_secs": 0.0},
    )
    _add_segmenter_script(
        out,
        "script-after-close-resets-pre",
        [0.0] * 5 + [0.05] * 12 + [0.0] * 50 + [0.05] * 12 + [0.0] * 50,
        {"silence_secs": 0.3},
    )
    _add_segmenter_script(
        out,
        "script-probe-tail-is-267-chunks",
        [0.0] * 5 + [0.05] * 500 + [0.0] * 10,
        {"silence_secs": 10.0, "max_utterance_secs": 30.0, "quick_silence_secs": 0.2},
        probe={"mode": "true"},
        chunk_bytes=960,
    )


# ---------------------------------------------------------------------- main
def generate(out: FixtureFile) -> None:
    corpus = _corpus()
    _add_wav_cases(out, corpus)
    _add_pcm_rms(out)
    _add_pcm_is_weak(out)
    _add_pcm_to_wav(out)
    _add_resample(out)
    _add_barge_in(out)
    _add_segmenter(out)
    _add_segmenter_scripts(out)


if __name__ == "__main__":
    main_guard(DOMAIN, generate)

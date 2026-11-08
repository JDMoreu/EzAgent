"""Lógica del banco de latencia.

Qué mide y cómo:

* **VAD** en *tiempo de audio*: se alimenta el ``Segmenter`` real chunk a chunk
  sin dormir, y ``vad_wait_ms`` sale de los chunks de silencio contados. Es
  determinista y no depende de la máquina.
* **STT, LLM y TTS** en *tiempo real*, con los componentes reales o falsos
  (``--stt/--llm/--tts``).
* ``cut`` = el VAD cerró el turno antes del final de la frase (pausa a mitad).

El TTS mide solo la síntesis de la primera frase; la apertura del stream de
audio no se incluye (la elimina E2). Documentado para no confundir la cifra.
"""

from __future__ import annotations

import json
import random
import subprocess
import time
import wave
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Protocol

import numpy as np

from ezagent.voice.audio import CHUNK_SECS, Segmenter, pcm_rms, pcm_to_wav
from ezagent.voice.config import VoiceConfig
from ezagent.voice.metrics import LatencyStats, TurnTimings

RATE = 16000
CHUNK_BYTES = int(RATE * CHUNK_SECS) * 2
METRICS = (
    "vad_wait_ms", "stt_ready_ms", "first_token_ms", "first_sentence_ms",
    "first_sound_ms", "ttfa_ms",
)


class SttDriver(Protocol):
    def transcribe(self, wav: bytes, phrase: Phrase) -> str: ...


class LlmDriver(Protocol):
    def stream(self, text: str) -> Iterator[str]: ...


class TtsDriver(Protocol):
    def synth(self, text: str) -> None: ...


@dataclass(frozen=True)
class Phrase:
    id: str
    text: str
    group: str
    wav: Path | None = None


# ---------------------------------------------------------------- audio
def read_wav(path: Path) -> bytes:
    """PCM int16 mono 16 kHz; cualquier otro formato es un error claro."""
    with wave.open(str(path), "rb") as wav:
        if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate()) != (1, 2, RATE):
            raise ValueError(f"{path.name}: se espera WAV mono, 16 bits, 16 kHz")
        return wav.readframes(wav.getnframes())


def noise(seconds: float, rms: float = 0.0002, seed: int = 7) -> bytes:
    """Silencio con ruido de fondo gaussiano (nunca ceros: el VAD calibra con él)."""
    rng = np.random.default_rng(seed)
    samples = rng.normal(0.0, rms * 32768.0, int(RATE * seconds)).astype(np.int16)
    return samples.tobytes()


@dataclass
class VadResult:
    utterances: list[bytes]
    vad_wait_ms: float
    cut: bool


def run_vad(pcm: bytes, cfg: VoiceConfig, tail_s: float | None = None) -> VadResult:
    """Pasa ``pcm`` (+ silencio final) por el Segmenter real, en tiempo de audio."""
    seg = Segmenter(
        min_rms=cfg.min_rms,
        silence_secs=cfg.silence_secs,
        pre_roll_secs=cfg.pre_roll_secs,
        min_speech_secs=cfg.min_speech_secs,
        max_utterance_secs=cfg.max_utterance_secs,
        noise_multiplier=cfg.noise_multiplier,
    )
    lead = noise(0.9, seed=1)  # calibración del suelo de ruido
    tail = noise((tail_s if tail_s is not None else cfg.silence_secs + 0.6), seed=2)
    stream = lead + pcm + tail
    utterances: list[bytes] = []
    last_voiced = -1
    first_end = -1
    for idx in range(len(stream) // CHUNK_BYTES):
        chunk = stream[idx * CHUNK_BYTES:(idx + 1) * CHUNK_BYTES]
        level = pcm_rms(chunk)
        if level >= seg.thresholds()[1]:
            last_voiced = idx
        utt, _ = seg.feed(chunk, level)
        if utt:
            utterances.append(utt)
            if first_end < 0:
                first_end = idx
    final = seg.flush()
    if final:
        utterances.append(final)
    wait = (first_end - last_voiced) * CHUNK_SECS * 1000.0 if first_end >= 0 else 0.0
    return VadResult(utterances, max(wait, 0.0), cut=len(utterances) > 1)


# ------------------------------------------------------------- drivers
class FakeStt:
    def transcribe(self, wav: bytes, phrase: Phrase) -> str:
        return phrase.text


class RealStt:
    def __init__(self, cfg: VoiceConfig) -> None:
        self.cfg = cfg

    def transcribe(self, wav: bytes, phrase: Phrase) -> str:
        from ezagent.voice.stt import transcribe

        return transcribe(self.cfg, wav)


class FakeLlm:
    def __init__(self, delay_s: float = 0.0) -> None:
        self.delay_s = delay_s

    def stream(self, text: str) -> Iterator[str]:
        time.sleep(self.delay_s)
        yield "Claro, "
        yield "te escucho. "
        yield "¿En qué te ayudo?"


class _NullHerdr:
    def __getattr__(self, name: str) -> Callable[..., list]:
        return lambda *a, **k: []


class MessengerLlm:
    """El ``Messenger`` actual sin núcleo agéntico (charla; no mide herramientas)."""

    def __init__(self, cfg: VoiceConfig) -> None:
        from ezagent.voice.llm import Messenger

        self.messenger = Messenger(
            cfg, _NullHerdr(), last_summary=lambda: "", watch=lambda a: None,
            mute=lambda m: None, request_close=lambda: None,
            mic_mute=lambda m: None, runtime=None,
        )

    def stream(self, text: str) -> Iterator[str]:
        return self.messenger.ask_stream(text)


class NullTts:
    def synth(self, text: str) -> None:
        return None


class RealTts:
    def __init__(self, cfg: VoiceConfig) -> None:
        from ezagent.voice.tts import Speaker

        self.speaker = Speaker(cfg)
        self.speaker.synthesize("Hola.")  # calentamiento, como la app

    def synth(self, text: str) -> None:
        self.speaker.synthesize(text)


# --------------------------------------------------------------- turno
def run_turn(
    phrase: Phrase, pcm: bytes, cfg: VoiceConfig,
    stt: SttDriver, llm: LlmDriver, tts: TtsDriver, run: int = 1,
) -> dict[str, object]:
    from ezagent.voice.app import _Sentences

    vad = run_vad(pcm, cfg)
    row: dict[str, object] = {"id": phrase.id, "run": run, "group": phrase.group,
                              "cut": vad.cut}
    if not vad.utterances:
        row["error"] = "sin_utterance"
        return row
    vad_end = time.monotonic()
    timings = TurnTimings(speech_end=vad_end - vad.vad_wait_ms / 1000.0, vad_end=vad_end)
    text = stt.transcribe(pcm_to_wav(vad.utterances[0], RATE), phrase)
    timings.mark("stt_ready")
    sentences = _Sentences()
    first_token = False
    first_sentence = False
    for delta in llm.stream(text):
        if not first_token:
            timings.mark("first_token")
            first_token = True
        for sentence in sentences.feed(delta):
            if not first_sentence:
                timings.mark("first_sentence")
                tts.synth(sentence)
                timings.mark("first_sound")
                first_sentence = True
    if not first_sentence:
        for sentence in sentences.flush():
            timings.mark("first_sentence")
            tts.synth(sentence)
            timings.mark("first_sound")
            break
    timings.mark("turn_end")
    row.update({k: round(v, 1) for k, v in timings.report().items()})
    row["transcript"] = text
    row["wer"] = round(wer(phrase.text, text), 3)
    return row


def wer(reference: str, hypothesis: str) -> float:
    """Tasa de error de palabras (Levenshtein por palabra, sin puntuación)."""
    def tok(s: str) -> list[str]:
        return "".join(c.lower() if c.isalnum() or c.isspace() else " " for c in s).split()

    ref, hyp = tok(reference), tok(hypothesis)
    if not ref:
        return 0.0 if not hyp else 1.0
    prev = list(range(len(hyp) + 1))
    for i, r in enumerate(ref, 1):
        cur = [i]
        for j, h in enumerate(hyp, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (r != h)))
        prev = cur
    return prev[-1] / len(ref)


def summarize(rows: list[dict[str, object]]) -> dict[str, object]:
    out: dict[str, object] = {}
    for key in METRICS:
        values = [float(r[key]) for r in rows if key in r]
        if values:
            out[key] = {
                "p50": round(LatencyStats.percentile(values, 50), 1),
                "p95": round(LatencyStats.percentile(values, 95), 1),
            }
    out["cuts"] = sum(1 for r in rows if r.get("cut"))
    out["turns"] = len(rows)
    return out


def git_commit() -> str:
    try:
        return subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"], capture_output=True, text=True,
            timeout=5, check=False,
        ).stdout.strip() or "unknown"
    except (OSError, subprocess.SubprocessError):
        return "unknown"


def load_phrases(directory: Path) -> list[Phrase]:
    manifest = json.loads((directory / "phrases.json").read_text(encoding="utf-8"))
    phrases = []
    for item in manifest["phrases"]:
        wav = directory / f"{item['id']}.wav"
        phrases.append(Phrase(item["id"], item["text"], item["group"], wav if wav.exists() else None))
    return phrases


def run_bench(
    phrases: list[Phrase], cfg: VoiceConfig, stt: SttDriver, llm: LlmDriver,
    tts: TtsDriver, runs: int = 1, meta_extra: dict[str, object] | None = None,
    synthetic: bool = False,
) -> dict[str, object]:
    rows: list[dict[str, object]] = []
    for phrase in phrases:
        if phrase.wav is not None:
            pcm = read_wav(phrase.wav)
        elif synthetic:
            pcm = synthetic_speech()
        else:
            continue
        for run in range(1, runs + 1):
            rows.append(run_turn(phrase, pcm, cfg, stt, llm, tts, run))
    meta = {"commit": git_commit(), "date": datetime.now(timezone.utc).isoformat(),
            "runs": runs, "silence_secs": cfg.silence_secs, **(meta_extra or {})}
    return {"meta": meta, "turns": rows, "summary": summarize(rows)}


def synthetic_speech(seconds: float = 1.4, gap_at: float | None = None, gap_s: float = 0.0) -> bytes:
    """Tono modulado que el VAD toma por voz; opcionalmente con una pausa a mitad."""
    t = np.arange(int(RATE * seconds)) / RATE
    wave_ = 0.12 * np.sin(2 * np.pi * 180 * t) * (0.6 + 0.4 * np.sin(2 * np.pi * 4 * t))
    pcm = (wave_ * 32767).astype(np.int16)
    if gap_at is not None and gap_s > 0:
        a = int(RATE * gap_at)
        gap = np.frombuffer(noise(gap_s, seed=3), dtype=np.int16)
        pcm = np.concatenate([pcm[:a], gap, pcm[a:]])
    return pcm.tobytes()


def write_report(report: dict[str, object], out: Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
